title: "Require command-adjacent suppression; reject assignment prefixes and whole-list reach"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "Assignment-prefix - receives a located refusal in native and C, independent of taint switches; no implicit rewriting to an inner prefix"
  - "A logical-list prefix belongs only to its adjacent command; later operands are not suppressed and result-kind checks inspect the actual recipient"
  - "Arithmetic mutation hoisting preserves the no-reach barrier for operand captures and value calls"
  - "Legal direct suppression, per-stage boundaries, capture conditions, result kinds and DD130 failure classes remain intact"
  - "Changed spec contracts take fresh global ids; focused before/reversion evidence and a full gate pass with measured predictions"

## Accepted rule

DG, 2026-11-08: **"reject the - not adjacent to the command"**.
DD55 records the rule; DD71 applies it to value bindings, and DD70/DD88
and GUIDE preserve the surrounding result and handling contracts.
Journal 588 is the policy recording, not an implementation claim.

Reject `- x=$(cmd)` instead of making it shorthand for `x=$(- cmd)`.
This covers scalar/cell assignment and append, with literal, captured,
interpolated or nested values and with captures in indices. Typed
declarations already refuse the prefix. Neither an assignment wrapper
nor a capture condition grants permission to reach through a binding.

For `- a || b` and `- a && b`, the prefix belongs only to `a`.
No whole-list suppression is permitted. Classification follows the actual
recipient: an unprefixed void `b` is not a reason to reject the prefix on
`a`; a directly prefixed void call cannot escape its existing refusal
by appearing in a list.

DG subsequently rejected consuming the discarded result (Q3, journal
589). Thus those leading-prefix logical lists must reject, not acquire
whole-list reach or an always-successful status. The separate
`reject-discarded-status-consumers` owns that consumer check.

`[[ ... ]]` and `(( ... ))` remain single commands, not compound bodies.
Their operands are separate evaluations. In particular, lowering an
arithmetic write into a store must not suppress a capture or value call
inside its expression.

## Evidence and ownership

This owns R1, R2, R4, R6 and R7's command-local classification from
`q-suppression-reach-residuals`. That file retains the historical
native/C probes and archive pointers. R1's former alias recommendation is
rejected. All its outer-assignment examples now require compile-time
refusal, not merely a runtime abort.

R2 was historically lowered by `hoist_arith_mutations` into a suppressed
assignment, allowing an operand capture's failure to disappear. Preserve
the same operand barrier as the arithmetic-test route; do not allow an
internal store to recreate the source-level alias being rejected.

The list-binding repair must also review errors.sh 55.145-55.147.
They pin B84's historical status-retention repair on the accepted
whole-list route. Do not preserve that route to keep those tests green,
or retire legitimate status-retention coverage with it.

These are recorded findings, not measurements on the current compiler.
Check fixture legality under current typing, taint, return and capture
rules before treating a result as evidence.

## Implementation and regression obligations

Use the shared statement/command classification and traversal. Reject the
source prefix at a useful location with advice to put it on the intended
command. Do not add a private peel/walk or fix only one lowering fast
path. Check all source placements, redirects and nested bodies in both
backends, with taint enabled and disabled.

Preserve `x=$(- cmd)` for commands whose status may be discarded, direct
non-void call discard, ordinary operand failure checks, per-stage
suppression and existing void/integer-capture refusals. Preserve DD88/
DD101 capture conditions, including `if ! x=$(cmd)`, without treating
handling as an assignment-level discard. Keep DD130's recoverable
redirect/fatal-fault distinction and existing child ownership.

R3's status-less value-intrinsic capture is now rejected by policy;
`reject-statusless-intrinsic-suppression` owns that check and review of
the historical noreach.12 spelling. Preserve the operand-barrier proof
on legal source rather than treating an earlier prefix refusal as cover.
Q1's control-transfer prefix rejection has its own owner,
`reject-control-transfer-suppression`. Its no-op/arithmetic-write
questions and Q5's discarded-test question remain separate.
Q3 is now settled: reject the consumed discard. Coordinate its check and
the 55.145-55.147 migration with `reject-discarded-status-consumers`,
without duplicate specs or retaining whole-list binding underneath a new
refusal. Valid statement-level `a || - b` and partially suppressed
pipelines still need command-local interpretation. Compound rejection
remains with `retire-suppression-on-compounds`.

Give changed contracts fresh ids above the spec.db historical high-water
mark. Prove the focused refusals and barriers against the before binary
and appropriate reversions, retaining legal preservation controls.
Measure impact before predicting NEW/RETIRED/FLIPS/TOTAL; the historical
three-spec B84 block is not a complete impact estimate.
