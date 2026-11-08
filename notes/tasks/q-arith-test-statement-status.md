title: Enforce no-result arithmetic mutations and reject value-only statements
status: open
priority: P3
kind: bug
depends: []
gates:
  - "Arithmetic mutations produce no value/status and establish no fresh permission to read $? or $(status), in native and C"
  - "Source - on arithmetic mutations and bare or suppressed arithmetic comparison/test/calculation statements receive located refusals, independently of taint switches"
  - "Parentheses do not give a mutation a result; a write used as another expression's operand is rejected"
  - "Statement commas contain only mutations; condition commas retain no-result mutation preludes and a consumed final result, coordinated with the existing comma tasks"
  - "Legal operand/status reads, capture-condition handling, ordinary mutations, consumed comparisons, no-reach and fatal arithmetic behavior remain"
  - "Historical status-source and suppression-barrier fixtures are migrated to legal forms without losing their original assertions; earlier refusals are not counted as barrier proof"
  - "Changed contracts use fresh global ids; focused before/reversion controls and the full gate pass with measured predictions"

## Accepted rule (2026-11-08)

DG ruled that assignments and mutations have no result, comparisons do,
and an arithmetic comparison must not stand as a statement; suppression
is rejected. DD31 is authoritative, with matching DD55/DD70/DD71 and
GUIDE text. Journal 593 records the decision. The policy question is
closed; this existing slug and P3 priority now track implementation.

Keep ordinary `(( n += 1 ))` as a no-result write. Reject
`- (( n += 1 ))`: neither the assigned value nor a fabricated success
status is available to discard. Reject standalone comparisons and
tests/calculations (`(( n > 0 ))`, `(( 0 ))`, `(( n ))`, `(( n + 1 ))`),
with or without `-`. A preceding comma mutation does not rescue an
unused final result. Operand effects or uncertainty do not override this
arithmetic context rule; DD70's separate effect/use check remains.

A parenthesized write remains a write, not a result-producing test.
Using a mutation as an operand, as in `(( n = (m += 1) ))`, is invalid:
the inner write supplies no value. Comma sequencing stays distinct.
Coordinate classification with `bug-arith-comma-nonfinal-part-unchecked`;
the logical-list comma-placement question retains its separate owner.

Writes cannot create fresh `$?`/`$(status)` permission. This does not
forbid an operand from reading an already available status under DD55,
nor change DD88/DD101 capture conditions: their result belongs to the
captured operation, not the assignment. Operand/body checks and fatal
arithmetic faults remain unchanged.

## Historical evidence

2026-10-27, while porting F1+F4 (journal 537), in the probes for DD#31's
2026-10-27 addendum. It is **pre-existing**: every row is the same at
`8e28c61`, with F1+F4, and at `1309385`, in both backends.
These are the original measurements, not a current probe or spec census.

Each program prints `rc=$?`, and `a=$a` where it writes.

| Id | Program | Output |
|---|---|---|
| t1 | `int a=0; (( a ))` | `rc=1` |
| t2 | `int a=1; (( a ))` | `rc=0` |
| t3 | `(( 0 ))` | `rc=1` |
| t6 | `int a=0; (( a + 1 ))` | `rc=0` |
| t4 | `int a=0; (( a++, a - 1 ))` | `rc=1 a=1`: a split comma's final test part sets the status too |
| t5, h4 | `int a=0; (( a = 0 ))`, then `(( a += 0 ))` | `rc=0 a=0`: a mutation's status is always 0, even when its value is 0 |
| h1 | `int a=-2; (( (a+=2) ))` | `rc=1 a=0`: a parenthesised write classifies as a test, so its status follows its value |
| h2 | `int a=1; (( (a+=2) ))` | `rc=0 a=3` |
| h3 | `int a=-2; (( (a+=2), 0 ))` | `rc=1 a=0` |
| o5 | `int a=0; (( a )); exit $?` | the program exits 1 |

None of them aborted. A false test set the status without being a failed
command.

The investigation also tried the following status sources. The bare
status-function call in o1 predates the 2026-11-06 return-kind enforcement
and is no longer a valid replacement:

| Id | Program | Result |
|---|---|---|
| o1 | `fn f { return 1 }; f; echo "rc=$?"` | `rc=1` |
| b1 | `- /bin/false; echo "rc=$?"` | SB5012 "no preceding command" |
| o2 | `/bin/false \|\| :`, then `$?` | SB5012 |
| o3 | `if /bin/false { : }`, then `$?` | SB5012 |
| o4 | `fn f { return 1 }; - f`, then `$?` | SB5012 |

(SB5012's "no preceding command" is wrong in its own way:
`bug-sb5012-message-names-no-command`.)

## Historical spec dependencies

The 2026-10-27 census found 13 specs using `(( 0 ))`, or `(( 1 ))` for the
success direction, as their status source. All were dual-mode, so 26 ids.
Remeasure before implementation; this is not a gate prediction:

