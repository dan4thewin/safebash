title: Reject statements that only compute an ignored result
status: open
priority: P3
kind: bug
depends: []
gates:
  - "Known result-only statements receive a located pointless-statement error in native and C, independent of taint switches"
  - "Equivalent source spellings, including - on a test and statement-level test || :, obey the same effect/use rule"
  - "Writes, I/O, observable operand/body effects and uncertainty prevent this rejection; silently discarded recoverable failures and incidental allocation do not"
  - "Intentional : remains permitted without becoming an effect witness for an enclosing test"
  - "Constant-returning definitions and consumed calls remain legal; a known effect-free discarded call is checked at its invocation"
  - "Existing type/placement/taint/result-channel guards and no-reach remain; advice cannot offer an equivalent pointless spelling as a workaround"
  - "Otherwise legal fixtures and changed contracts use fresh global ids; focused before/reversion evidence and the predicted ordinary gate pass"

## Accepted rule

DG proposed keeping result discard allowed while giving an error for a
pointless statement, then approved the conservative form on 2026-11-08.
DD70 is authoritative, DD55/DD58/DD69/DD74 and GUIDE agree, and journal
592 records the decision. Q5 in `q-suppression-and-stage-residuals` is
settled as policy; implementation remains open.

Reject a statement whose only work is computing an ignored result when
the absence of other observable effects is known. Keep discard itself
legal where it has a real, otherwise discardable result. This is not a
blanket ban on `- [[ ... ]]`, boolean `- match`, or non-void calls.
It is a new error, not silent dead-code elimination or a runtime assertion.

## Effect and use boundaries

For effect-free operands, reject `- [[ $s == yes ]]`, boolean-only
`- match "$s" "^a"` and statement-level `[[ $s == yes ]] || :`.
An intermediate branch whose only outcome is a no-op does not make
the surrounding computation useful. Apply the same rule to equivalent
spellings, not a private list of these examples. A condition selecting
only no-ops is not an escape. Results used to choose observable work or
supply returns/values remain distinct from an unused statement result.

Retain writes, I/O and observable effects in operands or called bodies.
Out-array `match`/`matchall`, meaningful redirects and wait/reaping work
remain useful when their own result is discarded. Do not require proof
that every written value is later read. Unknown effects prevent this
diagnostic; do not guess the behavior of an external command by name or
assume an arbitrary call terminates harmlessly.

A checked operand abort, for example `:?`, remains observable under
no-reach. A recoverable failure that the owning discard silently consumes
does not: dynamic regex failure alone cannot justify an ignored test.
Incidental allocation and possible OOM do not rescue result-only work.
This classification does not change DD130 or discard any fatal fault.

Preserve explicit `:` and otherwise legal `- :` as intentional no-ops.
They still evaluate arguments and apply redirects; `- : >optional.log`
has a real file operation. Permission to write a no-op is not evidence
of an effect in a larger `test || :`.

Constant-returning functions stay legal. A known effect-free discarded
call can be pointless at the invocation; calls that consume its result
and calls with other effects remain legal. Do not restore SB4017 or turn
this into a same-result test on definitions.

## Implementation and coordination

Use the shared statement/expression traversal and resolved command/call
identities for one effect/use classification. Include wrappers, redirects,
nested captures/value calls and the actual lazy/handling context. Reuse
existing facts only when they prove the needed property: `capture_purity`
means buffer eligibility, not absence of state or I/O effects.

The existing `validate_arith_as_statement` SB5007 comparison check is
narrower than this rule. Do not mistake it for a general effect analysis
or silently relax DD31's separate arithmetic placement restrictions.
The new check grants no otherwise forbidden syntax or result channel.

Coordinate with the adjacency, discarded-status-consumer, control-prefix
and status-less-intrinsic enforcement owners. Their refusals remain
independent. `bug-peel-family-classifiers` still needs its shared repairs
on effectful forms; an earlier pointless-statement refusal is not cover
for a hidden type/taint/operand bug.

Locate the pointless statement and explain the unused computation.
Reconcile SB4018 and any other affected advice: neither `-` nor `|| :`
is an unconditional escape. No diagnostic number or implementation
architecture was selected in the policy discussion.

## Regression obligations

Q5's old tables are historical; no probe was rerun for this recording.
Review all affected current fixtures, including advice assertions and
specs using discarded tests as scaffolding. Preserve their original
behavioral assertions on legal source rather than letting an earlier
refusal count as proof of operand evaluation or a lower-pass guard.

Use otherwise legal focused refusals and preservation controls in both
backends: effect-free test/list/call cases; consumed results; collection
outputs; operand effects/checks; redirects; intentional no-ops; uncertain
calls; and unchanged constant-returning definitions. Pin the diagnostic,
not any nonzero exit. Prove the new refusals against the before binary
and suitable independent reversions, allocate changed contracts above
the spec.db historical high-water mark, and predict the full gate from
the measured NEW/RETIRED/FLIPS/TOTAL impact.
