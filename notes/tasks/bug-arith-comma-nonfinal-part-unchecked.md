---
title: "Bug: a comma's non-final part is checked only on an unsplit comma in a condition — `(( a, a++ ))`, `if (( a, a++, a<3 ))` and `(( a<3, a>1 ))` compile, though DD#31 says each non-final part must be a mutation"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "every row of the first table is SB5007 \"non-final comma expression … must be a mutation\" in both backends, as a statement, in the background and in a condition alike"
  - "c3 gets that message, not the bare-statement advice \"use in if/while condition\""
  - "the controls keep their verdicts; cov.259 `(( (a+=2), 1 ))` stays legal unless DG rules that a parenthesised write is not a mutation (see Fix direction)"
  - "w1 either compiles, under that ruling, or is rejected with a message that is true"
  - "the final part of a statement comma follows `q-arith-test-statement-status`; this task does not decide it"
  - "31.57 and cov.2 assert the message they were written for, not the bare code (see Spec impact)"
  - "a spec pins each row in both modes, and each fails on the pre-fix binary"
  - "a DD#31 addendum corrects \"a comma holding a mutation anywhere classifies as a mutation\" and \"checked on the comma itself, because nothing else can see it\""
---

## Found

2026-10-27, while porting F1+F4 (journal 537), in the probes for DD#31's
2026-10-27 addendum. It is **pre-existing**: every row is the same at
`8e28c61` and with F1+F4 in both backends, and at `1309385` and on
`taint-walker-redesign` (`e98fca8`) natively.

Each program starts `int a=0` and ends `echo "a=$a"`.

| Id | Program | Position | Result |
|---|---|---|---|
| pc1 | `(( a<3, a>1 ))` | statement | `a=0` |
| pc2 | `(( a<3, a>1 )) &`, then `- wait` | background | `a=0` |
| pc4 | `(( a, a ))` | statement | `a=0` |
| pc7 | `(( a>1, a<3 ))` | statement | `a=0` |
| m3 | `(( a, a++ ))` | statement | `a=1` |
| m4 | `(( a + 1, a++ ))` | statement | `a=1` |
| c1 | `if (( a, a++, a<3 )) { echo y }` | condition | `y`, `a=1` |
| c2 | `if (( a + 1, a++, a<3 )) { echo y }` | condition | `y`, `a=1` |
| c4 | `while (( a, a++, a<3 )) { echo "w$a" }` | condition | `w1`, `w2`, `a=3` |
| c5 | `if (( a++, a, a<3 )) { echo y }` | condition | `y`, `a=1` |
| c3 | `if (( a<3, a++, a<3 )) { echo y }` | condition | SB5007 2:4 "comparison a < 3 as bare statement has no effect — use in if/while condition": rejected, with advice that is wrong inside an `if` |

Controls, right today:

| Id | Program | Result |
|---|---|---|
| pc8 | `if (( a<3, a>1 )) { echo y }` | SB5007 2:4 "non-final comma expression a < 3 must be a mutation" |
| m1, m7 | `(( a++, a<3 ))`, `(( a++, a<3, a++ ))` | SB5007 2:1 "comparison a < 3 as bare statement has no effect …" |
| m2 | `(( a<3, a++ ))` | the same message, for a non-final part |
| m5, m6 | `(( a++, a ))`, `(( a++, a + 1 ))` | compile, `a=1`; the final part is the question's |
| cov.259 | `int a=1; (( (a+=2), 1 ))` | compiles, `a` is 3: a hoisted write |
| w2 | `int a=1; (( (a+=2), a>0 ))` | compiles, `rc=0 a=3` |

The same blind spot shows in a condition, where the program is rejected for
the wrong reason:

| Id | Program | Result |
|---|---|---|
| w1 | `int a=1; if (( (a+=2), a>0 )) { echo y }` | SB5007 2:4 "non-final comma expression ... must be a mutation", for a part that writes |

## Why

