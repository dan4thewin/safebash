# 382 — SB5084, and a check that could not diverge

DG, on the item I left open yesterday:

> yes to proven negative as compile-time error

So a negative shift count the constant folder can prove is now **SB5084**,
alongside SB5077 for a proven zero divisor. The interesting part was not the
new diagnostic — it was what building it correctly turned out to require.

## The temptation, and why I didn't

The obvious implementation is to copy `walk_arith_divisors` (a 34-line
recursive walk over every `ArithExpr` variant), change two match arms, and
report a different code. Ten minutes, done.

That copy would have been a bug waiting to happen, and I can say exactly what
kind. The walk is not incidental — it *is* the reach of the check. Two copies
means two definitions of "what the folder proves", drifting the moment
someone adds an `ArithExpr` variant and updates one of them.

So the walk got generalised to `walk_arith_binops` (yield `(op, right)` for
every binary operator) and the per-operator decision moved into one
classifier, `const_arith_fault`. The two faults are now filters over a shared
traversal.

## What that bought, concretely

Two behaviours I did not have to decide, because the shift check inherited
them:

**Compound assignment.** Is `x <<= -1` covered? I nearly wrote an arm for
`CompoundAssign`. Before doing so I checked `--emit-ir`, and the parser
desugars it:

    x <<= 1  →  Assign { var: x, value: BinOp { op: Shl, … } }

`CompoundAssign` never survives to validate on this path. An arm for it would
have been dead code that *looked* load-bearing. The clue was already on
screen: `(( x /= 0 ))` reports SB5077 today, even though the divisor walker
has no compound-assign arm. That inconsistency was the question, and the
answer was the desugar.

**Dead ternary branches.** `(( x ? 1 : 8 >> -1 ))` with `x` always 1 — the
faulting branch never runs. Error, or not?

I did not have to reason about it. `(( x ? 1 : 5 / 0 ))` has always been
SB5077, so the precedent existed; sharing the walk means the shift check
could not have diverged from it even by accident. DD#25's own phrasing — "a
property of the expression as written" — is the justification, and it is
right: this is a claim about the program text, not a reachability analysis.

Had I copied the walker, that would have been a fresh decision I'd have made
implicitly, in a direction nobody reviewed.

## The reach, measured

| case | fault |
|---|---|
| `8 >> -1` literal | SB5084 |
| `8 >> (1 - 2)` folded through parens | SB5084 |
| `x >>= (1 - 2)` compound | SB5084 |
| ternary branch never taken | SB5084 |
| `int n=-1; x >> n` | runtime trap |
| `8 >> 64` | **not a fault** — 0 (or the sign) |

That last row matters. A count of 64 is now a legitimate answer, not an
error; only a *negative* count is meaningless. Yesterday's ruling and today's
are different questions about the same operator, and the specs say so
separately.

## One stale comment, caught by rereading my own work

Spec `25.17`, written yesterday, said a literal negative count "declines and
defers to the runtime trap". Half of that is still true — the folder does
decline — but validate now rejects the program before the runtime is
reachable, so the sentence describes a path that no longer exists. Rewritten.

This is the second time in three sessions that the wrong thing in the tree
was a *comment* rather than code (DD#25's stale gap list was the first). A
comment that describes an old path is worse than none: it is confident, and
it reads as documentation of intent.

`25.15` got a sentence too, naming why it is the runtime half — its count is
a variable, so the folder cannot see it. The division of labour between the
two checks is now stated where someone editing either one will read it.

## Result

Sweep 5847, zero failing, zero flips. Specs `25.18`–`25.21`.
`validate.rs` is net **+21 lines** for a second fault check — which is roughly
what it should cost when the first one was built well.
