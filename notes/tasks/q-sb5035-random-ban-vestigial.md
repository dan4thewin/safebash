---
title: "SB5035 (`$(random)` in a `:-` default) justifies itself by eagerness that no longer applies"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on whether SB5035 still earns its place"
---

## The premise, and why it may have expired

SB5035 rejects `$(random)` as a `:-` disposition default. The stated reason is
evaluation order: the default is materialized **eagerly**, so it "would advance
the PRNG whether or not the fallback is taken" — an observable side effect on a
path the programmer did not take.

`62bd809` made `:-` lazy when the operand contains a capture. `$(random)` is a
capture. So on today's compiler the operand is evaluated only when the fallback
is actually taken, and the argument above describes behaviour that no longer
happens.

Not proposing removal — measuring first is the whole point of the filing. But
the ban should either acquire a reason that is true today or go.

## The larger shape

DG: "perhaps `:-` should be simplified." The concrete complexity is that `:-`
is now **hybrid** — lazy via a transform-built branch when `contains_capture`,
eager via a branchless select otherwise. Two lowerings for one operator, with
the seam invisible in the source. Whether that seam is worth keeping (eager is
cheaper; lazy is what bash does and what side effects require) is the question
under the question.

## Measure before proposing

- does `${x:-$(random)}` advance the PRNG when `x` is set? (the direct test of
  the premise; needs an observable — two draws, compared)
- same for a user function with a side effect, which DD#78 allows and `62bd809`
  already flagged as contradicting DD#78's "side-effect-free" premise
- what breaks if the eager path is deleted and *every* `:-` lowers lazily —
  is the branchless select buying anything measurable?

## Found by

Journal 446, doing the archaeology DG asked for on the `:?` message arc.
