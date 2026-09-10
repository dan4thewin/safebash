# 261 — the operand slot: one defect wearing three faces

Resuming the backlog prune at `bare-collection-name-operand-asymmetry`, I
went to re-measure the asymmetry at HEAD and typed `len "hello"` almost by
reflex. It printed `0`.

That is the most idiomatic spelling in shell, and it silently produced a
wrong answer. Pulling on it turned up three faces of a single defect:

| spelling | before | should be |
|---|---|---|
| `len "$s"`, `len ${a[0]}`, `len "lit"` | `0`, silently | 5 / element length |
| `join $a ","` | empty output, and `a` not even counted as read (SB5050 fired) | the joined string |
| `keys @m`, `keys $m` | **ICE**: `undefined arrlen: ${marrvals}` | the map's keys |

## One root cause

`try_len_intrinsic` derives a variable *name* by calling `expr_to_string` —
which is a **source-text pretty-printer**, not a name resolver. It strips one
leading `$` and hands back whatever it rendered. So `"$s"` stringifies to
`"$s"` *including the quote characters*, which is then looked up as a variable
name, misses, and reads as nothing. `len $s` works only by accident: the
formatter happens to strip the sigil.

The validator was supposed to catch exactly this. `len_operand_name` only
matches `ExprKind::Literal`, and its `else { return }` carries the comment
*"resolved elsewhere"*. They are not resolved elsewhere. That comment was the
bypass. `join_operand_name` has the same shape — `_ => String::new()`, then
callers bail on the empty string.

Not deliberate: DD#75 §2 says `len` of an unknown name is a *compile error, no
runtime garbage*. There was zero spec coverage for any quoted operand.

## The fix, and what it is not

New `check_operand_spelling` pass emitting **SB5062**, rejecting exactly the
broken cells and naming the variable in the suggestion:

    error [SB5062] `join` resolves its operand as a variable name, and does not
    resolve this spelling. Use a collection name (`join a SEP`) or a splat
    (`join @a SEP`)

I first tried to put this in `type_check.rs` and backed out: every `TypeError`
is emitted as SB4030 at `lib.rs:247`, so it could not carry its own code.

This is rejection, not repair. `join $a` is now rejected while `len $a` still
works — a real inconsistency, left deliberately. DG's pending decisions (`len`
→ collection-only plus a new `strlen`, and the collection-parameter design)
rewrite this exact surface; encoding a guess about the final spelling now
would just have to be undone.

## Why the spellings differ at all

Chasing "just unify on `@a`" ran into the wall that explains the whole family:
**sash has no first-class collection values.** `@a` is a splat into N values,
not a handle to the array. So `keys` genuinely cannot take it — a splat carries
values, and values have no keys. The accepted sets differ because the builtins
need different things, not because someone was careless.

That same absence is the root of DG's parameterised-function problem: a
function can read *and mutate* a caller's array, but only by hard-coding its
name. You can parameterise over a collection's **contents** (splat, read-only);
you cannot parameterise over its **identity**.

The way out is cheap, and it is cheap for a reason I had to correct myself on.
I first told DG this was a "much larger change" — but I was costing a *runtime*
collection value. sash **inlines every function** (SB5037/DD#27 §5 rejects
recursion precisely because there is no runtime call stack), so a collection
parameter can be a compile-time name binding. Monomorphisation. No runtime
handles, no aliasing, no taint-through-pointers.

## The fourth face

The control spec — pinning every spelling that already worked — failed in
janet, and that turned out to be the most interesting result of the session.

| form | native / c | janet |
|---|---|---|
| `len a`, `keys m`, `join @a` | ✓ | ✓ |
| `len $s`, `len $a`, `len @a` | ✓ | compile error: unknown symbol `$s` |
| `join @a ","` | ✓ | bad slot #1, expected string, got array |
| `join a ","` | `x,y,z` | **execs /usr/bin/join** |

So **no single spelling works uniformly across all three backends** — a fact
that belongs in the spelling design, not after it.

Most of this was already filed in `janet-join-intrinsic-gaps` back in August,
including the coreutils fallthrough; only the `$(len $x)` command form was new,
and it went on as an addendum. All of it is pre-existing, verified against the
pre-change build. The cause is the one that file keeps circling: `emit_janet`
renders from the **AST**, so it never sees the operand resolution that native
and c inherit from SSA.

DG deferred the janet overhaul wholesale, including a guard for the
fallthrough. Guarded 75.38 to `native,c`, following the convention that task
already set.

The `/usr/bin/join` case still deserves a name: a backend that silently execs
an external binary when it does not recognise a builtin is not a feature gap,
it is a safety property. Filed, not forgotten.

## Result

8 specs, 75.31–75.38, fail-first verified (7 red, control green) before the
fix. Sweep: **4639 passing / 40 failing / 459 skipped, zero regressions**
against `272b977`.

## Addendum — the fifth face, found by asking a design question

DG's response to the above was not "which spelling wins" but something
better: *there ought to be a sigil that means this is an identifier, not a
string.* `@a` looks like it, but it is the splat — a value operation over
collections, so it cannot name a scalar.

Checking that reading turned up a fifth face, and the worst of them:

```sash
array a=(old)
argv 1 "a"          # compiles clean, no diagnostic
echo $(len a)       # -> 1, "old". The argv values never arrive.
```

`argv N "a"` and `matchall S P "out"` accepted a quoted target and **silently
wrote nowhere**. The bare form `argv 1 a` taints the array and demands an
`attest` (SB5020); the quoted form performs no write, so the attest obligation
disappears along with it. Not a taint escape — no tainted data flows in — but
the program runs on stale data with the diagnostic that would have caught it
suppressed.

