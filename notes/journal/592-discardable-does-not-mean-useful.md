title: Discardable does not mean useful
status: complete

DG asked: **"can we keep the discard allowed but give an error for a
pointless statement?"** After the distinction and examples were
discussed, DG approved it: **"go with this"**.

Discard eligibility and statement usefulness are independent. `-` may
legally discard a real result, while the statement is still an error
because its only work is computing that ignored result. A comparison
with effect-free operands therefore rejects both as `- [[ ... ]]` and
as the statement-level `[[ ... ]] || :`. The second spelling is not an
escape through an intermediate branch.

The check is conservative. Writes, I/O, observable operand/body effects
and uncertainty prevent rejection. An out-array match still fills its
array; a redirected no-op still performs its file operation. A checked
operand abort remains observable because a prefix does not reach it.
Conversely, a recoverable regex failure that the discard also consumes
is silent under DD70 and does not make a statement useful. The preceding
Q5 framing overstated that exception. Incidental allocation and possible
OOM likewise do not justify otherwise pointless calculation; the runtime
fatal-fault contract is unchanged.

Explicit `:` remains an intentional no-op, including its otherwise
legal suppressed form. That is permission to write a no-op, not a claim
that `:` has an effect which rescues an enclosing test. Q1's no-op
question is resolved on that basis; arithmetic-write prefix eligibility
remains separate.

The same distinction applies to function calls, not just boolean tests.
A known effect-free `- f` can be a pointless invocation, while f's
definition and calls using its result remain legal. Constant-returning
functions, compatible forwarding and result kinds are unchanged.
Copilot reported that consequence before recording it. This is not a
revival of SB4017, a new purity requirement on function definitions or
a requirement to prove that every written value is eventually read.

Copilot recorded the rule in DD70 and reconciled DD55/DD58/DD69/DD74,
GUIDE and the related backlog owners. DD70's opening channel summary and
discard advice now state the current rule in place, rather than leaving
an earlier ban or unconditional `|| :` escape as active guidance.
Q5 is settled as policy; the new
P3 `reject-pointless-statements` owns enforcement and advice changes.
The shared-classifier repair remains necessary for effectful forms.
No diagnostic number or implementation architecture was selected.

The preceding source inspection found SB5007's existing narrow
comparison check, not a general effect analysis. It also confirmed that
`capture_purity` means buffer eligibility, not freedom from effects.
Those are implementation cautions, not evidence that the new rule works.
No compiler, runtime, spec or build code changed; no new behavior probe,
full sweep or coverage run was made. Journal 577 remains the accepted
implementation baseline.
