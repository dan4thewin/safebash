---
title: "Dispatch `[[ … ]]` requires spaces around `==`/`=~`, unlike the boolean test"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "`[[ $x==a { … } ]]` (compact operator) parses like `[[ $x == a { … } ]]`, OR the boolean test is made to match (both require spaces) — DG decides which direction"
  - "boolean `[[a==a]]` and dispatch `[[$x==a{…}]]` agree on operator spacing"
---

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
