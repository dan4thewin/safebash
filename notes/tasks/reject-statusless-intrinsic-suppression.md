title: Reject suppression on status-less value-intrinsic captures
status: open
priority: P3
kind: bug
depends: []
gates:
  - "Captured value intrinsics without a separate status receive a located refusal for their source - in native and C, independent of taint switches"
  - "The rule covers string/integer results and all otherwise legal capture placements; wrappers and redirects do not change result kind"
  - "Ordinary value captures, getenv lookup handling/discard, external stdout captures, explicit integer/pid discard and existing body/operand boundaries are preserved"
  - "Historical noreach.12 and other affected specs are reviewed; a prefix refusal is not accepted as proof of the no-reach barrier"
  - "Changed contracts use fresh global ids; focused before/reversion evidence and the full predicted gate pass"

## Accepted rule

DG chose **reject** on 2026-11-08 for a `-` on a value-intrinsic capture
with no separate status to discard. DD55 records the rule, with companion
text in DD58/DD70/DD111 and GUIDE. Journal 590 records the decision;
R3 in `q-suppression-reach-residuals` is closed as a policy question.

Reject `$(- toupper abc)` and status-less integer counterparts such as
`$(- strlen abc)`. The capture consumes the intrinsic's value; the
prefix cannot claim to discard another result that does not exist.
Use the ordinary capture without `-`. The rule follows result channels,
not whether particular arguments make an otherwise fallible operation
known to succeed.

## Boundaries

Use the canonical builtin identity and result/capture classification.
Do not introduce a private name list or repair only the historical
`lift_suppressed_capture` path. Cover ordinary assignments, declarations,
arguments, interpolation and nested expressions wherever the capture
itself is otherwise legal. Wrappers and redirects do not create a status
or turn a value capture into stdout capture; this grants no new redirect
or statement-position syntax to intrinsics.

DD101's `getenv` has a lookup status as well as a value. Preserve its
legal status discard and ordinary capture-condition handling, while
`reject-discarded-status-consumers` forbids consuming an explicitly
discarded status. Preserve legal external stdout captures too.
Taint and status-availability rules remain in force.

Ordinary `- int_fn` and `- wait` discard the actual integer or pid rather
than simultaneously capturing it. Preserve them and the separate
`waitstatus` latch. A getter's integer value is not itself a new status
channel merely because it represents an earlier command or child result.
Nor is the child outcome saved by `wait` a status of that invocation.

No-reach still protects operands and bodies. Heap exhaustion and other
fatal faults remain fatal (DD130); they do not justify the prefix.
Recoverable attached-redirect handling requires an otherwise legal form
and cannot change a callee's result kind or manufacture a successful
empty value. Control-word/no-op prefixes and discarded tests remain
separate questions in `q-suppression-and-stage-residuals`.

Coordinate integer capture classification with
`bug-int-capture-redirect-switches-kind`, and operand-barrier coverage
with `reject-nonadjacent-suppression`, without duplicate checks/specs.
No diagnostic number or implementation architecture was selected here.
The diagnostic should locate the prefix and explain that the capture
already consumes the only result, advising removal of `-`.

## Evidence and regression obligations

R3 retains the original native/C measurement and archive pointer. It
was not rerun or revalidated against later language changes. Review the
historical `noreach.12`, `int n=$(- strlen $(/bin/false))`: its runtime
operand-failure assertion cannot survive as evidence for no-reach if
the source now rejects for the inert prefix. Preserve that boundary on
a legal form, and avoid rival coverage.

Revalidate fixtures and measure all affected specs before implementation.
Use focused refusals and preservation controls in both backends, proving
the refusals against the before binary and relevant independent reversions.
Allocate changed contracts fresh ids above the spec.db historical
high-water mark and predict NEW/RETIRED/FLIPS/TOTAL from the actual impact,
not from this one historical fixture.
