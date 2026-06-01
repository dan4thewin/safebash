# 065 — extracting emit_param_expansion_inline helpers

## Background

`emit_param_expansion_inline` in `sash/src/emit_janet.rs` was a
196-line dispatcher (CC 41) over 14 parameter-expansion operators
emitting Janet S-expressions. Repeated patterns:

- 9 of the 14 arms each pulled the operand via
  `pe.operand.as_ref().map(expr_to_literal).unwrap_or_default()`
  (5 lines apiece).
- Four prefix/suffix strip operators (`#`, `##`, `%`, `%%`) each
  had three sub-cases (`lit*`, `*lit`, literal) → 12 `if`-chains
  with mostly-identical scaffolding.

Audit verdict: **A**. The note recommended per-operator helpers
shared with `handle_param_expansion` (pattern #4); the codegen
side (handle_param_expansion) was already cleared in commit
`394a6ad`, but the Janet emit side shares no Rust code — the
target output is a string of Janet S-exprs, not Cranelift IR. So
the helpers here stand on their own.

## What changed

One shared utility + 9 per-operator helpers:

- `pe_operand_literal(pe) -> String` — the
  `expr_to_literal(o).unwrap_or_default()` boilerplate
- `emit_pe_default` — `:-`
- `emit_pe_suffix_strip_longest` — `%%`
- `emit_pe_suffix_strip_shortest` — `%`
- `emit_pe_prefix_strip_longest` — `##`
- `emit_pe_prefix_strip_shortest` — `#`
- `emit_pe_case_op` — `^^` / `,,` / `^` / `,` (merged; the four
  share input/output shape and were trivial)
- `emit_pe_substring` — `substring`
- `emit_pe_replace(var_ref, operand, global: bool)` — `/` and
  `//` collapsed via `global` bool (the only difference was
  `string/replace` vs `string/replace-all`)

The parent now uses `let-else` to short-circuit the no-operator
case at the top — cleaner than the original `else if/else` chain.

Drive-by: clippy caught four `if x.ends_with('*'); &x[..len-1]`
patterns that the original buried inside the 196-line function;
fixed to `if let Some(lit) = x.strip_suffix('*')`. Pre-existing
issue exposed by the extraction.

## Results

| function                        | before | after |
| ------------------------------- | -----: | ----: |
| `emit_param_expansion_inline`   |     41 |    14 |
| `emit_pe_case_op`               |      — |     6 |
| `emit_pe_default`               |      — |     4 |
| `emit_pe_suffix_strip_longest`  |      — |     3 |
| `emit_pe_suffix_strip_shortest` |      — |     3 |
| `emit_pe_prefix_strip_longest`  |      — |     3 |
| `emit_pe_prefix_strip_shortest` |      — |     3 |
| `emit_pe_substring`             |      — |     3 |
| `emit_pe_replace`               |      — |     3 |
| `pe_operand_literal`            |      — |     2 |

Parent CC **41 → 14** (one match arm per operator + the let-else
+ outer var_ref construction). Max helper CC 6.

Verification: build clean, clippy clean (4 strip-suffix fixes
applied), sash lib 12/12, spec 291/291.

## Top hotspot list after this commit

| CC | function | file |
| --: | --- | --- |
| 49 | `check_stmt` | `sash/src/type_check.rs:243` (C) |
| 48 | `print_arith_expr` | `sash/src/printer.rs:446` (B/intrinsic) |
| 48 | `to_tok` | `sash/src/parse/logos_tokens.rs:137` (B/intrinsic) |
| 44 | `arith_expr_to_janet` | `sash/src/emit_janet.rs:1246` (B/intrinsic) |
| 43 | `print_stmt` | `sash/src/printer.rs:27` (C) |
| 43 | `validate_stmt_ctx` | `sash/src/validate.rs:69` (C, audit) |
| 40 | `value_is_tainted` | `sash/src/ssa_taint.rs` (B/whitelist, audit) |
| 40 | `lower_case` | `sash/src/transform/control_flow.rs` (A — next) |
| 39 | `emit_stmt` | `sash/src/emit_janet.rs` |
| 37 | `from_name` | `sash/src/ssa.rs` |

Only one **A**-verdict remaining at the top: `lower_case` (40).
After that, `emit_stmt`, `from_name`, `lower_for` (35) per audit.