DD#31's comma rule is not scoped to conditions: "Each non-final expression
must be a mutation." It adds that this half "is checked on the comma itself,
because nothing else can see it". That check is
`validate_arith_comma_in_condition` (validate.rs 2916). It is reached only from
`validate_arith_in_condition`, and only while the comma is still a comma:

- A comma with a part that is itself a write (`a++`, `a = 1`) classifies as a
  mutation (`classify_arith_expr`, arith.rs 318-350). `lower_arith_mutation_structured`
  then splits it, before validation, into a `StmtKind::Group` with
  `GroupOrigin::ArithComma` and one statement per part.
  `validate_arith_as_statement` (validate.rs 2938) checks each part as a
  statement and rejects only `ArithKind::Comparison`. So a non-final `a` or
  `a + 1` passes in either position (m3, m4, c1, c2, c4, c5). A non-final
  comparison gets the bare-statement message, even inside an `if` (c3).
- A comma with no such part is `ArithKind::Test`, even when a part writes
  inside parentheses (w1, w2). Outside a condition,
  `validate_arith_as_statement` lets every Test through. So the comma is never
  checked (pc1, pc2, pc4, pc7), and neither is its final comparison, which
  alone would be SB5007 (pc1's `a>1`).

## Fix direction

One check on the comma, in every position: each non-final part is a mutation,
or else SB5007 "non-final comma expression X must be a mutation" (pc8's
message).

- A split comma is still visible after the split, as
  `Group { origin: GroupOrigin::ArithComma }`. F1+F4 added that tag. Check the
  group's non-final parts before the per-part statement checks, so that c3
  gets the comma message.
- A comma with no write and two or more parts always fails the check, so it is
  SB5007 in every position.
- Decide what counts as a mutation. cov.259's non-final `(a+=2)` writes, but
  `classify_arith_expr` answers Test for a `Paren` (its `_` arm), so a check
  built on the classifier flips cov.259. That is also why w1 reaches
  `validate_arith_comma_in_condition`, whose doc says only a mutation-free
  comma ever arrives there.
  - The check can look for a write anywhere in the part
    (`ir::mutation_target_in`, as the non-comma condition check does), or the
    classifier can peel `Paren`. Peeling changes more than this check: see h1
    in `q-arith-test-statement-status`.
  - Either way w1 then passes the non-final rule. It is not split today, and
    what the transform does with it in a condition is unprobed. If w1 should
    stay rejected, it needs a message that is true.
  - Which reading DD#31's "mutation" means is DG's to rule. irbrief.1's
    `(x + 1)` has no write and stays rejected under both.
- The final part keeps the ordinary rules.

F1+F4 did not touch these functions: `validate_arith_context`,
`validate_arith_in_condition`, `validate_arith_comma_in_condition`,
`validate_arith_as_statement` and `classify_arith_expr`. The branch did not fix
the bug. So it waits here (DG 2026-10-25).

## Spec impact

0 flips are predicted; verify with a reversion stub.

- Every comma in the suite with a non-final non-mutation part already expects
  SB5007 and keeps it: 31.57, 31.58, cov.2, cov.70 and irbrief.1.
- acm.4 (`(( a<3, a++ )) | /bin/cat`) is SB4022 at parse time first.
- cov.259 stays legal by the constraint above.

But 31.57 (`for (( ; k<3, k++; ))`) and cov.2 (`while (( a==1, b=2 ))`)
assert only the code. Today both report the final-part rule, "mutation of k
(b) not allowed in condition (( )) expression context". Both also have a
non-final comparison, so the new check would satisfy them on its own if that
rule were lost. Pin each to its message. cov.2's comment says it pins the
non-final rule; cov.70's comment and the probe say otherwise.

Probes: `files/port-f1f4-probes.tar.gz`, `pc/` (pc1-pc8, m1-m7, c1-c5, w1-w3,
and `s3157.sh`/`scov2.sh` for the two specs).
