title: One definition per function name
status: complete

DG chose "reject duplicates" after considering three possibilities:
reject repeated function definitions, permit only same-signature
replacements, or retain definition-in-force behavior with changing kinds.
The decision is one name, one body, one result kind and one inferred
signature. Identical repetitions, same-kind replacements and uncalled
duplicates are errors too.

This removes source-order replacement rather than making it more
predictable. Names stay global and hoisted; a call on either side of the
unique definition refers to that same body. Main-list-only placement,
forward references, anonymous capture bodies, generated handlers and
legal self-tail calls are unchanged.

Copilot recorded the rule in DD86, alongside scope and function-name
placement, rather than creating another DD. DD55's operative
definition-in-force paragraph and GUIDE now agree; DD58's name-collision
section defers to DD86 instead of retaining obsolete local shadows.
DD127 identifies its all-definitions output classification as a temporary
implementation boundary, not a continuing language promise.

The new P1 `reject-duplicate-function-definitions` owns structural refusal
and directly dependent definition/class bookkeeping. The existing P1
`bug-int-call-by-name-redefinition` depends on it and remains open.
Its old int/void reproducers must now reject at the duplicate definition;
their old expected runtime output is no longer the acceptance criterion.
The separate function-name-as-variable bug is not closed by this decision.

Classifier residual Q3 is settled, not implemented. Q1's integer-capture
surface remains open; Q2/Q4 were already implemented in journal 577.
The summary-precision and completed return-kind records now point to the
unique-definition work instead of asking for a redefinition policy.
The implementation must migrate affected old nonvoid specs and cover
same-kind/identical duplicates, not only the original int/void miscompile.

No compiler, runtime, spec or build code changed. No new behavior probe,
full sweep or coverage run was made; journal 577's accepted gate remains
the implementation baseline. Duplicate rejection is still pending.
