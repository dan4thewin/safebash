title: "SB5035 (`$(random)` in a `:-` default) justifies itself by eagerness that no longer applies"
status: resolved
priority: P3
kind: question
depends: []
gates: []

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

## Resolved 2026-09-30 — SB5035 is deleted, and so is the seam under it

DG ruled past the question: "the `:-` default, let's change it to match the
`:?` so eager/side-effects are a non-issue." A disposition default is a
**literal** in all three spellings now, so `$(random)` cannot appear in one and
the ban has nothing left to reject. SB5035, `flag_random_default`,
`random_default_diag`, `expr_invokes_random`, and `cmdsub_head_is` are gone.

Each measurement this filing asked for was answered by the change rather than
by an experiment:

- *does the PRNG advance when `x` is set?* — moot; the operand cannot invoke it.
- *the side-effecting user function DD#78 allows* — moot; the operand cannot
  call one. DD#78's read discipline is withdrawn entirely.
- *what breaks if the eager path is deleted* — the hybrid is gone in the other
  direction: `lift_default_value` is a branchless select again and
  `contains_capture` is deleted. The "two lowerings for one operator, with the
  seam invisible in the source" is one lowering.

The filing's instinct was right and undersold: the premise had not merely
expired, the *artifact the premise described* (eager materialization) was never
a safety decision to begin with. See DD#65's follow-on and DD#78's final
amendment.
