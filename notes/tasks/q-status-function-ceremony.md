title: "Resolved: anonymous captures may not explicitly return a status"
status: done
priority: P3
kind: question
depends: []
gates:
  - "DG's rejection of explicit anonymous-capture status returns is recorded in DD55/DD86"
  - "pending enforcement has an explicit backlog owner; closing this question does not claim implementation"

## Return-kind and ceremony questions resolved, 2026-11-06

DG separated bare return from explicit zero and declined all constant-result
checking, including the existing int-only SB4017 rule. The current contract
is in DD55/DD58/DD74; journal 577 implements
`enforce-function-return-kinds` and `retire-return-ceremony-check`.

The body contract applies to anonymous function bodies as well (DD86).
The caller-side status question was separate from the result kind and
constant-result policy; it is now resolved by the rejection below.

This slug is retained for incoming links. Its former eight-flip estimate
assumed that bare return would be rejected along with explicit zero; that
is not the chosen rule and is not an implementation prediction.

## Anonymous status returns rejected, 2026-11-06

DG chose rejection, not a new anonymous-result handling/discard surface.
An anonymous capture body may not explicitly return a status: `return 3`,
`return 0`, `return $?` and status-producing forwarding all reject.
Redirects do not change that kind. Bare return, fall-through and compatible
void forwarding remain legal under the existing return-kind contract.

Use a named status function when that result is needed. Its call sites
must handle/discard it, and `$(- status_f)` is the existing stdout-capture
form that discards the status explicitly. A prefix on the first statement
inside an anonymous body does not discard the body's result.
DD88's user-function/multi-statement capture-condition exclusions remain.

DD55/DD86 record the accepted policy; journal 584 records the decision.
The open P2 `reject-anonymous-capture-status-returns` owns enforcement,
including nested positions, forwarding, diagnostics and native/C controls.
This question is closed; the compiler fix is not implemented.

The original F6 filing observed that the status was ignored and found no
dedicated spec; rec.7/.8 asserted SB5037 instead. Native CLI controls on
2026-11-06 confirmed that anonymous `return 3` and `return 0` both capture
`data` and continue with process status zero. Bare return is also accepted;
the named status-function capture gets SB4016 unless written `$(- f)`.
The multi-statement capture-condition control gets SB4025. C was not
remeasured in that control run. Inventory current specs before implementing;
these observations are evidence of the gap, not new spec coverage.

`dd55-nonvoid-functions` is the historical completion record, not authority
for the retired bare-return/zero conflation.
