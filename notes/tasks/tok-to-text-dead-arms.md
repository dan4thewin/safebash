---
title: "Remove four unreachable arms from `tok_to_text`"
status: next
priority: P4
kind: cleanup
depends: []
gates:
  - "`LParen`/`RParen`/`DblParen`/`DblParenClose` arms removed from `tok_to_text`"
  - "the remaining default is `unreachable!()` or an explicit diagnostic, not a silent fallthrough"
  - "spec suite unchanged; no new specs needed (the arms are unreachable)"
---

# Four dead arms in `tok_to_text`

`sash/src/parse/arith.rs::tok_to_text` (≈1523) has arms for `Tok::LParen`,
`RParen`, `DblParen` and `DblParenClose`. They are **unreachable**.

Its only caller is `step_default_token`, which is reached from
`step_arith_collect` — and `step_arith_collect` matches those four token kinds
**itself**, before falling through to the default step. So no call can arrive
carrying one of them.

Found while working the cold-line queue (journal 383): they showed as
uncovered lines and stayed uncovered under every input, which is the signature
of dead code rather than a coverage gap.

## Why it is worth doing

Not the four lines. The arms make `tok_to_text` read as if it were a
general token→text function, so the next person to add a caller will believe
those cases are handled. They are only correct as long as every caller
pre-filters exactly the way `step_arith_collect` does — an invariant nothing
states.

## Note

`tok_to_text` is also a top-5 cyclomatic hotspot, so this edit will trip
the grandfather-on-modify complexity gate. Removing four arms lowers cc, so
the gate should pass, but check before assuming: measure the means fresh
(`AVG(cyclomatic), AVG(cognitive)` over `sash/src/%`) rather than quoting
older numbers.
