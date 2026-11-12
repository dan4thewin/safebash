title: "Bug: before `&&`/`||`, an arithmetic comma list is still called a compound command, while its `!` twin compiles"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "DG rules on the question below, and DD#31 records the ruling"
  - "every row of the first table gets the ruling's verdict, in both backends; none calls the comma list a compound command"
  - "acm.11 and acm.12 (spec/errors.sh) are reviewed against the ruling; changed contracts receive fresh ids, and z4 gets coverage"
  - "`reject_arith_comma_list_op` (sash/src/parse/parser.rs) is deleted, or becomes the ruling's diagnostic"

## Found

2026-10-27, while porting F1+F4. Most of it was fixed there (journal 537;
DD#31 addendum 2026-10-27). A comma with a write, `(( a++, a<3 ))`, is held
as a group only so that a condition can take its last part, and the parser
classed that group with `{ … }` and `if`. Now a pipe, a redirect or a `&` on
one gets an assignment's answer (SB4022, SB4046, SB4024; acm.1-acm.10).

The part that remains is the left operand of `&&` and `||`. It keeps the
compound's SB4023 until the question below is ruled on. Each program starts
with `int a=0` and ends with `echo "a=$a"`. Results are on the journal-537
tree, the same in both backends. Rows marked † are the same at `8e28c61`.

| Id | Program | Result |
|---|---|---|
| p2 | `(( a++, a<3 )) && echo y` | SB4023 2:1 "`&&` / `\|\|` on a compound command isn't allowed — wrap the compound in a function" (acm.11) |
| z1 | `(( a++, a<3 )) \|\| echo y` | SB4023 2:1 † |
| x02 | `/bin/true && (( a++, a<3 )) && echo y` | SB4023 2:14 (acm.12) |
| d02 | `- (( a++, a<3 )) && echo y` | SB4023 2:3 |
| z4 | `if (( a++, a<3 )) && /bin/true { echo y }` | SB4023 2:4 † — inside an `if` condition |

Its twins already compile, with the last part deciding the list:

| Id | Program | Result |
|---|---|---|
| d05 | `! (( a++, a>3 )) && echo y` | `y`, `a=1` |
| z3 | `! (( a++, a<3 )) \|\| echo y` | `y`, `a=1` |
| x01 | `fn f { return (( a++, a<3 )) && echo y }; f` | `y`, `a=1` |
| x05 | `fn f { return (( a++, a<3 )) \|\| echo y }; f` | `a=1` |
| z5 | `while (( a++, a<3 )) { echo "w$a" }` | `w1`, `w2`, `a=3` |

A plain write in the same positions is read as a condition. Each of these
is SB5007 "mutation of a not allowed in condition (( )) expression context":

| Id | Program | Position |
|---|---|---|
| w06 | `(( a++ )) && echo y` | 2:1 |
| w26 | `(( a++ )) \|\| echo y` | 2:1 |
| y02 | `/bin/true && (( a++ )) && echo y` | 2:14 |
| y01 | `fn f { return (( a++ )) && echo y }; f` | 2:15 |

Probes: `files/port-f1f4-probes.tar.gz`, directories `ac5/` (x, y, w and
d/ rows) and `pc/` (z rows).

## The question: is an `&&` / `||` operand a condition position?

DD31's 2026-11-08 ruling settles results, not this comma-placement
question: writes produce no result, and a comparison cannot be an unused
arithmetic statement. d02 now requires rejection for its source `-`
independently of either placement choice. The tables above are historical;
return-kind enforcement also requires revalidating their bare-call fixtures.

DD#31 says "'Condition position' is all four of them": `if`, `while`,
`until`, and a C-style `for`'s test slot. It does not list an `&&`/`||`
operand.

- **(a) Yes.** p2, z1, x02 and z4 compile as d05 does, and DD#31 lists
  the operand as a fifth position. Recommended:
  - The SB5007 rows already read the operand as a condition.
  - The `!` and return-producer twins already compile.
  - z4 stands inside an `if`, which is a condition position by DD#31's own
    list.
- **(b) No.** Then this comma form is unavailable as a logical-list
  operand; it cannot fall back to a successful mutation status, because
  a write has no result. Plain writes remain invalid condition operands.
  d05, z3, x01 and x05 need the same comma-placement treatment. One
  diagnostic naming that restriction replaces SB4023, and DD31 explains it.

## Fix direction

The parser's `reject_arith_comma_list_op` is the whole hold. Its doc points
here.
- Under (a), delete it. Then check that the transform takes a group's last
  part as a list operand's status, as it already does under `!` (d05) and in
  a return producer (x01). Do not assume this.
- Under (b), it becomes that comma-placement diagnostic. The independent
  no-result-write and discarded-status-consumer guards remain.
