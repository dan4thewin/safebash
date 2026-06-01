# 063 — extracting scan_word lexer helpers

## Background

`scan_word` in `sash/src/parse/lexer.rs` was 102 lines, CC 47.
Two distinct phases were packed into one fn:

1. The character-scanning loop (word-break dispatch, with nested
   ifs for `==`, `+=`, bare `=` at word start) — about 10 CC of
   control flow.
2. A 30-arm keyword-classification match — about 30 CC, but
   **intrinsic** (each keyword contributes exactly 1).

Audit verdict: **A**. The win is isolating the phases, not
shrinking the keyword table.

## What changed

Two-helper split:

1. `scan_word_chars(&mut self, start, &mut word) -> Option<SpannedToken>`
   — runs the byte loop, appends to `word` by-ref. Returns
   `Some(eq_tok)` only when a bare `=` at word start was consumed
   (so the caller can emit it directly without confusion with the
   accumulated-word path); `None` after a normal break or EOF.
2. `classify_word_token(word: String) -> Tok` (free fn, not
   method — pure mapping with no `self`) — the 30-arm keyword
   match.

`scan_word` itself is now a 22-line dispatcher: scan, handle the
Eq short-circuit, empty-check, classify.

## Results

| function              | before | after |
| --------------------- | -----: | ----: |
| `scan_word`           |     47 |     3 |
| `scan_word_chars`     |      — |    12 |
| `classify_word_token` |      — |    35 |

Parent CC **47 → 3**. The classifier helper is 35 (one CC per
keyword); this is intrinsic and the right place for it — it sits
out of the way as a pure top-level function.

Codebase max CC (after this commit): 49 (`check_stmt`,
C-verdict). Top-15 list:

| CC | function | file |
| --: | --- | --- |
| 49 | `check_stmt` | `sash/src/type_check.rs:243` (C-verdict) |
| 48 | `print_arith_expr` | `sash/src/printer.rs:446` (B/whitelist) |
| 48 | `to_tok` | `sash/src/parse/logos_tokens.rs:137` (B/whitelist) |
| 44 | `arith_expr_to_janet` | `sash/src/emit_janet.rs:1246` (B/whitelist) |
| 44 | `parse_parameter_expansion` | `sash/src/parse/expr.rs:333` |
| 43 | `print_stmt` | `sash/src/printer.rs:27` |
| 43 | `validate_stmt_ctx` | `sash/src/validate.rs:69` |
| 41 | `emit_param_expansion_inline` | `sash/src/emit_janet.rs` |
| 40 | `value_is_tainted` | `sash/src/ssa_taint.rs` |
| 40 | `lower_case` | `sash/src/transform/control_flow.rs` |
| 35 | `classify_word_token` | `sash/src/parse/lexer.rs` (intrinsic) |

Verification: build clean, clippy clean, sash lib 12/12, spec
291/291.

## Note on the missing whitelist

The `.cyclo-allow` file was retired (commit `967af30` —
"adopt indexnudge v9 + retire contrib/cyclo.sh"); the pre-commit
hook now reports the unfiltered top-5. The `classify_word_token`
helper, like `to_tok`, `print_arith_expr`, and
`arith_expr_to_janet`, is intrinsic-CC and would have been
whitelisted under the old scheme. It belongs at the top of the
report list as a documented expected hotspot, not as a refactor
target.

## Next A-verdict targets

`parse_parameter_expansion` (parser, 44), `validate_stmt_ctx`
(validate, 43), `lower_case` (transform, 40),
`value_is_tainted` (ssa_taint, 40). `check_stmt` (49) is
C-verdict — defer until A-list cleared.
