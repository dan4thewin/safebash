---
title: "A cold diagnostic, a shadowed one, and eight runtime ICEs"
date: 2026-09-22
---

# 389 — A cold diagnostic, a shadowed one, and eight runtime ICEs

Coverage was regenerated after `b4549aa` (the Sep-21 database was two commits
stale and my own edits had shifted line numbers across five files). Fresh
numbers: **96.61% lines, 98.35% functions, 90.80% branches**.

Three findings, and the first one was mine.

## 1. SB4064 shadowed SB5025 — a regression from the commit before

`check_argv_splat_decl` showed nine cold lines, all of them the *body* of its
SB5025 diagnostic. That function is code DD#117 had just touched, so the cold
lines were suspicious in a specific way: had my change made the diagnostic
unreachable?

It had. `array a=$(argv 1)` is a scalar initializer in a collection
declaration, so SB4064 matches it — and SB4064 runs in
`run_structural_checks`, a phase that **exits** before
`run_semantic_checks`, where SB5025 lives. Proved by execution rather than
by reading: with the `check_decl_value_shape` call commented out, the same
input reports SB5025 again.

The two messages are not interchangeable. SB5025 says *use the out-param form
`argv N a`*. SB4064 says *write `a=(…)`* — and that advice is actively wrong
here, because `array a=($(argv 1))` is itself rejected (SB5021/SB5025). So
SB4064 now defers for exactly this shape. One authoritative site for the
message, no rival.

The general lesson is about phase ordering: **a new check in an earlier phase
silently steals every input it matches from the later phases.** Nothing fails,
because the program is still rejected. Only the message changes, and only for
inputs nobody has a spec for.

## 2. `attest`'s two cold sites — DD#116, and the fallback was a rival

`handle_attest_stmt` had eight cold lines across two sites:

- `let AttestPredicate::Test(test) = pred else { return block; }` — an
  `ArithConvert` predicate in a non-scalar form. Silently dropped the whole
  statement.
- the tail fallback: "no TestExpr — emit as regular command".

`AttestPredicate` has exactly two variants and `classify_attest_convert`
only ever pairs `ArithConvert` with a single-name lead, so the first looked
structurally impossible. That is the same call-site argument that has now
been wrong twice (journals 386, 388), so it was settled by execution instead:
two `panic!`s, one build, twenty-odd shapes including every non-statement
position I could construct — pipeline stage, command substitution, `&&` tail,
`!`, redirected, function body, `if`/`while` condition, `for` body. Zero hits.

`attest` is a lexer keyword (`Tok::Attest`) with a dedicated production, so
every occurrence in statement position goes through `parse_attest` and a
malformed one is SB4014 *at parse time*. Quoting the name (`"attest" x`)
makes it an ordinary command and SB5011 takes it. There is no way in.

Applying DD#116:

- The predicate site has **property 2** available and was taking none of it.
  It is now a total `match` over `AttestPredicate`, so a third variant is a
  compile error here rather than a silently dropped `attest`.
- The tail fallback is a **rival** — a second, silent answer to "what is a
  well-formed `attest`?", whose authoritative answer is `parse_attest`. It
  has none of the three properties. Deleted, per DD#116.

Deleting it left `name` unused, which is its own small verdict on how much
that fallback was contributing.

## 3. `argv` — eight spellings that compiled clean and ICEd at runtime

This one came out of taking SB5025's own advice literally. Its message reads
`argv [N] a`, where the brackets mean *optional*. Written verbatim:

```
$ cat p.sh
argv [1] a
echo $(len a)
$ sash -o p.bin p.sh && ./p.bin x y z
sash: bug: bare command passed to sash_exec (expected absolute path)
p.bin: argv: command failed (exit 127)
```

Not a diagnostic — a clean compile and an internal `sash: bug:` at runtime.

The mechanism is the same shape as the `attest` fallback deleted above.
`try_handle_argv` returned `Option<BlockId>` and yielded `None` for any shape
it could not lower; `None` in `try_builtin` means *"not a builtin"* (it is the
legitimate `_ => None` path for every builtin handled downstream), so the
statement became an ordinary command named `argv`.

Eight spellings reached it:

| spelling | why it fell through |
|---|---|
| `argv` | `args.last()?` on an empty list |
| `argv 1 2 a` | arity > 2 |
| `argv [1] a` | index not an integer |
| `argv -1 a` | negative index filtered out |
| `argv 1.5 a` | index not an integer |
| `argv 99999999999999999999 a` | does not fit `i64` |
| `argv "1" a` | **quoted** |
| `argv '1' a` | **quoted** |

The last two are the interesting ones. The index is read with
`expr_to_string`, which is a **display** renderer — it re-emits `"1"` as
`"\"1\""` and `'1'` as `"'1'"`. Using it for semantic extraction means a
quoted index can never parse. Quoting a numeric argument is an entirely
ordinary thing to write.

`argv` was the only fill builtin with no shape check: `readline` has SB5057,
`signal` SB4059, `match`/`matchall` SB4045, and `argv`'s own *target* has had
SB5063 since the `av.rej4`/`av.rej5` work — which fixed precisely this bug
class for the target ("a quoted target silently wrote nowhere") and left the
index and the arity untouched.

`check_argv_form` now covers both: SB4045 for arity, **SB5085** (new) for the
index, which must be a bare non-negative decimal literal that fits `i64`.
With the shape guaranteed, `handle_argv` returns `BlockId` rather than
`Option` and states its bound with `unreachable!` — every path to lowering
runs `run_semantic_checks` first (`lib.rs:176`, before `transform_or_exit` at
`:183`), including the `--emit-c`/`--emit-ir` paths, so the assert is sound.

Rejecting a **sigilled** index is a narrowing by default rather than a
considered answer — `ArgvFill` carries a constant `start`, so there is nowhere
to put a runtime value. Filed as `notes/tasks/argv-dynamic-index.md` for DG.
The current behaviour is safe either way.

## Method

The thing that found all three was refusing to close an item on a call-site
argument. Twice now the argument has been sound in form and wrong in fact.
The cost of instrumenting is one build; the cost of believing the argument
was, in this case, eight spellings of memory-safe-but-broken code shipping.

Second: **probe the family, not the cold lines.** The SB5025 shadowing was
found by asking why a function I had just edited had cold lines at all, and
the `argv` bug was found by taking a *diagnostic message* literally rather
than by reading any cold line. Neither was on the queue.

13 specs: `av.f1`-`av.f10` (argv form), `decl.21`-`decl.23` (SB4064 defers
to SB5025, with a control that it still fires for a non-argv command
substitution).
