---
title: "Question: a bare arithmetic test statement (`(( 0 ))`, `(( a ))`, `(( a + 1 ))`) compiles and sets `$?` from its value without aborting. The suite relies on `(( 0 ))` as its non-aborting failing status, but DD#31 accepts only mutations as a statement and says a statement's status is always 0"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on (a) or (b) below, or on a third reading"
  - "a DD#31 addendum records the ruling and corrects \"Statement `(( ))` exit status is always 0\" (DD31:67); DD#55's \"Arithmetic doesn't produce exit codes\" (DD55:252) gets the matching correction"
  - "under (a): the idiom comments (coverage.sh:1782, errors.sh:2925) cite the ruling, and a lone comparison either compiles too or stays SB5007 with a reason that is true"
  - "under (b): the 13 status specs are re-sourced first (o1's failing function works today), so the rejection itself flips none of them; each newly rejected spelling is then pinned in both modes and fails on the pre-change binary"
  - "h1's parenthesised write follows the same ruling as `bug-arith-comma-nonfinal-part-unchecked`'s question of what counts as a mutation"
---

## Found

2026-10-27, while porting F1+F4 (journal 537), in the probes for DD#31's
2026-10-27 addendum. It is **pre-existing**: every row is the same at
`8e28c61`, with F1+F4, and at `1309385`, in both backends.

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

None of them aborts. A false test sets the status without being a failed
command.

The only other source of a readable failing status that does not abort is a
status-returning function:

| Id | Program | Result |
|---|---|---|
| o1 | `fn f { return 1 }; f; echo "rc=$?"` | `rc=1` |
| b1 | `- /bin/false; echo "rc=$?"` | SB5012 "no preceding command" |
| o2 | `/bin/false \|\| :`, then `$?` | SB5012 |
| o3 | `if /bin/false { : }`, then `$?` | SB5012 |
| o4 | `fn f { return 1 }; - f`, then `$?` | SB5012 |

(SB5012's "no preceding command" is wrong in its own way:
`bug-sb5012-message-names-no-command`.)

## The suite relies on it

13 specs use `(( 0 ))`, or `(( 1 ))` for the success direction, as their
status source. All are dual-mode, so 26 ids:

| Spec | Where | Reads |
|---|---|---|
| 31.22, 31.23 | arithmetic.sh:367, :370 | `$(( $? + 7 ))` after `(( 0 ))` / `(( 1 ))` |
| sx.5 | arithmetic.sh:395 | `$(( (status) ))` |
| as.8 | arithmetic.sh:2073 | `$(( $? ))` and `$(( (status) ))` agree |
| cov.350-cov.353 | coverage.sh:1785-1808 | `int n=$(status)`; cov.351 uses `(( 1 ))`; cov.353 asserts SB5012 on a second read |
| isink.13-isink.15 | errors.sh:2926-2942 | `int n=$?`, `exit $?` (rc=1), `int n=$(status)` |
| spv.2, spv.3 | misc.sh:255, :258 | `echo $?`, `echo $(status)` |

Three places call it the idiom: coverage.sh:1782 ("`(( … ))` is the status
source because an uncaught `/bin/false` aborts the program (DD#55)"),
errors.sh:2925 ("`(( 0 ))` is the non-aborting way to set a failing status")
and `audit-validate-wildcard-escapes.md`:124. No DD sanctions it. A scan of
every spec for a bare `(( ))` that does not write found no other reliance. It
also found the comparison specs listed below, and rdr.1 (errors.sh:2207). That
spec's SB4046 calls `(( 1 + 1 ))` "an arithmetic test".

cov.259 (`int a=1; (( (a+=2), 1 ))`) also ends in a test part. Its status is
not read.

## What it conflicts with

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

## Options

**(a) Sanction it.** A bare arithmetic test sets the status from its value
and never aborts. It is the way to make a status without a command.

- DD#31's table gains the test as a statement form. Line 67 becomes "a
  mutation's status is 0; a test's is its truth value". DD55:252 changes to
  match.
- The 13 specs and their comments stand.
- Sub-question: a lone comparison then has an effect too. Either it compiles
  (the eight comparison specs above flip, 16 ids), or SB5007 keeps it with a
  reason that is true.

**(b) Reject it.** A statement holds only mutations, as the table says. Every
test statement becomes SB5007, and so does the final test part of a statement
comma (t4, and the comma task's m5/m6).

- First re-source the 13 specs, for example from `fn fail { return 1 }` (o1),
  so the rejection itself flips none of them.
- h1 depends on what counts as a mutation, which is
  `bug-arith-comma-nonfinal-part-unchecked`'s question. If a parenthesised
  write is a mutation, h1's status becomes 0. If not, h1 is rejected.

**Recommendation: (b)**, tentatively. The table is explicit, and the idiom
was invented in spec comments, not decided. A bare `(( a ))` that a user
writes is more likely a slip, meant as `(( a++ ))` or a condition, than a
request for a status. (a) costs nothing in specs but makes the statement
two-purpose, against DD#31's title.

It is not obviously a bug, because the suite was written to rely on it, so it
waits here (DG 2026-10-24).

Probes: `files/port-f1f4-probes.tar.gz`, `ts/` (t1-t6, h1-h4, o1-o6, b1-b3).
