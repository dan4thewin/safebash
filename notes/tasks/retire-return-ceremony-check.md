title: Remove SB4017 and keep constant function results legal
status: complete
priority: P2
kind: bug
depends: [enforce-function-return-kinds]
gates:
  - "remove the int-only returns-zero check and code used solely to support it; do not replace it with a general constant-result check"
  - "explicit constant integer returns, including zero, compile and preserve the full value"
  - "constant status results, including zero and nonzero values, are legal and retain non-void call-site handling"
  - "bare/missing/incompatible non-void results still reject through return-kind/completeness checks, not SB4017"
  - "retire active SB4017 assertions and reconcile the diagnostic census, focused native/C controls and the predicted ordinary spec gate"

## Decision

DG, 2026-11-06: "drop the constant-result checking and the int-only
returns 0 check, record the rest and add backlog tasks for the implementation".
DD55/DD58/DD70/DD74 record the retirement; journal 574 records the discussion.

A function may deliberately return the same value on every returning path.
Zero is not a special exception. A status-zero function remains a status
function, and an integer-zero function remains an integer function; neither
becomes void because of its value.

## Dependency and scope

The current SB4017 scan also happens to reject some bare integer returns
and void-to-int forwarding. Removing it without replacement kind/completeness
checks would let those malformed returns silently supply zero. Complete
`enforce-function-return-kinds` first or land both tasks together.

The source inspection for the ruling found
`transform/statements.rs::analyze_function_returns`, its invocation and the
zero-return helpers in `transform/functions.rs`. Enumerate their consumers
before deleting helpers; remove only code whose sole purpose is this policy.
Do not repurpose SB4017 as a missing-return or type error.

Use focused cases for explicit integer zero and constant status results,
plus negative controls proving that missing/mismatched results are still
refused. Reconcile old source-level rejections with fresh spec IDs and the
usual retirement predictions. Preserve SB4016/SB4076 and the integer/status
boundary: a constant integer is still not a legal status-producing stage.

Implemented with the replacement return-kind guards in journal 577.
SB4017, its scan and its zero-result helpers/assertions are removed.
Native/C before-controls distinguish explicit integer zero from the old
rejection; independent reversions pin missing/mismatched-result guards.
The final ordinary gate passes 11345 checks with no failures.
