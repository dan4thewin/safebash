---
title: "Dispatch `[[ … ]]` requires spaces around `==`/`=~`, unlike the boolean test"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "`[[ $x==a { … } ]]` (compact operator) parses like `[[ $x == a { … } ]]`, OR the boolean test is made to match (both require spaces) — DG decides which direction"
  - "boolean `[[a==a]]` and dispatch `[[$x==a{…}]]` agree on operator spacing"
---

## Resolution (2026-08-10, 3bca6b1)

DG chose **loosen** — the dispatch parser now accepts a compact `==`/`=~`,
matching the boolean test. Tightening was rejected because it would have
broken the compact boolean tests (`[[a==a]]`, H1/H3/H7) added the same day
in the DD#59 whitespace work. `parse_test_binary_tail` now calls
`extract_compact_operator` in its default arm, splitting a `Literal`/`Concat`
that embeds `==`/`=~` into a proper `Binary` (variable refs preserved).
Specs: `spec/dispatch.sh` I1–I8 (`dp.co1`–`dp.co8`). 0 native regressions.


## Context

Surfaced while making `[[`/`]]` whitespace-optional (DD#59 addendum,
2026-08-10). The **boolean** test parser accepts a compact operator —
`[[a==a]]` works — but the **dispatch** form parser requires the operator
to be space-delimited from the subject/pattern: `[[ $x==a { … } ]]` →
SB4028 ("the `[[ ]]` dispatch form needs `SUBJECT ==|=~ PATTERN`"), while
`[[ $x == a { … } ]]` is fine.

This is independent of bracket tokenization: `[[`/`]]` adjacency
(`[[$x == a { … } ]]`) works after the DD#59 change; only the interior
`==`/`=~` spacing differs between the two `[[ ]]` forms.

## Direction (needs DG)

Two ways to make them consistent:
- Loosen the dispatch parser to accept a compact `==`/`=~` (match the
  boolean test).
- Tighten the boolean test to also require spaces (match dispatch).

DG scoped the 2026-08-10 change to `[[`/`]]` whitespace only and asked to
file this separately rather than expand scope.

## Pointers

- Dispatch parse + SB4028 emission live in the dispatch/`[[ ]]` parser
  (`sash/src/parse/` — the `SUBJECT ==|=~ PATTERN` shape).
- Boolean `[[ ]]` test parsing accepts compact `==` today.
- Repro: `[[ $x==a { echo hit } || { echo no } ]]` (SB4028) vs
  `[[a==a]]` in an `if` (OK).
