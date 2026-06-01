# 061 — extracting lower_one transform helpers

## Background

`lower_one` in `sash/src/transform/statements.rs` was the
top hotspot after the two previous refactors (CC 55, 245
lines). It dispatches on `StmtKind` with 15 variants, but
five arms had grown to 25-80 lines of inline logic each.

Originally audited as **C** ("dispatcher that mostly recurses
over children"), upgraded to **A** when the inline arms were
recognized as substantive enough to extract. No p3 todo
existed — added `p3-lower-one` (audit Wave 2 implicit).

## What changed

Extracted five sub-handlers per the audit recommendation
(Until-desugaring, Function, Redirected, Suppressed) plus
ArithTest which was also substantive (22 lines):

1. `lower_while_stmt(w, ...)` — handles both `while` and
   `until` (via desugar-to-`while`-+-Not-patch pattern).
2. `lower_function_stmt(f, stmt, ...)` — function registration
   for inline expansion. Special cases: `__sash_end__` (DD#56
   exit trap), `__local__*` (DD#18 local scope), `-> int`
   return type tracking, DD#55 non-void detection + SB4017
   "function only returns 0" diagnostic.
3. `lower_redirected_stmt(r, ...)` — redirected statements;
   bracket compound bodies with `__redirect_push/_pop`,
   attach to the last Command/Pipeline/ReadLine* inst for
   simple commands.
4. `lower_arith_test_stmt(expr, stmt, ..., &mut block)` —
   `(( expr ))` as a condition. Takes `&mut block` because
   `lift_int_func_calls` needs to update the current block.
5. `lower_suppressed_stmt(inner, ...)` — DD#55 `- cmd`
   handling; lowers inner then walks emitted insts to mark
   them unchecked.

Plus one small helper:

- `uncheck_block_from(ssa, block_id, from)` — lifted from
  the nested `fn` inside the original Suppressed arm.

Parent `lower_one` is now a clean 32-line dispatcher: one
line per `StmtKind` variant, no inline logic anywhere.

## Results

| function                  | before | after |
| ------------------------- | -----: | ----: |
| `lower_one`               |     55 |    17 |
| `lower_redirected_stmt`   |      — |    15 |
| `lower_function_stmt`     |      — |    14 |
| `lower_while_stmt`        |      — |     5 |
| `uncheck_block_from`      |      — |     5 |
| `lower_suppressed_stmt`   |      — |     3 |
| `lower_arith_test_stmt`   |      — |     1 |

Parent CC 55 → **17** (15 match arms + dispatch overhead).
Drops out of top-10. Max helper CC is 15.

Verification:
- `cargo build --release`: clean
- `cargo clippy --release --lib -D warnings`: clean
- `cargo test -p sash --lib`: 12/12
- spec: 291/291

## Audit staleness note

The audit's `handle_command` (transform) entry listed CC 48
in Wave 2 #4, but the indexed live CC is 26 — already
heavily decomposed by prior commits. Skipped that task
(reverted to pending) and went to the actual top hotspot
(`lower_one`, 55). Should sweep the audit document for
other stale CC values when next reviewing it.

## Top hotspot list after this commit

| CC | function | file |
| --: | --- | --- |
| 49 | `check_stmt` | `sash/src/type_check.rs:243` |
| 48 | `print_arith_expr` | `sash/src/printer.rs:446` (B/whitelist) |
| 48 | `lower_value` | `sash/src/codegen/values.rs:261` |
| 48 | `to_tok` | `sash/src/parse/logos_tokens.rs:137` (B/whitelist) |
| 47 | `scan_word` | `sash/src/parse/lexer.rs:1095` |
| 44 | `arith_expr_to_janet` | `sash/src/emit_janet.rs:1246` (B/whitelist) |
| 44 | `parse_parameter_expansion` | `sash/src/parse/expr.rs:333` |
| 43 | `print_stmt` | `sash/src/printer.rs:27` |
| 43 | `validate_stmt_ctx` | `sash/src/validate.rs:69` |

Next A-verdict targets: `lower_value` (codegen, 48),
`scan_word` (lexer, 47), `parse_parameter_expansion`
(parser, 44). `check_stmt` (49) is C-verdict — defer until
A-list cleared.
