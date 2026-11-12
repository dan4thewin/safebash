title: DD#49 — bless test anti-tautology check
status: done
depends: []
gates:
  - 24.10
  - 24.11
  - 24.16
  - tests
example: check_taint (sash/src/taint.rs)
index_keyword: bless
  - commit

SB4012 currently checks that `$VAR` appears somewhere in the bless
test expression. Strengthen: `$VAR` must appear on LHS of the test
operator, must NOT appear on RHS.

Rejects:
- `bless x [[ "$x" == "$x" ]]` — var on RHS (tautology)
- `bless x [[ "$x" =~ $x ]]` — var on RHS (self-match)
- `bless x [[ "$x" != "$x" ]]` — var on RHS (contradiction)

Accepts:
- `bless x [[ "$x" =~ ^[0-9]+$ ]]` — var on LHS, literal on RHS
- `bless x [[ "$x" == "expected" ]]` — var on LHS, literal on RHS
- `bless x [[ "$x" =~ $pattern ]]` — var on LHS, different var on RHS
- `bless x [[ -n "$x" ]]` — unary test, var is operand

Implementation: in taint.rs where SB4012 is checked, parse the test
expression to identify LHS/RHS of binary operators. Reject if `$VAR`
appears in RHS position.

Ref: DD#24 (bless section), DD#49
