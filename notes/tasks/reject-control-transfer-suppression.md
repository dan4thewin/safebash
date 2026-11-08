title: Reject suppression prefixes on control transfers
status: open
priority: P3
kind: bug
depends: []
gates:
  - "Source - on return, exit, fail, break or continue receives a located refusal in native and C, independent of taint switches"
  - "Bare/valued/forwarding returns, default/explicit termination statuses, counted loop transfers, redirects and nested bodies cannot evade the rule"
  - "Unprefixed transfers, non-void call-site discard, compatible forwarding, constant results, no-reach and terminal routing retain their contracts"
  - "Fixtures are otherwise legal under current return/capture/type rules; historical Q1 results are not treated as fresh evidence"
  - "Changed contracts use fresh global ids; focused before/reversion controls and the predicted ordinary gate pass"

## Accepted rule

DG chose **reject** on 2026-11-08 for source `-` on `return`, `exit`,
`fail`, `break` and `continue`. DD55 records the rule, with companion
text in DD70/DD74/DD128/DD130 and GUIDE. Journal 591 records the decision.
Q1's control-transfer cohort in `q-suppression-and-stage-residuals` is
settled; this implementation remains open.

Locate the prefix and explain that it must be removed, not silently
ignored. A return may send a result to its caller, but the return
statement does not provide a separate discard site. Termination and
loop transfers likewise do not become resumable through a prefix.
No diagnostic number or implementation architecture was selected.

## Boundaries and implementation

Use the shared statement/command identity and traversal; do not add a
private classifier or recognize only a few textual forms. Cover bare,
valued and producer/forwarding returns, exit/fail with default and
explicit statuses, and ordinary/counted break/continue. Follow otherwise
legal placements through named bodies, captures, handlers and wrappers.
Do not grant legality to an independently forbidden form merely to
exercise the prefix check.

Attached redirects are not an exception. DD130's recoverable file-error
handling requires an otherwise legal handler; its failure classification
does not turn a forbidden prefix into a legal one. Coordinate with
`recoverable-file-failures` without changing the order or effects of
unprefixed transfers.

Preserve `- f` at a non-void function's call site and ordinary integer/pid
discard. Rejecting `- return` is not a ban on returning a nonzero or
constant answer, compatible forwarding or self-tail calls. Preserve
ordinary return-producer handling, counted-loop targets and DD128's
exit/fail routing and once-only process ownership.

Operand and body failures retain no-reach; fatal faults remain fatal.
This does not legalize explicitly discarded status consumers or
anonymous status returns. Coordinate with those enforcement owners,
and with `reject-nonadjacent-suppression`, without duplicate checks or
conflating authored prefixes with internal unchecked-handling flags.

The no-op `:` is not in this cohort. Its meaningful redirected form,
arithmetic-write prefix eligibility and boolean-only test discard remain
separate Q1/Q5 questions; no general no-effect restriction was selected.

## Evidence and regression obligations

Q1 retains its F1+F4 tables and archive pointer. No fixture was rerun for
this recording; those tables predate return-kind/capture enforcement and
the `fail` builtin. Revalidate each fixture before relying on its result.

Use otherwise legal refusals and unprefixed preservation controls for
all five transfers, both backends and relevant taint modes. Pin the
prefix diagnostic, not any unrelated rejection or a generic nonzero exit.
Prove new refusals against the before binary and relevant independent
reversions. Review affected specs, preserve any operand/transfer
assertions on legal source, allocate changed contracts above the
spec.db historical high-water mark, and predict the full gate from the
actual NEW/RETIRED/FLIPS/TOTAL impact.
