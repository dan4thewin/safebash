# 191 — retire the `${#…}` length sugar (DD#75 §2 amended)

Found while coverage-grinding (journal 190): the systematic "fully-uncovered
function" scan surfaced `codegen/value_handlers.rs::emit_strlen_elem`
("${#arr[N]} — string length of element N"). Probing it exposed a bug.

## The bug

`${#a[0]}` (length of array element 0) silently returned **0** (bash: the
element's byte length). Root cause: the parser's `try_strlen_or_arrlen` only
special-cased `[@]`/`[*]`; any other subscript fell through to `strlen` of a
variable *literally named* `a[0]`, which doesn't exist → empty → 0. Meanwhile
`emit_strlen_elem` existed to compute element length but had **no front-end
producer** — nothing ever emitted its `strlen_elem` operator. Element-length was
half-wired and never worked: a silent wrong answer.

## DG decision (Policy C)

Surfaced the fork (bug policy = stop-everything; the fix is a language-surface
choice). Options: (A) keep bare `${#name}`, reject subscripted `${#a[...]}`;
(B) implement element-length; (C) drop `${#…}` **entirely**, `$(len …)` /
`(len …)` the sole spelling. DG chose **C** (converter rewrites bash `${#x}` →
`$(len x)`, keeps `len` as the intrinsic name).

Rationale: a `${#…}` `$`-sigil hides a `len` call — the same "looks scalar,
isn't" hazard that removed `$@`/`$*`. One length spelling > paste-friendliness,
and the converter rewrite keeps pasted bash working.

## Implementation

- **parse/expr.rs** `try_strlen_or_arrlen`: every `${#…}` (bare, `[@]`, `[i]`)
  now yields the `len_retired` sentinel (was `strlen` / `arrlen_retired`).
- **parse/parser.rs** `retired_bracket_operator_diag`: `len_retired` → SB4038
  "the `${#…}` length sugar is retired — use `$(len x)` / `(len x)`". Dropped
  the `arrlen_retired` arm (subsumed).
- **parse/arith.rs**: removed the `ATok::LenOf` token + `braced_length_token` +
  its parse arm. `${#a}` inside `$(( ))` now falls through to the parameter-
  expansion path and hits the same real-parser reject. (`(len a)` unaffected.)
- **codegen/value_handlers.rs**: deleted the dead `emit_strlen_elem` + its
  `strlen_elem` dispatch arm.

`$(len x)` / `(len x)` are untouched — they converge on the same live
`strlen`/`arrlen` ParamExpansion operators via `try_len_intrinsic`, so nothing
`len`-related was disturbed.

## Spec migration + DD

- `types.sh`: 12.1/12.3/30.3/30.11 → `$(len …)`; **75.4/75.5/75.6 repurposed**
  to assert the retirement (SB4038, value + `$(( )))`; 75.13 comment.
- `arithmetic.sh` 58.14-58.16 → `(len …)`. `var_modifiers.sh` bvb.4 → SB4038
  (the length-form reject fires before the env-var check).
- DD#75 §2 amended (the "only surviving `${#…}` form" claim reversed); §4/§6
  tables updated; SUBSET.md length row → `$(len var)`.

Converter rewrite (`${#x}` → `$(len x)`) is the next commit.

Native gate 0 regressions; the 3 janet types failures are pre-existing
(16.1/33.10 baseline-failing; unrelated to `${#}`).
