title: "Bug: an array literal's elements are lexed as command arguments, against DD#71's operand-word rule"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "an unquoted `|`, `&`, `&&`, `||`, `{` or `}` inside an indexed element or a map value is a literal character of the element, as it is in a scalar value (m7, ar15, ar16), in both backends"
  - "DG rules on Q1 and Q2; DD#71's table records both rulings and says that `)` closes the literal"
  - "ar6 gets one diagnostic, naming the `;`"
  - "no program in the first table reports a bare \"expected expression\""
  - "specs pin each operator in an indexed element and in a map value, in both modes"

## Found

2026-10-27, while porting F1+F4 (journal 537). It is **pre-existing**: every
result is the same at `8e28c61` (before F1+F4) and with F1+F4, in both
backends. Each program ends with `echo "${a[0]}"` (or `"${m[k]}"`).

| Id | Program | Result |
|---|---|---|
| ar2 | `a=(x\|y)` | SB4014 1:5 "expected expression" |
| ar1 | `array a=(x\|y)` | SB4014 1:11, the same |
| m1 | `array a=(x \| y)` | SB4014 1:12 |
| ar18 | `array a=(x\|y z)` | SB4014 1:11 |
| ar5 | `array a=(x&y)` | SB4014 1:11 |
| m5 | `array a=(x &)` | SB4014 1:12 |
| ar12 | `array a=(x&&y)` | SB4014 1:11 |
| m4 | `array a=(x && y)` | SB4014 1:12 |
| ar11 | `array a=(x\|\|y)` | SB4014 1:11 |
| ar10 | `array a=(x{y})` | SB4014 1:11 |
| ar13 | `array a=(x}y)` | SB4014 1:11 |
| ar20 | `array a=(x(y)z)` | SB4014 1:11 |
| ar3 | `array a=(x>y)` | SB4014 1:11 |
| ar4 | `array a=(x<y)` | SB4014 1:11 |
| ar6 | `array a=(x;y)` | SB4014 1:11 and SB4014 1:13 |
| m3 | `map m=([k]=a\|b)` | SB4014 1:13 |
| ar19 | `map m=([k]=a&&b)` | SB4014 1:13 |
| ar7 | `map m=([k]=a>b)` | SB4014 1:13 |

The scalar twins, and the controls:

| Id | Program | Result |
|---|---|---|
| m7 | `x=a\|b` | `a\|b` |
| ar16 | `x=x\|\|y` | `x\|\|y` |
| ar15 | `x=x{y}` | `x{y}` |
| ar21 | `x=x(y)z` | `x(y)z` |
| ar14 | `x=x!y` | `x!y` |
| ar9 | `array a=(x!y)` | `x!y` |
| ar8 | `x=a>b` | SB4046 1:1 "a redirect cannot attach to a variable assignment" |
| ar23 | `x=a<b` | SB4046 1:1, the same |
| m8b | `array a=("x\|y")` | `x\|y` |
| ar17 | `array a=(x "\|" y)` then `echo "${a[1]}"` | `\|` |

DD#71's table lists "array elements" with the assignment RHS: an operand
word, terminated by whitespace, `;` or newline. Every row of the first table
breaks that rule, and the message names no operator.

Probes: `files/port-f1f4-probes.tar.gz`, directories `b28/` (m1–m8),
`fin2/` (ar1, ar2, m8b) and `adv/` (ar3–ar23).

## Cause (read, not yet confirmed by a fix)

`parse_array_literal` and `try_parse_assoc_element`
(sash/src/parse/expr.rs) parse each element with `parse_expr`, which is
`AbsorbMode::None`: command-argument lexing. A connector, brace, paren or
redirect token ends the word, the loop calls `parse_expr` again at that
token, and `parts_to_expr` reports "expected expression" for the empty word.
A scalar value uses `parse_assignment_value` (`AbsorbMode::Assign`), which
absorbs `( ) { } | ! & && ||` but not `<`/`>`; that is why ar8 is SB4046.

F1+F4 did not modify either function, so this goes to the backlog under DG's
rule for the port (fix a known bug in a function the port modifies; file the
others).

## Fix direction

Give elements their own absorb mode. It cannot be `AbsorbMode::Assign` as
is: that mode absorbs `)`, and `)` is what closes the literal, so
`array a=(x)` would lose its close. The new mode absorbs `| & && || { } !`
and leaves `)` (and `(`, below) to the literal. `;` inside a literal
sequences nothing, so ar6 should get one diagnostic that names it.

## Q1. `<` and `>` in an element

DD#71 does not mention them. In a scalar value they start a redirect, which
SB4046 rejects (ar8, ar23). An array literal has no command to redirect.

- **(a) Reject them in an element, with a diagnostic that names the
  operator and says to quote it.** Recommended: it matches the scalar
  verdict without claiming there is a redirect.
- **(b) Absorb them as literal characters in every value word.** This also
  changes ar8 and ar23, and DD#71's table.

## Q2. Parens inside an element

`x(y)z` is a literal in a scalar (ar21), but inside a literal `)` closes the
list.

- **(a) Reject an unquoted `(` or `)` inside an element, with a diagnostic
  that says to quote it.** Recommended.
- **(b) Balance parens inside an element.** `array a=(x(y)z)` would then
  hold `x(y)z`, at the cost of a second paren rule.
