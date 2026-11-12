title: DD#55 — non-void function detection and call-site enforcement
status: done
depends: []
severity: correctness
gates:
  - 55.12
  - 55.13
  - 55.14
  - 55.15
  - tests
  - commit

## Completed enforcement

F6 unit A implemented B15: a bare call or list tail of a status function is
an error unless its status is handled or discarded with `-`. Integer
results must be captured, used as values or explicitly discarded, not used
directly as predicates. Journal 547 records that implementation.

## Current contract and follow-up

DG's 2026-11-06 ruling replaces this task's original bare-return/zero
conflation and ceremony rule. Bare return supplies no result; explicit
zero supplies a status or integer according to the function's kind.
Non-void functions must supply their result on every returning path.
Compatible forwarding remains legal, and constant results are legal.

Journal 577 implemented the replacement return-kind contract in:

- `enforce-function-return-kinds` — kind classification, completeness and
  compatible forwarding;
- `retire-return-ceremony-check` — remove SB4017 after the replacement guards
  are in place, without introducing any same-result check.

DD55/DD58/DD74 are authoritative. DG subsequently rejected explicit status
returns from anonymous captures (DD55/DD86, journal 584).
`q-status-function-ceremony` is closed as a question;
`reject-anonymous-capture-status-returns` owns the pending enforcement.
