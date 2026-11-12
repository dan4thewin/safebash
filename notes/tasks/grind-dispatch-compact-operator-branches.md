---
title: "grind: dispatch compact-operator Literal/leading-op branches are never-run (extract_from_literal, extract_leading_operator)"
status: done
priority: P3
kind: coverage
depends: []
gates:
  - "extract_from_literal and extract_leading_operator either flip 0→>0 with a targeted dispatch spec, or are proven shadowed-dead and removed"
---

## Resolution (2026-08-31) — shadowed-dead, REMOVED (bucket B)

Settled by an eprintln probe on `extract_compact_operator` /
`extract_from_literal` / `extract_leading_operator`, run against a comprehensive
`[[ ]]` battery (dispatch + boolean, compact + spaced, quoted + bare): **neither
sub-branch ever fired.** Every compact operand — `a==a`, `$x==a`, `pre$x==post`,
`ab==cd==ef` — arrives at `extract_compact_operator` as a `Concat` whose `==`/`=~`
is its own **middle** part (`Concat([$x, "==", a])`), because the `[[ ]]` lexer
always splits the operator into a separate token. So:

- `extract_from_literal` needed a single glued `Literal("a==b")` — unreachable
  (the operand is always split into a Concat). A quoted `"a==a"` arrives as
  `DoubleQuoted` and a `'a==a'` as `SingleQuoted` → `_ => None` (correctly *not*
  treated as an operator).
- `extract_leading_operator` needed the last Concat part to be `"==rest"` —
  unreachable (the operator is always its own part, never glued to the pattern).

Only `extract_from_concat`'s middle-part branch (`match_operator` on the interior
`==`/`=~` Literal) is live, and it covers every `dp.co1`–`dp.co8` case.

Removed `extract_from_literal`, `extract_leading_operator`, the `Literal` arm of
`extract_compact_operator`, and the last-part branch of `extract_from_concat`.
remove-then-run: all dispatch + boolean compact forms still work; full 3-mode
sweep **0 regressions**. (Original never-run finding — journal 217 / accounting
2026-08-31.)


## Background (original finding)

Two functions in the dispatch compact-operator parse are never-run by the spec
suite (surfaced in the 2026-08-31 coverage grind):

- `extract_from_literal` (parser.rs) — the `ExprKind::Literal` arm of
  `extract_compact_operator` (a dispatch subject that is a *single* `Literal`
  token embedding `==`/`=~`, e.g. `[[ a==a { … } ]]` with a literal subject).
- `extract_leading_operator` (parser.rs) — the "operator glued to the **last**
  Concat part" branch of `extract_from_concat` (`Concat([…, Literal("==rest")])`).

`extract_compact_operator` itself flipped covered when the dispatch
compact-operator support landed (`dispatch-compact-operator-spacing`, done
2026-08-10, 3bca6b1) — but the `dp.co1`–`dp.co8` specs only exercise the
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
- Accounting: [historical coverage ledger](https://github.com/dan4thewin/safebash/blob/f02477dd31e67a0fe6fe3c4a359935569c4715d8/notes/coverage-accounting.md), 2026-08-31 section.
