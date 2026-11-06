title: Return kinds without constant-result checks
status: complete

DG identified the conflation of bare return with explicit zero and proposed
separating void and status returns. The AI discussed a matching-result
obligation for every non-void returning path, compatible tail forwarding,
and a separate constant-result check. DG declined the last part:

> drop the constant-result checking and the int-only returns 0 check, record
> the rest and add backlog tasks for the implementation

The accepted rule is about kinds, not values. Bare return and fall-through
are void. Status and integer functions supply their own kind on every
returning path; neither gets an implicit zero for a missing result.
Compatible forwarding retains the callee's kind, including through a
redirect, and self-tail calls are loop edges rather than result exits.
Process termination is not a return.

Constant results are legal for both status and integer functions. Explicit
zero is a real result, not void. The old int-only SB4017 check is retired
without a general same-result check replacing it. The existing call-site
handling and integer/status distinction still apply to constant results.

DD55/DD58/DD70/DD74 and GUIDE were reconciled in place. The former ceremony
question is resolved; its task retains only the anonymous-capture status
consumption question. The classifier task's missing-integer-return and
redirected-forwarding questions are settled consequences; capture syntax
and redefinition policy remain separate.

Two P2 implementation tasks were added. `enforce-function-return-kinds`
owns classification, completeness and forwarding compatibility.
`retire-return-ceremony-check` depends on it because the old check
accidentally rejects some bare/mismatched integer returns; removing it alone
would expose implicit-zero behavior. The tasks may land together.

The existing B79 task now owns only direct pipeline-stage rejection; its
return-producer compatibility work moved to the new kind task. No dependency
was invented between stage rejection and the kind task merely because they
share classification machinery. Return operand conversion/disposition
questions retain their existing separate home.

This records DG's design, not an implementation. No compiler, runtime,
dependency, spec or build rule changed. The new tasks, B79's stage repair
and DD129's runtime repair remain pending; coverage remains paused.
