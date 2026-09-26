# 418 — The only int-typed test

DG: "`-t` should cease to be a `[[ ]]` test."

It is retired. `-t` joins `-v`, `-o` and `-R` in
`unsupported_unary_test_hint` — the four operators the parser accepts and
validate rejects with SB5028. They were already neighbours in
`is_unary_test_op` (`parser.rs` 1766), which is why the error is SB5028 and
not SB4014: the parser must keep recognising `-t` as a unary test operator,
or `[[ -t 1 ]]` degrades into two words and reports the wrong thing. The
rule is that the *parser* stays permissive and *validate* is where a
retirement is stated.

## Why it was the odd one out

The `[[ ]]` unary family is otherwise homogeneous: string shape (`-n`, `-z`)
and file predicates (`-e`, `-f`, `-d`, `-r`, `-w`, `-x`, …). Every one of
them takes a string. `-t` took an **fd number** — it was the sole int-typed
unary test, and it sat alone against that family. Its codegen said so
plainly: `emit_isatty_test` was the only unary arm that had to call
`atoi_call` on its operand before it could do anything.

And the question it answers is an interactivity question. A compiled static
binary has little business asking whether fd 1 is a terminal.

## `terminal?` was considered and dropped

DG raised a predicate spelling — `if $(terminal? $fd) { … }` — and then asked
whether there are any unary predicates among the arithmetic ops. There are
not. `IrUnaryArithOp` is Neg / Pos / Not / BitNot; only `!` is a predicate at
all, and every sexpr intrinsic is value-producing. `terminal?` would have
been the first of its kind, in a language that already has a predicate
context (`[[ ]]`) which it would have been deliberately declining to use.

DG's call: **YAGNI.** That is the whole justification, and it is a better one
than a new category would have been.

## What came out

Removing a feature is worth more than adding one when the removal is total,
so this went all the way down:

| layer | removed |
|---|---|
| `codegen/tests.rs` | the `"-t"` dispatch arm and `emit_isatty_test` |
| `codegen/setup.rs` | the `sash_isatty` extern declaration |
| `codegen/types.rs` | `ExternFuncs::isatty` |
| `emit_c.rs` | the `"-t"` arm and `extern i64 sash_isatty(i64)` |
| `sash-rt/src/lib.rs` | `pub extern "C" fn sash_isatty` |

The compiler told me where to stop. Deleting the dispatch arm left
`atoi_call` unused in `tests.rs` and rustc said so — that import was there
*only* for `-t`, which is the same observation as "it was the only int-typed
test", arriving from the other direction. Dropping the runtime symbol is
decide-and-report under the decision tree: reversible, and nothing can call
it now that both backends have stopped emitting the call.

## The proof

Neutering the `-t` arm in `unsupported_unary_test_hint` fails exactly
`62.22`, one per mode. The failure mode is the interesting part:

```
thread 'main' panicked at sash/src/codegen/tests.rs:194:21:
ICE: unimplemented unary test operator: -t
```

That doc comment has claimed for a long time that these operators "must be
rejected here (SB5028) before codegen ICEs on them". The neuter is the first
time anything demonstrated it. The claim was true.

`cov.89` retires with the mechanism it covered. It asserted
`emit_isatty_test` returned `notty` for a non-tty harness stdout; there is no
`emit_isatty_test`, so there is nothing for it to cover, and keeping it as a
rejection test would only have duplicated `62.22`. The sweep announced
`c:cov.89` and `native:cov.89` — a retirement I predicted, which is the only
kind that is not a finding.

Net spec count is unchanged: one dual-mode spec out, one dual-mode spec in.
That makes the *total* useless as a signal here, so the sharp prediction was
the purge naming exactly those two ids. It did.

## Gate

- sweep **6510** (native 3355, c 3142, skipped 13) — predicted exactly
- zero failing, zero flips vs `a875d58`
- `mk test` exit 0
- both `chk` collision checks clean; `cargo fmt` / `clippy` clean
- `coverage.sh` plan 339 → 338 keeps its known `plan − 2` offset (the
  variable-id loop), so the one legitimate mismatch stayed legitimate

## Addendum — DD#89 closes

With `-t` retired, DD#89's "Left open" section had nothing left open. It
asked whether `exit $s` / `return $s` *should* reject, and tracked four sites:
`exit`, `return`, `wait`, `[[ -t ]]`. All five (counting `$(arg $s)`, which it
listed separately as "untested rather than settled") are now closed, and the
uniformity is the point: **every one closed by rejection, none by coercion.**

| sink | was | now | closed by |
|---|---|---|---|
| `exit $s` | atoi → 0 | SB4030 | `a875d58` |
| `return $s` | atoi → 0 | SB4030 | journal 411 |
| `wait $s` | accepted | SB4030 | `a875d58` |
| `$(arg $s)` | **argv[0]**, silently | SB4030 | `a875d58` |
| `[[ -t $s ]]` | accepted | SB5028 | this commit |

Two of those entries are worth re-reading against what the DD predicted.

`$(arg $s)` was recorded as "masked by SB5020 — taint fires first — so it is
untested rather than settled". That was the honest reading, and refusing to
mark it closed is exactly what left it to be measured properly later. It did
not coerce to 0 like its neighbours; it silently returned **argv[0]** — a
real, plausible-looking value with no diagnostic. The most dangerous of the
five, recorded as the least known.

`[[ -t ]]` was the site the DD itself floated removing rather than tightening
("DG has also raised whether `-t` should stop being a test operator at all …
Not decided"). That is what happened, and it is the only one of the five that
needed no rule.

Also corrected: the rule list still said `exit $s` "does still coerce today",
true when written and false since `a875d58`; and `docs/sash.1`'s `-d`/`-f`
collision table still listed `-t` as "is a terminal" in the `[[ ]]` column.
The man page's "table wider than line width" warning is pre-existing and
untouched.
