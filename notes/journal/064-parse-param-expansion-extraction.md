# 064 — extracting parse_parameter_expansion helpers

## Background

`parse_parameter_expansion` in `sash/src/parse/expr.rs` was a
180-line function (CC 44) that dispatched seven distinct
syntactic cases:

1. `${#var}` / `${#var[@]}` — strlen / arrlen
2. `${!var}` / `${!var[@]}` — indirection / arrkeys
3. `${var[idx]}` — subscript (literal idx or `@`/`*` with
   optional chained operator)
4. `${var:s/regex/replacement/flags}` — DD#39 sed
5. `${var:offset}` / `${var:offset:length}` — substring
6. `${var@X}` — parameter transform (rejected SB4015)
7. `${var:-...}`, `${var:+...}`, `${var##pat}`, `${var/pat/repl}`,
   etc. — standard operators (with the slash family compiling
   globs to regex)

Audit verdict: **A**. Each case had its own inline block of
6-30 lines.

## What changed

Seven case helpers, each returning `Option<Expr>` so the parent
can chain them with `if let Some(e) = … { return e; }`. Plus one
utility for the leading-identifier scan:

- `try_strlen_or_arrlen`
- `try_indirection_or_arrkeys`
- `scan_param_name` — `(&str, usize)` of (name slice, end index)
- `try_subscript`
- `try_sed_op`
- `try_substring_op`
- `try_at_transform`
- `parse_standard_operator`

Parent is now a 36-line linear chain: try each case in order,
fall through to `Expr::param(name, None, None, span)`.

The slash-operator regex packing (`pattern\0replacement`,
`/#` / `/%` anchoring) stays inside `parse_standard_operator`
because no other case shares it.

## Results

| function                     | before | after |
| ---------------------------- | -----: | ----: |
| `parse_parameter_expansion`  |     44 |     9 |
| `try_substring_op`           |      — |     9 |
| `parse_standard_operator`    |      — |     8 |
| `try_subscript`              |      — |     7 |
| `try_at_transform`           |      — |     7 |
| `try_strlen_or_arrlen`       |      — |     6 |
| `try_sed_op`                 |      — |     5 |
| `try_indirection_or_arrkeys` |      — |     4 |
| `scan_param_name`            |      — |     4 |

Parent CC **44 → 9** (seven dispatch arms + empty-name guard +
fallback). Max helper CC 9.

Verification: build clean, clippy clean, sash lib 12/12, spec
291/291.

## Top hotspot list after this commit

| CC | function | file |
| --: | --- | --- |
| 49 | `check_stmt` | `sash/src/type_check.rs:243` (C-verdict) |
| 48 | `print_arith_expr` | `sash/src/printer.rs:446` (B/intrinsic) |
| 48 | `to_tok` | `sash/src/parse/logos_tokens.rs:137` (B/intrinsic) |
| 44 | `arith_expr_to_janet` | `sash/src/emit_janet.rs:1246` (B/intrinsic) |
| 43 | `print_stmt` | `sash/src/printer.rs:27` (B candidate) |
| 43 | `validate_stmt_ctx` | `sash/src/validate.rs:69` |
| 41 | `emit_param_expansion_inline` | `sash/src/emit_janet.rs` |
| 40 | `value_is_tainted` | `sash/src/ssa_taint.rs` |
| 40 | `lower_case` | `sash/src/transform/control_flow.rs` |
| 35 | `classify_word_token` | `sash/src/parse/lexer.rs` (intrinsic) |

Codebase max real-target CC: 43. The remaining A-verdict targets
are `validate_stmt_ctx`, `lower_case`, `value_is_tainted`,
`emit_param_expansion_inline`.

`print_stmt` may also be a B/intrinsic (printer dispatcher
mirroring StmtKind variants); will check before queueing.