| Spec | Where | Reads |
|---|---|---|
| 31.22, 31.23 | arithmetic.sh:367, :370 | `$(( $? + 7 ))` after `(( 0 ))` / `(( 1 ))` |
| sx.5 | arithmetic.sh:395 | `$(( (status) ))` |
| as.8 | arithmetic.sh:2073 | `$(( $? ))` and `$(( (status) ))` agree |
| cov.350-cov.353 | coverage.sh:1785-1808 | `int n=$(status)`; cov.351 uses `(( 1 ))`; cov.353 asserts SB5012 on a second read |
| isink.13-isink.15 | errors.sh:2926-2942 | `int n=$?`, `exit $?` (rc=1), `int n=$(status)` |
| spv.2, spv.3 | misc.sh:255, :258 | `echo $?`, `echo $(status)` |

Three places called it the idiom: coverage.sh:1782 ("`(( … ))` is the status
source because an uncaught `/bin/false` aborts the program (DD#55)"),
errors.sh:2925 ("`(( 0 ))` is the non-aborting way to set a failing status")
and `audit-validate-wildcard-escapes.md`:124. No DD sanctioned it. The
original scan of every spec for a bare `(( ))` that does not write found
no other reliance. It also found the comparison specs listed below, and
rdr.1 (errors.sh:2207). That spec's SB4046 called `(( 1 + 1 ))` "an
arithmetic test".

cov.259 (`int a=1; (( (a+=2), 1 ))`) also ends in a test part. Its status is
not read.

## Recorded causes and former contract conflict

The following source locations and DD quotations describe the original
investigation. DD31/DD55 now state the no-result contract above; their
old zero-status claim is superseded, not a second active rule.

- DD#31's table: a `(( ))` statement accepts mutations (`=`, `+=`, `-=`,
  `x++`, `x--`) and rejects comparisons. A test such as `(( a ))` is neither.
  The validator lets it through: `validate_arith_as_statement`
  (validate.rs 2938-2958) rejects only `ArithKind::Comparison`, and
  `classify_arith_expr` (arith.rs 318-350) answers Test for everything that is
  neither a write nor a comparison, `Paren` included (h1).
- DD31:67, "Statement `(( ))` exit status is always 0." That holds for a
  mutation (t5, h4). It is false for a test (t1, t3, t4) and for a
  parenthesised write (h1). A test lowers through
  `lower_arith_test_structured` (arith.rs 1610), which hoists any embedded
  write (`hoist_mutation_node`, transform/statements.rs 575) and sets the
  status from the value.
- DD55:252, "Arithmetic doesn't produce exit codes." DD55:247's "`(( 0 ))`
  kills the script" describes bash, and that half holds: nothing aborts.
- A lone comparison is SB5007 "comparison … as bare statement has no effect —
  use in if/while condition" (31.2, 31.14, 31.60-31.62, irbrief.2, cov.368,
  cov.369). Yet it would set `$?` exactly as `(( a ))` does. "No effect" is
  true only by DD31:67, which the test statement contradicts.

## Implementation and regression obligations

The alternative of sanctioning bare arithmetic status producers was not
selected. Nor was an always-zero mutation result retained. Use shared
arithmetic classification and statement traversal, not a private
effect-only test or a backend-specific status patch. Cover wrappers,
parentheses, split/unsplit comma forms and nested bodies. A mutation's
presence somewhere inside a value expression does not make that
expression a valid mutation statement.

Keep SB5007's arithmetic-context refusals meaningful: an unused comparison
is rejected for its result/context, not because its operands cannot have
effects. Give a prefix refusal a useful location and explain what must
be removed. No new diagnostic number or implementation architecture was
chosen by the policy ruling.

Re-source affected status fixtures from actual legal handling contexts
and preserve their original status/type/consumption assertions. Do not use
the old bare-call o1 or its proposed helper named `fail`; the former is
now an unconsumed non-void call and the latter is a builtin name. Review
cov.259 and the comma-tail cases too; a final unused value is no longer
a positive control merely because an earlier part writes.

Coordinate ordinary assignment-prefix refusals and R2's operand barrier
with `reject-nonadjacent-suppression`. R2's `- (( ... ))` now requires
compile-time refusal, not merely a runtime abort. Review arithmetic
no-reach fixtures, including noreach.14, without treating that earlier
refusal as proof that the lowering barrier works. Preserve the no-reach
claim on legal paths, and account for any implementation path made
unreachable by the source restriction.

Retain positive controls for ordinary/parenthesized writes, legal
mutation-only statement commas, consumed comparisons and condition
commas, pure value expressions, legal reads of prior status and
capture-condition results. Pin the intended diagnostic in negative
controls, not any error. Preserve type/taint checks and fatal faults.
Run both backends, prove focused behavior against the before binary and
independent reversions, and allocate fresh ids for changed contracts.
Predict NEW/RETIRED/FLIPS/TOTAL only after measuring the current suite.

Probes: `files/port-f1f4-probes.tar.gz`, `ts/` (t1-t6, h1-h4, o1-o6, b1-b3).