## Why a sigil, argued from the evidence rather than taste

The measurement that makes the case: the language already has ~7
name-position slots, and "must be a bare name" has been enforced **ad hoc,
three separate times, with three unrelated diagnostics** — and two slots were
missed entirely.

| slot | non-name operand | enforced |
|---|---|---|
| `read ARR`, `readline v` | SB5057 | ✓ DD#99 |
| `attest x in a` | SB4014 | ✓ |
| `len` / `join` / `keys` | SB5062 | ✓ (this session) |
| `argv N a` | silent no-op | ✗ → SB5063 |
| `matchall S P ARR` | silent no-op | ✗ → SB5063 |

SB5057's own wording already states the concept exactly: *"the target must be
a bare name — it is a binding, not a value."* So "name" is a real category the
language keeps rediscovering one slot at a time. Every rediscovery has cost a
bug. A sigil promotes it from a validator rule someone must remember to write
into something the parser knows.

The codebase had even written the inventory down already — `command_writes_args`
enumerates `read`/`readline`/`matchall`/`argv`/`attest` targets, for the
unrelated purpose of checking whether a builtin writes the reserved `args`.
The list existed; the enforcement did not follow it.

## Free characters, measured not guessed

`&` is background, `=` is assignment, `@` is the splat, `#` `*` `?` `'` are
taken. Currently free as a leading character: `:` `%` `^` `~` `+` — each
presently lexing as an ordinary word (`echo ^a` prints `^a`), so any choice
costs a little churn.

`:` is the strongest semantic fit — in Janet (the backend), Clojure and Ruby,
`:foo` means *symbol, not string* — but it collides with the `:` builtin and
the `@{a:DISP}` disposition separator. `^` is unused anywhere.

The fork that actually matters is not the character: the sigil must be a
**syntactic marker resolved at compile time**, never a runtime value. If `&a`
could be stored, DD#6's rejection of `${!var}` indirect expansion would stop
meaning anything. Inlining makes the compile-time reading free — which is the
same property that makes collection parameters cheap.

DG chose: fix `argv`/`matchall` now, enumerate every name slot before picking
a spelling. Marker placement and character both deferred to that inventory.

## Addendum — the enumeration, and what it caught

DG's call: fix `argv`/`matchall`, then enumerate every name slot before
choosing a sigil. The enumeration was not bookkeeping. It found three more
defects, one of them mine.

**`match S P "o"` ICEd** — `ICE: undefined array: "o"`, the quoted name
carried all the way to codegen. `match` is `matchall`'s sibling: DD#63 gives
it both a two-arg boolean form and a three-arg out-array form. I had fixed
`matchall` and not looked next door.

**`unset "a"` bypassed a deliberate rejection.** `unset a` on a scalar is
rejected on purpose (SB4030, *"use a=\"\" to clear"*). The quoted spelling
compiled clean and silently did nothing — `type_check.rs:1186` reads
`if name.contains('[') || name.is_empty() { continue; }`, and a quoted operand
yields the empty name. The *third* instance of the same shape as
`flag_len_cmdsub`'s `else { return }` and `join_operand_name`'s
`_ => String::new()`: an unreadable operand treated as *nothing to check*
rather than as *not a name*.

**And a regression I had just introduced.** My SB5063 took
`c.args.last()` unconditionally, so `matchall S P` (wrong arity) started
reporting SB5063 instead of SB4045's much better *"the out-array is required:
every match is appended, so there is no single value to return"*. Caught by
probing arities rather than trusting the green sweep — no spec covered the
two-arg form, so nothing would have told me. The target is now only inspected
at the arity that has one.

## The inventory

| slot | kind | non-name operand | enforced by |
|---|---|---|---|
| `read ARR`, `read $s ARR` | binds | SB5057 | DD#99 |
| `readline v` | binds | SB5057 | DD#99 |
| `argv [N] a` | binds | SB5063 | this session |
| `matchall S P a` | binds | SB5063 | this session |
| `match S P a` (3-arg) | binds | SB5063 | this session |
| `unset a`, `unset a[i]` | unbinds | SB5063 | this session |
| `attest x in a`, `attest k v in m` | reads | SB4014 | — |
| `len a`, `join a S`, `keys m` | reads | SB5062 | this session |
| `map m = @{a:DISP}` | reads | SB5033 | — |

Nine slots. Four were silently wrong until this session; one crashed the
compiler. Correctly *not* name slots, verified rather than assumed: `for x in
"a"` (a value list), `getenv "HOME"` (an environment name — a different
namespace, where a string is right), `arg N`.

## A separate bug the probing turned up

Chasing an odd link failure: `sash prog.sash -o u` **failed to link**, and so
did `-o ut`, while `-o out` and `-o zz` were fine.

`lib.rs` built the flag as `format!("-o{}", output)`. `ld` uses
`getopt_long_only`, so the glued `-ou` is matched against single-dash *long*
options — and `ou` is an unambiguous prefix of `--out-implib`, which takes an
argument, so ld consumed the object file as the implib name. The link then had
the runtime but not our code: *undefined reference to `main`*.

Nothing to do with names or sigils; found because a scratch binary happened to
be called `u`. Fixed by passing `-o` and the name as separate arguments, which
is what should have been there all along. Spec `cli.7`.

## Where the sigil argument stands

Nine slots, six diagnostics, four of them added only after a bug. The pattern
is not that anyone was careless — it is that "this operand is a name" has no
representation in the language, so every slot re-derives it and each
re-derivation is a chance to forget. That is the case for the sigil, and it is
now measured rather than asserted.
