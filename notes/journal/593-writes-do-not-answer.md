title: Writes do not answer
date: 2026-11-08

## DG's ruling

The next arithmetic question was whether `- (( n += 1 ))` could discard a
result. The write is useful, so the new pointless-statement rule does
not settle it. DD31 still said a mutation statement's status was always
zero, while DD55 said arithmetic did not produce exit codes.

DG answered:

> an assignment or mutation shouldn't have a result; a comparison should, I think and thus it should be an error to have such as a statement; suppression should be rejected

Assignments and arithmetic mutations now explicitly produce no result:
neither their assigned value nor a fabricated success status. Keep
`(( n += 1 ))`; reject its `-` prefix. Comparisons have results that must
be consumed, not left as arithmetic statements. A `-` does not make
that statement valid.

The AI applied the same principle to bare `(( 0 ))`, `(( n ))` and
`(( n + 1 ))`: these cannot be status-producing statements. It also
reported the operand consequence: `(( n = (m += 1) ))` cannot obtain a
value from the inner write. Parentheses alone do not turn a write into a
test. Comma sequencing remains separate: all statement parts are writes;
a legal condition comma has a mutation prelude and a consumed final result.
The logical-list comma-placement question remains separate.

No mutation establishes fresh `$?`/`$(status)` permission. Existing
handling-position operand reads and capture-condition results remain:
the latter belong to the captured operation, not the assignment.
Operand checks and fatal arithmetic behavior are unchanged. DD70's
discard/effect rule for other eligible operations is not replaced.

## Recording and queued enforcement

DD31 is authoritative. Its zero-status assertion and overstatement of
comma enforcement were corrected in place; DD55/DD70/DD71 and GUIDE
agree. Q1's arithmetic-prefix question is settled.

`q-arith-test-statement-status` keeps its slug and P3 priority but is now
an open implementation bug. It owns arithmetic result/context enforcement
and migration of the old status idiom. The non-final-comma task now
applies the same parenthesis/result rule; cov.259's earlier write cannot
rescue its unused final `1`.

The original task's thirteen-spec census and probe tables are historical,
not fresh measurements or gate predictions. Its bare status-function
replacement predates return-kind enforcement, and the proposed helper
name `fail` is now a builtin. Implementation must preserve those status
assertions using legal handling contexts and fresh ids for changed contracts.

The R2 lowering barrier remains owned by `reject-nonadjacent-suppression`.
A new refusal of `- (( ... ))` does not prove that operand suppression
was repaired. Review the old arithmetic barrier fixtures, including
noreach.14, and retain evidence on legal paths or account for paths made
unreachable. The residual records and neighboring owners now agree.

This is policy/backlog recording only: no compiler, runtime, spec or build
code changed, and no new behavior probe, sweep or coverage run was made.
Journal 577 remains the accepted implementation baseline.
