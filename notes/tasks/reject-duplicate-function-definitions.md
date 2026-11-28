title: Reject duplicate function definitions
status: done
priority: P1
kind: work
depends: []
gates:
  - "a second source definition of a function name is a located compile-time error, regardless of body, result kind, signature or whether it is called"
  - "same-kind and identical duplicates reject as well as void/status/integer changes; source order cannot select a replacement or overload"
  - "duplicate refusal precedes definition-keyed analysis that would otherwise overwrite, combine or disagree about the bodies"
  - "the i1-i3 reproducers in bug-int-call-by-name-redefinition reject in both modes; no miscompiled output is accepted as a result"
  - "unique definitions retain forward calls, result-kind handling, inferred signatures, capture behavior, taint summaries and legal self-tail calls"
  - "focused native/C specs and negative/reversion controls prove the refusal; affected nonvoid specs migrate at fresh ids and the predicted ordinary gate passes"

## Delivery, 2026-11-28

Implemented on `unattended-backlog-20261128`; journal 681 records the
shared inventory, early SB5109 guard and removal of the duplicate-only
template-class filter. `fnuniq.1`-`.15` cover the duplicate matrix,
including i1-i3, signatures, unused bodies and repeated original-site
reporting. All fifteen fail on the original compiler and with the guard
disabled, in both modes. Seven old contracts retire at fresh IDs.
The predicted gate passes with 55 new mode IDs, 14 retirements, no flips
and 13,467 total checks. Existing unique-definition behavior remains green.

## Settled contract

DG chose "reject duplicates" on 2026-11-06 (DD86/DD55, journal 582).
One source function name denotes one body, one result kind and one inferred
signature. Even identical repetitions and uncalled duplicates are errors.
No replacement or overloading survives merely because two definitions
have compatible signatures.

Names remain global and hoisted. SB5071's main-list-only placement stays;
anonymous capture bodies and compiler-generated functions do not create
additional source definitions of a user name.

## Implementation scope

Use the existing structural-validation and canonical definition inventory
to diagnose repeated names before later name-keyed passes can merge or
overwrite them. Locate the repeated definition and identify the original
site. Do not repair the miscompile by making replacement semantics more
elaborate or by inventing a second function resolver.

Align directly dependent definition/class bookkeeping with the validated
single-definition invariant, including integer-call selection, templates,
signature inference and the output/taint summary consumers. Enumerate
producers before removing duplicate-only paths; do not mechanically add
duplicate AST-gate ICEs or silently retain a conflicting fallback.
This is not a general classifier rewrite.

The existing P1 `bug-int-call-by-name-redefinition` depends on this task
and retains its historical reproducers. Its acceptance criteria are now
source rejection, not making those replacement programs run correctly.
Same-kind/identical definitions need coverage beyond that old int/void
matrix. Check current specs and spec.db history before choosing fresh ids;
`nonvoid.25`-`nonvoid.29` contain the old definition-in-force cases.

Preserve unique-definition forward references and the settled return-kind,
scalar/collection-signature, no-shadowing, taint and self-tail contracts.
Integer-capture semantics are now settled separately (DD58, journal 583);
`bug-int-capture-redirect-switches-kind` owns their enforcement.
Anonymous-capture status-return rejection is also settled separately
(DD55/DD86, journal 584); `reject-anonymous-capture-status-returns` owns it.
The single-definition function-name-as-variable bug also remains
independently owned by `bug-function-name-read-as-variable`.

The sections above retain the implementation scope set before delivery;
journal 681 supplies its measured outcome rather than a policy-only closure.
