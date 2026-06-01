# 059 — extracting handle_param_expansion helpers

## Background

`handle_param_expansion` in `sash/src/codegen/value_handlers.rs`
was the top-CC function in the tree (CC 58). Task
`p3-handle-param-expansion` called for per-operator helpers plus a
dispatcher. The function was 540 lines long and handled ~12
distinct parameter-expansion forms inline: `${var}` length,
glob/glob_regex, regex_replace, array iteration
(arrkeys/arrvals/arrjoin/arrvals_*), prefix/suffix strip,
default/alternate values, substring, case conversion, sed-style
slash replacement.

## What changed

Extracted four helpers from `handle_param_expansion`:

1. `emit_glob_expansion(operand, use_regex, ...)` — was two
   near-duplicate `if` blocks (glob and glob_regex differed only
   in the extern they call). Deduped into one helper.
2. `emit_regex_replace_op(operand, s_ptr, s_len, ...)` — was
   ~30 lines inline.
3. `emit_array_join_expansion(operator, operand, s_ptr, ...)` —
   the large 256-line array-iteration loop with nested sub-op
   match for `arrvals_/`, `arrvals_#`, `arrvals_%`.
4. `emit_string_op_expansion(operator, operand, s_ptr, s_len,
   pat_ptr, pat_len, ...)` — the final operator match handling
   `%`/`%%`, `#`/`##`, `:-`/`-`, `:+`/`+`, `substring`,
   `^`/`^^`/`,`/`,,`, `/`/`//`/etc.

The parent is now a clean dispatcher: glob → array-join →
intern → string-op.

No behavior change. Each helper is a verbatim move of the
original code with the necessary cranelift values passed in.

## Results

| function                       | before | after |
| ------------------------------ | -----: | ----: |
| `handle_param_expansion`       |     58 |    18 |
| `emit_glob_expansion`          |      — |     4 |
| `emit_regex_replace_op`        |      — |     4 |
| `emit_array_join_expansion`    |      — |    15 |
| `emit_string_op_expansion`     |      — |    20 |

Parent CC 58 → **18**, beating the audit's "expected ~20"
target. All four helpers are well under the report threshold
(top-5 is currently CC ≥48). `handle_param_expansion` drops
out of the top-15 entirely.

The glob/glob_regex dedup is the only true byte-saving
extraction; the others move code without saving lines, but
each is now a single-purpose helper.

Verification:
- `cargo build --release`: clean
- `cargo clippy -D warnings`: clean
- `cargo test --lib`: 12/12
- spec: 291/291

## Top hotspot list after this commit

| CC | function | file |
| --: | --- | --- |
| 55 | `lower_one` | `sash/src/transform/statements.rs:27` |
| 54 | `handle_pipeline` | `sash/src/transform/stmt_handlers.rs:306` |
| 49 | `check_stmt` | `sash/src/type_check.rs:243` |
| 48 | `print_arith_expr` | `sash/src/printer.rs:446` |
| 48 | `lower_value` | `sash/src/codegen/values.rs:261` |
| 48 | `to_tok` | `sash/src/parse/logos_tokens.rs:137` |
| 47 | `scan_word` | `sash/src/parse/lexer.rs:1095` |
| 44 | `arith_expr_to_janet` | `sash/src/emit_janet.rs:1246` |

`emit_param_expansion_inline` (Janet backend, CC 41) is an
obvious dual of this refactor — same operator dispatch, just
emitting Janet instead of Cranelift. Some of the new helpers'
shape could inform its refactor.

`lower_one`, `handle_pipeline`, `check_stmt` are the next three
above-40 candidates. None has an existing p3 todo with a
matching name in the SQL list — need to map back to audit
verdicts before picking.
