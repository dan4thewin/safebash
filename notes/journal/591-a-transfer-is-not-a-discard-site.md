title: A transfer is not a discard site
status: complete

DG chose **reject** for source `-` on `return`, `exit`, `fail`, `break`
and `continue`, then requested the next decision. The prefix cannot
cancel the transfer or reach failures inside its operands. Keeping the
redundant control-transfer spellings was the alternative, not the ruling.

Copilot recorded the rule in DD55 and reconciled DD70/DD74/DD128/DD130
and GUIDE. The new P3 `reject-control-transfer-suppression` owns
enforcement; Q1's control-transfer cohort is settled, not implemented.

The rule covers otherwise legal bare, valued and forwarding returns,
default and explicit termination statuses, and counted loop transfers.
Body placement, wrappers and attached redirects do not grant exceptions.
Independent typing, placement and anonymous-capture return rules remain.
No diagnostic number or implementation architecture was selected.

A function's returned result and discard at its call site are different
from a prefix on its return statement. Legal `- f` for a non-void f
stays legal, as do compatible forwarding and constant results. Unprefixed
transfers retain their behavior; DD128's exit/fail routing, operand checks
and fatal-fault boundaries are unchanged.

DD130 classifies failures, not syntax. Recoverable attached-open handling
still requires an otherwise legal form, so an open failure cannot
justify a forbidden control-transfer prefix. Nor does lacking that
spelling make the file failure fatal. The no-op `:` remains separate:
`- : >optional.log` has meaningful recoverable file-error handling under
the accepted model. Arithmetic-write prefix eligibility remains with
Q1's non-control forms.

The next question is Q5's boolean-only test discard. Its old "no effect"
framing omitted dynamic regex and redirect failures and understated
operand effects. More importantly, DD70 deliberately permits explicit
discard of a real boolean; this is not the missing-status case just
rejected for value intrinsics. The question now makes that distinction,
recommends retaining the existing permission, and states what a narrower
rejection would need to settle, including `test || :`. That recommendation
is Copilot's, not a new ruling by DG.

Historical Q1/Q5 measurements were not rerun. Enforcement must use
otherwise legal fixtures: an old return/capture rejection cannot pin the
new prefix rule. No compiler, runtime, spec or build code changed; no
new behavior probe, full sweep or coverage run was made. Journal 577
remains the accepted implementation baseline.
