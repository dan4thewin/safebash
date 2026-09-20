# 379 — The compiler's own names, written by a user

Cold-line queue: `BuiltinCmd::from_name` in `ssa.rs`. Nine cold arms, all
`__`-prefixed — `__negate_test_result`, `__table_delete`, `__subshell`, and
six siblings. These are the compiler's internal opcode spellings.

## Why they were cold

The reverse map, `sigil_str`, is **hot**. That is the whole finding in one
line: the compiler builds these by *enum* and spells them with `sigil_str`, so
`from_name` never parses them back. The string→enum direction had no producer
inside the compiler at all.

Which leaves exactly one way to reach those nine arms: **a user writes the
name.** `from_name` classifies raw command names from source.

## The question that mattered

Not "can I reach the line" but "what happens when someone does". If
classification beat the user's own definition, then

```sh
__table_delete() { echo hi; }
__table_delete
```

would lower to a *table delete* instead of calling the function. A
compiler-internal namespace silently capturing a user name is the shape of
bug an adversarial audit looks for.

Probed all nine, in five positions, across bash / native / C. **All correct** —
the function wins every time, a bare sigil name with no definition is an
ordinary `SB5011 command not found`, and args, exit status, and pipeline
stages all behave. Nothing to fix.

But nothing spec'd it either. Four specs now do (`cov.327`–`cov.330`), and
`cov.327` defines and calls all nine in one program, so the nine cold arms
close together.

## The second thread: an op that is checked, and four that are not

The other cold line was `CheckedArithOp::from_arith`'s `_ => None`. Both
callers guard with `ArithOp::Add | Sub | Mul` and then `.expect(…)`, so the
`None` is unreachable *and already has a loud failure* — there is nothing to
convert and I left it alone. Not every cold line wants an edit.

The useful part was the question it prompted: **which arithmetic ops can
overflow but are not in that set?** So I probed the lot.

`/` traps. `%` on `i64::MIN % -1` is `0` and correctly not a fault. `**`
traps. Unary `-` on `i64::MIN` traps. `<<` masks the shift count and matches
bash on every spelling I tried.

## The finding: DD#25's gap list had rotted

DD#25 closes with a "Known gap (follow-up)" paragraph:

> `Pow` saturates rather than traps; `Shl` (shift ≥ 64) and unary `Neg`
> (i64::MIN) are not yet trap-with-message.

Two of those three were false. `Pow` traps — and spec `25.9`, which pins it,
is annotated *"(was: saturated to i64::MAX silently)"*. `Neg` traps — spec
`25.11` says *"(regression: -x → 0 - x → ssub_overflow)"*. The fixes landed
**with specs recording that they were fixes**, and the DD was never updated.

The same paragraph also still described the janet backend's disposition.
Janet was removed by DD#104.

A "follow-up" that is already done is worse than no note at all: it is a work
item someone picks up and finds finished. This is precisely the rot the native
backlog review named — *a filing is a snapshot of a moving tree and nothing
ages it* — except here the moving tree was the DD, not the task.

I rewrote it as a table of measured behaviour, one row per op, each citing its
spec.

## What I did not decide

`Shl` is the one real remainder, and I am not treating it as a gap to close.
It does not overflow: x86 masks the count to 6 bits, so `1 << 64` is `1 << 0`.
sash already agrees with bash exactly — `1 << 63` is i64::MIN, `1 << 64` is
`1`, `1 << 200` is `256`. Making it trap would be a **deliberate divergence
from bash**, which is DG's call and not something to pick up off a follow-up
list.

What I did instead was pin the current answers in `25.13`, so adopting a trap
has to be a visible decision rather than a silent one. That is the useful
thing a spec does for an undecided question: it does not settle it, it makes
sure nobody settles it by accident.

## Result

Sweep 5815, zero failing, zero flips. Predicted 5815 before running.
