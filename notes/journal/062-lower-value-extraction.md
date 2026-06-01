# 062 — extracting lower_value codegen helpers

## Background

`lower_value` in `sash/src/codegen/values.rs` was a 459-line
match on `SsaValue` with 14 arms (CC 48). Several arms — Concat,
Subscript, TableValue, TableKey, Ternary, IndexedArray — had
grown into 30-60-line blocks of inline cranelift IR emission with
shared idioms (stack slot allocation, array-table key lookup,
atoi-on-string-var) duplicated across them.

Audit verdict: **A**. Itoa-from-i64 was pattern #1 across $?, $$,
int var, ArithExpr, ArithBinOp arms — already cleared by Wave 1
extraction of `emit_int_to_str`. Per-arm decomposition was the
remaining win.

## What changed

Dispatcher + per-arm handlers, mirroring the pattern from journals
060/061.

Two shared utilities:

1. `alloc_out_slot(builder) -> (StackSlot, Value)` — sized stack
   slot + addr (used by Concat, Subscript int-key, TableValue,
   TableKey, IndexedArray).
2. `lookup_array_table(vars, array) -> Option<Variable>` — find
   the canonical `array_*_0` table variable for an array name
   (used by Subscript, TableValue, TableKey).

One small helper for the duplicated atoi-on-var pattern (TableValue
and TableKey):

- `lookup_atoi_var(builder, externs, vars, name) -> Value`

Eleven per-arm handlers:

- `lower_literal_value`
- `lower_var_value` — $?, $$, undefined, int var, str var
- `lower_func_call_value`
- `lower_concat_value`
- `lower_assoc_array_value`
- `lower_subscript_value` — int-key and string-key paths
- `lower_argv_index_value`
- `lower_table_value_value` — int-key and var-name-as-key paths
- `lower_table_key_value` — same shape, swap value/key
- `lower_ternary_value`
- `lower_indexed_array_value`

The ArithExpr and ArithBinOp arms stay inline in the dispatcher —
they reduce to a single `lower_value_as_int` call + `emit_int_to_str`
each (already factored).

Three of the helpers needed `#[allow(clippy::too_many_arguments)]`
(`lower_concat_value`, `lower_assoc_array_value`,
`lower_indexed_array_value`) — they take the standard
8-context-arg bundle plus the arm's own data. Matches existing
file conventions.

## Results

| function                    | before | after |
| --------------------------- | -----: | ----: |
| `lower_value`               |     48 |    17 |
| `lower_var_value`           |      — |    10 |
| `lookup_array_table`        |      — |     5 |
| `lower_func_call_value`     |      — |     3 |
| `lower_assoc_array_value`   |      — |     3 |
| `lower_concat_value`        |      — |     2 |
| `lower_subscript_value`     |      — |     2 |
| `lower_argv_index_value`    |      — |     2 |
| `lower_indexed_array_value` |      — |     2 |
| `lookup_atoi_var`           |      — |     2 |
| `lower_literal_value`       |      — |     1 |
| `lower_table_value_value`   |      — |     1 |
| `lower_table_key_value`     |      — |     1 |
| `lower_ternary_value`       |      — |     1 |
| `alloc_out_slot`            |      — |     1 |

Parent CC **48 → 17** (one match arm per SsaValue variant +
dispatch overhead). Max helper CC is 10 (`lower_var_value`, which
already mirrors the 5-case VarRef match shape). Most are CC ≤ 3.

Total lines in `lower_value` body dropped from 459 to 164 (a
dispatcher's worth).

Verification:
- `cargo build --release`: clean
- `cargo clippy --release --lib -D warnings`: clean
- `cargo test -p sash --lib`: 12/12
- spec: 291/291

## Top hotspot list after this commit

| CC | function | file |
| --: | --- | --- |
| 49 | `check_stmt` | `sash/src/type_check.rs:243` (C-verdict) |
| 48 | `print_arith_expr` | `sash/src/printer.rs:446` (B/whitelist) |
| 48 | `to_tok` | `sash/src/parse/logos_tokens.rs:137` (B/whitelist) |
| 47 | `scan_word` | `sash/src/parse/lexer.rs:1095` |
| 44 | `arith_expr_to_janet` | `sash/src/emit_janet.rs:1246` (B/whitelist) |
| 44 | `parse_parameter_expansion` | `sash/src/parse/expr.rs:333` |
| 43 | `print_stmt` | `sash/src/printer.rs:27` |
| 43 | `validate_stmt_ctx` | `sash/src/validate.rs:69` |
| 41 | `emit_param_expansion_inline` | `sash/src/emit_janet.rs` |
| 40 | `value_is_tainted` | `sash/src/ssa_taint.rs` |
| 40 | `lower_case` | `sash/src/transform/control_flow.rs` |

Codebase max CC: 55 → 49. With the top two now C/B verdicts, next
real A targets are `scan_word` (lexer, 47),
`parse_parameter_expansion` (parser, 44), `validate_stmt_ctx`
(43), `lower_case` (40).
