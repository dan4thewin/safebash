# Journal 094 — lower_expr_value arm helpers + empty_value dedup

## Goal

Fourth Phase 10 target. `lower_expr_value` (`codegen/values.rs`,
CC 26) — A-verdict from `r47-abstraction-audit.md:132`. Recipe
referred to "Itoa pattern duplicated (pattern #1)" but the real
duplication on the floor was the `(zero, zero)` empty-value
literal, repeated 5 times.

## Pattern

Two distinct issues:

1. **5-site `(zero, zero)` duplication.** The empty-value
   sentinel was open-coded as `let zero = iconst(I64, 0); (zero, zero)`
   in: Literal-not-found, var-not-found, Concat-empty-parts,
   ParameterExpansion-with-operator, and the wildcard arm.

2. **Multi-line inline arm bodies.** Each `ExprKind` variant did
   its own work in 5–40 lines, mixing the dispatcher with the per-
   arm logic. The Concat arm alone was 50 lines.

## Refactor

| helper | what it owns | CC |
| --- | --- | --: |
| `empty_value(builder)` | the 5-site `(zero, zero)` | 1 |
| `lower_literal(s, ...)` | literal-table lookup with empty fallback | 2 |
| `lower_simple_expansion(name, ...)` | $?/$$/var-with-int-or-str-branch | 6 |
| `lower_concat(parts, ...)` | 1/empty/multi-part sash_concat loop | (4) |
| `lower_subscript(name, index, ...)` | array key resolution + table_get | 10 |

Notable shape change: the `ParameterExpansion` arm used to
reconstruct a fake `Expr` (`SimpleExpansion(pe.name.clone())`) and
recurse through `lower_expr_value`. Now it calls
`lower_simple_expansion(&pe.name, …)` directly — same semantics,
no fake expr.

`lower_subscript` carries the bulkiest helper (CC 10) because the
key-type discrimination (numeric vs string) is irreducibly a
3-arm match feeding a 2-branch if-else.

## Result

| function | before | after |
| --- | --: | --: |
| `lower_expr_value` | 26 | 8 |
| `lower_simple_expansion` (new) | — | 6 |
| `lower_subscript` (new) | — | 10 |
| `lower_literal` (new) | — | 2 |
| `empty_value` (new) | — | 1 |

The dispatcher is now 5 arms + wildcard, each a single delegate or
small if-else. CC = 1 entry + 5 arms + 1 wildcard + 1 inline OR
(ParameterExpansion operator check) = 8.

## Insight

The audit recipe said "Itoa pattern duplicated" — that turned out
to be a less interesting issue than the `(zero, zero)` duplication.
Itoa (`emit_int_to_str`) was already a named helper called from 3
sites; that's not duplication, that's reuse. The real
duplication was the unnamed empty-value sentinel, which became
load-bearing across 5 arms and was never abstracted.

A 1-line helper for a 2-line literal pattern is normally noise —
but when the same 2 lines appear 5 times across distinct semantic
contexts (each meaning "couldn't compute a value here, default to
empty"), naming it makes the contract explicit. `empty_value` now
documents "return the zero sentinel that downstream lowering treats
as a 0-length string."

## Gates

- `cargo build --release` clean
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/codegen/values.rs` — extracted 5 helpers; dispatcher
  collapsed to single-line arms; ParameterExpansion-no-op now
  delegates directly to `lower_simple_expansion`.
- `notes/tasks/r47-abstraction-audit.md:132` — annotated.
