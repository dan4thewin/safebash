# 409 — The function that ran once

*2026-09-25. Continuing the coverage queue. DG: "continue with coverage."*

## The clue was a count, not a gap

The queue had gone flat — four to six uncovered lines per function, mostly ICE
guards. So instead of reading the ranking top-down I filtered the uncovered set
to lines that are *not* `unreachable!`/`panic!` arms, and one line stood out:

```
sash/src/codegen/arithmetic.rs:259    builder.ins().bor(l, r)
```

That is the `||` half of `emit_logical_and_or`. Its sibling two lines up —
`band` — was covered. So the function ran, but only ever with `And`. Checking
the hit count made it worse: `emit_logical_and_or` executed **once** in a sweep
of 6344 spec programs, and the C mirror's `Or` arm (`emit_c.rs:1654`) had
**zero** hits.

One execution across the whole suite is not "thinly covered". It means
arithmetic `||` had no native coverage at all, and `&&` had a single witness.

## Probing the values found nothing

`echo $(( 1 && 0 ))`, `$(( 0 || 3 ))`, and the rest all matched bash. Every
truth-table entry was right. If I had stopped at "the values agree" — which is
what the uncovered line literally asked about — I would have added a truth-table
spec and moved on, and the bug would have survived.

What stopped me was the function's own doc comment:

> Short-circuit-free `&&` / `||` (operands are already evaluated)

"Operands are already evaluated" is a true statement about `emit_arith_op` — it
receives Cranelift values, so by the time it runs there is nothing left to skip.
But it is also a confession. If the operands are always evaluated, then
`&&` is not bash's `&&`.

## The bug

```sh
d=0
echo $(( d != 0 && 10 / d > 1 ))
```

bash prints `0`. sash aborted with `division by zero`, rc 2. The guard idiom —
*test the divisor, then divide* — did not work, in either backend.

| shape (d=0) | bash | sash |
|---|---|---|
| `echo $(( d != 0 && 10 / d > 1 ))` | `0` | abort, rc=2 |
| `echo $(( d == 0 \|\| 10 / d > 1 ))` | `1` | abort, rc=2 |
| `echo $(( d != 0 && 10 % d ))` | `0` | abort, rc=2 |
| `if (( d != 0 && 10 / d > 1 ))` | `no` | abort, rc=2 |

Per the bug policy this stopped everything else.

## It had a twin, already fixed

Before designing anything I checked `notes/decisions/` for a DD making eager
evaluation deliberate. Nothing. What I found instead was
`notes/tasks/native-ternary-eager-eval.md`, **status: done**, journal 193 — the
identical bug class for `a ? b : c`, fixed by lowering the branches into real
control flow rather than evaluating both. So the approach was not an open
design fork; it was an established pattern in this codebase, and the fix is its
sibling.

`lower_ternary_int` became the template on the native side and
`emit_ternary_as_int_c` on the C side.

## Two backends, two different root causes

**Native** was the obvious one: `emit_arith_op` takes values, so the fix cannot
live there at all. It has to live at the caller that lowers the operand *tree*.
`lower_logical_short_circuit` lowers the left operand, compares it to zero,
and branches to an `rhs_block` or straight to a `merge_block` carrying the
short-circuit constant as a block argument (`And` → 0, `Or` → 1). The right
operand is lowered **inside** `rhs_block`, so its guards go with it.

**C** was the interesting one, because C's `&&` already short-circuits. Writing
`(l && r)` looks like it should be correct for free. It is not:
`lower_value_as_int` emits the div-by-zero check and the overflow trap as
**statements** preceding the expression it returns. The statements ran
unconditionally; only the arithmetic was skipped. Exactly the same trap the
ternary fell into, and exactly why `--emit-c` is only an oracle if both sides
are written with this in mind.

## The case that looks like a bug and is not

A function call in a short-circuited operand still runs:

```sh
echo $(( 0 && f() ))    # f still executes
```

`lift_int_func_calls` (`transform/functions.rs:222`) hoists calls out of
`ArithBinOp` *and* `Ternary` before lowering, so they are evaluated ahead of the
branch. My first instinct was that this is the same bug one level down.

It is not, and the test is the invariant: **bash does the same thing.**

```sh
echo $(( 0 ? $(echo EAGER >&2; echo 1) : 2 ))   # prints EAGER
```

Bash's command substitution inside arithmetic is itself eager. So the hoist
matches bash, the ternary has always shared this property, and there is nothing
to fix. Worth writing down precisely because it *looks* wrong.

Assignments inside `$(( ))` are separately rejected (SB4020), so the remaining
side-effect class is exactly the guards — div/mod-by-zero and the overflow trap
— which is what the fix addresses.

## Specs

`31.129`–`31.136`, checked free against the whole suite first. Four are the
divergence shapes above. The other four exist because a fix that *never*
evaluated the right operand would also pass those four — `31.133`/`31.134`
force the RHS to be needed, and `31.135`/`31.136` walk the truth table with both
operands runtime-valued.

Non-vacuity proven by neutering each fix in turn: exactly `31.129`–`31.132`
fail, in each mode, and the truth-table four keep passing. That is the right
split — it shows the new specs test the laziness and the old behaviour was
genuinely wrong.

## The dead emitters

Fixing at the caller left `emit_logical_and_or` and the C `And`/`Or` arms
unreachable. `emit_arith_op` has three callers, all in `values.rs`: two are the
`SsaValue::ArithExpr` arm, and that node is constructed at exactly four sites
(`transform/pipelines.rs` 102/141/240/284), every one of them a literal
`ArithOp::Add` — it is a special-purpose "loop index += 1" node, not a general
arithmetic carrier. The third caller is the `ArithBinOp` arm, now guarded.

DG's call was to delete them and use `unreachable!`, matching the precedent
already sitting in the same `match` (`unreachable!("comparison ops handled by
comparison_intcc fast path")`). The deciding argument was that option A —
leaving them as "correct-but-cold fallbacks" — leaves a trap: the code they
contain computes the **eager** answer, which is the bug. Anything that reached
them again would silently reintroduce the divergence, and it would look like
working code while doing it.

A green sweep only proves *not reached by these inputs* (journal 386), so the
`unreachable!` rests on the structural argument above, not on the sweep. The
sweep is corroboration: 6360 spec programs, none fired it.

## Gates

6360 (predicted exactly: 6344 + 8 × 2), zero failing, zero flips against
`cc5f7b0`. `mk test` 106 passed. fmt and clippy clean; every touched function
clears the complexity gate against a 3.85 / 1.88 mean.

## What to take from it

The coverage number that mattered was not zero. `emit_logical_and_or` was
*covered* — one hit, `And` only — and a summary that reports files or even
functions would have shown it green. The finding lived in the **execution
count of a single line**, and what turned it from a gap into a bug was reading
the doc comment next to it and taking its wording literally.
