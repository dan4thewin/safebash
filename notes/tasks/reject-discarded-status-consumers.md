title: Reject source-level consumption of a discarded status
status: open
priority: P3
kind: bug
depends: []
gates:
  - "Directly suppressed results in conditions, logical-list decisions, capture conditions and status return producers receive located refusals in native and C, independent of taint switches"
  - "Middle clauses and consumed list tails are covered; unconsumed statement tails remain legal"
  - "All-suppressed pipeline consumers reject; partial pipelines retain the aggregate over unsuppressed stages"
  - "Ordinary handling, text operands from legal captures, body-local discard followed by a fresh return, waitstatus and existing result/body/failure boundaries are preserved"
  - "Changed contracts use fresh global ids; focused before/reversion evidence and the full gate pass with measured predictions"

## Accepted rule

DG chose **reject** on 2026-11-08 when asked about testing a result after
`-` discards it. DD55 records the rule; DD70/DD74/DD88 and GUIDE agree.
Q3 in `q-suppression-and-stage-residuals` is closed as a policy question,
not as implemented behavior. Journal 589 records the choice.

Reject a source `-` when the result it discards is consumed:

- a direct command in `if`, `elif`, `while` or `until`;
- an operand whose status decides an `&&`/`||` continuation, including
  middle clauses and the final clause of a consumed list;
- the command supplying a capture condition's status;
- a status producer forwarded by `return`, which DD74 defines as an
  explicit consumer.

Reject an all-suppressed pipeline in those consumer positions, including
inside a capture condition. Preserve partially suppressed pipelines:
their consumer uses the aggregate over the remaining stages, not the
discarded status. The rejection is structural; do not make a known-success
command or an unreachable branch an exception.

The alternatives of supplying success for a discarded status or keeping
different single-command/all-suppressed-pipeline rules were not selected.

## Evidence and coordination

Q3 retains the original native/C table and probe archive pointers.
Its direct-condition, logical-left, capture-condition and all-suppressed
pipeline examples require refusal. Its partial-pipeline conditions are
preservation controls. These are historical measurements; revalidate
fixture legality under current taint, typing, return and capture rules
before drawing a before/after conclusion.

`reject-nonadjacent-suppression` separately owns prefix binding, assignment
refusal and the arithmetic operand barrier. Share command-local
interpretation; do not use the historical `Suppressed(List(...))` shape
to grant whole-list reach. Coordinate review of errors.sh 55.145-55.147
and any changed contracts, without duplicate coverage or predicting impact
from only that three-spec block.

## Required boundaries and preservation controls

Follow the existing shared predicate/statement classification and walkers.
This is a check on explicit source discard, not every unchecked or
suppressed lowering flag: ordinary conditions, capture conditions and
return producers disable implicit aborts because they handle status.
Rejecting those would break the feature this rule is meant to clarify.

DD88's accepted named-predicate extension follows the same distinction:
`if text=$(f)` consumes f's status, whereas `if text=$(- f)` attempts to
consume a discarded result and must reject. Coordinate direct/redirected
call handling and advice with `predicate-capture-conditions`; do not reject
the valid form through the old blanket function-capture exclusion.

Check nested list consumption, wrappers and all executable bodies.
`a || - b` is legal as a statement, but its tail is consumed in
`if a || - b { ... }`, `a || - b || c` and `return a || - b`.
Treat a pipeline as an aggregate producer rather than marking every
suppressed stage as an independently consumed result.

Do not ban a minus merely because it occurs somewhere inside a condition:
a legal `$(- cmd)` text operand supplies stdout, not the discarded status.
Keep its existing taint/type checks. Do not inherit a caller's predicate
context into a function or capture body: `- cmd; return 0` supplies a
fresh authored result. Constant returns remain legal.

Preserve ordinary `x=$(- cmd)` without a status consumer, unconsumed
statement tails, statement-level all-suppressed pipelines and normal
`if x=$(cmd)` / `if ! x=$(cmd)` handling. Preserve existing `!`/`-`,
void-call, integer/status and status-availability restrictions.
Do not reset process exit bookkeeping or erase child outcomes:
`- wait` discards a pid while `waitstatus` retains a separate child
status (DD126). No-reach, process ownership and DD130 failure classes
remain unchanged.

Choose a located diagnostic that identifies the discard/consumer conflict
and advises handling the status without `-`. It must not be satisfied
accidentally by an unrelated type, taint or stale-status diagnostic.
No diagnostic number or implementation architecture was selected in the
policy recording.

Use focused native/C refusals and preservation specs with before/reversion
controls. Give changed contracts fresh ids above the spec.db historical
high-water mark. Measure NEW/RETIRED/FLIPS/TOTAL before the full gate;
do not treat the old table as a fresh impact census.
