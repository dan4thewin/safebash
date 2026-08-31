---
title: "grind: dispatch compact-operator Literal/leading-op branches are never-run (extract_from_literal, extract_leading_operator)"
status: ready
priority: P3
kind: coverage
depends: []
gates:
  - "extract_from_literal and extract_leading_operator either flip 0→>0 with a targeted dispatch spec, or are proven shadowed-dead and removed"
---

## What

Two functions in the dispatch compact-operator parse are never-run by the spec
suite (surfaced in the 2026-08-31 coverage grind):

- `extract_from_literal` (parser.rs) — the `ExprKind::Literal` arm of
  `extract_compact_operator` (a dispatch subject that is a *single* `Literal`
  token embedding `==`/`=~`, e.g. `[[ a==a { … } ]]` with a literal subject).
- `extract_leading_operator` (parser.rs) — the "operator glued to the **last**
  Concat part" branch of `extract_from_concat` (`Concat([…, Literal("==rest")])`).

`extract_compact_operator` itself flipped covered when the dispatch
compact-operator support landed (`dispatch-compact-operator-spacing`, done
2026-08-10, ee3a2fa) — but the `dp.co1`–`dp.co8` specs only exercise the
**middle-part Concat** path (a `$var` subject → `Concat([$x, "==", a])`), leaving
these two sibling branches untested.

## The catch (why this isn't just "add a spec")

A passing probe does NOT prove the branch ran (the classic grind caveat). A pure
`[[ a==a { echo } ]]` compiles, but the lexer may split `a==a` into
`a` / `==` / `a` (three tokens → the normal split-operator path or the middle
Concat path), never producing the single `Literal("a==a")` that
`extract_from_literal` needs. If the lexer *always* splits `==`/`=~`, both
branches are **shadowed-dead** (bucket B → remove, like `strip_matching_quotes`).

## Do

1. Measure (`mk cov-spec-only` → per-function count) which dispatch input, if
   any, flips `extract_from_literal` / `extract_leading_operator`. Candidate
   forms to try: a quoted glued subject (`[[ "a==a" { … } ]]`), a subject that
   forces a single Literal token, and a `$x`-prefixed glued last part
   (`[[ pre$x==a { … } ]]` → `Concat([pre, $x, "==a"]?)`).
2. If a form flips them → add it to `spec/dispatch.sh` (bucket A).
3. If nothing reaches them (the lexer always splits the operator) → delete both
   (and the now-dead `extract_from_concat` last-part branch), remove-then-run to
   confirm 0 regressions (bucket B).

## Pointers

- `extract_compact_operator` parser.rs:1317 → `extract_from_literal` (1326) /
  `extract_from_concat` (1381) → `extract_leading_operator` (1409).
- `parse_test_binary_tail` default arm (~1474) is the single caller.
- Specs: `spec/dispatch.sh` `dp.co1`–`dp.co8`.
- Accounting: `notes/coverage-accounting.md` 2026-08-31 section.
