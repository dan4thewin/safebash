# 067 — extracting lower_for shape dispatchers

## Background

`lower_for` in `sash/src/transform/control_flow.rs` was an
A-verdict refactor at CC 35, ~170 lines. The audit (line 193)
named the shape:

> Shape dispatcher: dynamic vs static, $@, array iter,
> $(cmd)→pipe-while-read. Extract `try_lower_dollar_at`,
> `try_lower_array_iter`, `try_lower_cmdsubst_pipe`.

Two of those (`lower_for_dollar_at`, `lower_for_array`) were
already extracted into `transform::pipelines`. The original
`lower_for` was the inline dispatcher that ran the precondition
check (`is_dynamic && f.words.len() == 1`), matched the four
shapes, and emitted the static-unroll fallback.

The `$(cmd)` shape was still inline as ~55 lines of pipeline
construction + while-read emission.

## What changed

Reworked `lower_for` into a dispatcher chain of three `try_*`
helpers plus the static fallback:

```rust
pub(super) fn lower_for(f, ssa, state, block) -> BlockId {
    if let Some(b) = try_dispatch_dollar_at(f, ssa, state, block)  { return b; }
    if let Some(b) = try_dispatch_array(f, ssa, state, block)      { return b; }
    if let Some(b) = try_dispatch_cmdsubst(f, ssa, state, block)   { return b; }
    lower_for_static(f, ssa, state, block)
}
```

New helpers:

- `is_dynamic_single_word(f) -> bool` — shared precondition.
- `try_dispatch_dollar_at(...) -> Option<BlockId>` — detect `$@`
  in either `SimpleExpansion("@")` or `DoubleQuoted([@])` form;
  defer to existing `lower_for_dollar_at`.
- `try_dispatch_array(...) -> Option<BlockId>` — detect array
  iteration; defer to existing `lower_for_array`.
- `detect_array_iter(kind) -> Option<(String, bool)>` —
  recognizes `${arr[@]}`, `${!arr[@]}`, `$(keys arr)` (DD#41),
  and the same patterns wrapped in `"…"`. Recurses through
  `DoubleQuoted` for free thanks to single-arm extraction.
- `try_dispatch_cmdsubst(...) -> Option<BlockId>` — detect
  `$(cmd)` and lower per DD#28/DD#40 to a pipe + while-read.
- `build_pipeline_stages_from_cmdsubst(stmts, state) -> Vec<...>`
  — extract each Command inside the substitution into a stage
  tuple, resolving the command name to an absolute path via
  `resolve_pipeline_command`.
- `emit_cmdsubst_pipe_while_read(f, stages, ssa, state, block)
  -> BlockId` — emit the `Pipeline` SsaInst with
  `capture_read_fd: true`, then build header/body/exit blocks
  around a `ReadLine { fd: -1, … }` over the captured pipe.
- `lower_for_static(...)` — the static word-list unroll path.

## Results

| function                              | before | after |
| ------------------------------------- | -----: | ----: |
| `lower_for`                           |     35 |     4 |
| `detect_array_iter`                   |      — |    10 |
| `build_pipeline_stages_from_cmdsubst` |      — |     8 |
| `try_dispatch_dollar_at`              |      — |     6 |
| `lower_for_static`                    |      — |     4 |
| `is_dynamic_single_word`              |      — |     3 |
| `try_dispatch_array`                  |      — |     3 |
| `try_dispatch_cmdsubst`               |      — |     2 |
| `emit_cmdsubst_pipe_while_read`       |      — |     1 |

Parent CC **35 → 4** (just the if-let chain). Max helper CC 10.

Verification: cargo build clean, clippy clean, sash lib 12/12,
spec 291/291.

## Top hotspot list after this commit

| CC | function | file | verdict |
| --: | --- | --- | --- |
| 49 | `check_stmt` | type_check.rs | C |
| 48 | `print_arith_expr` | printer.rs | B/intrinsic |
| 48 | `to_tok` | parse/logos_tokens.rs | B/intrinsic |
| 44 | `arith_expr_to_janet` | emit_janet.rs | B/intrinsic |
| 43 | `print_stmt` | printer.rs | C |
| 43 | `validate_stmt_ctx` | validate.rs | C |
| 40 | `value_is_tainted` | ssa_taint.rs | B/whitelist |
| 39 | `emit_stmt` | emit_janet.rs | C |
| 37 | `from_name` | ssa.rs | intrinsic |
| 36 | `verify_ssa_taint` | ssa_taint.rs | (live, unaudited) |
| 36 | `allocate_variables` | codegen/setup.rs | A |
| 35 | `classify_word_token` | parse/lexer.rs | intrinsic |
| 34 | `handle_pipeline` | codegen/inst_handlers.rs | A |
| 34 | `expr_to_value` | transform/expressions.rs | A |

The CC 40+ tier is fully cleared of A-verdicts. Next A-verdict
targets: `allocate_variables` (36), `handle_pipeline` (34),
`expr_to_value` (34), `lower_test_unary` (32, audit says 45 but
extraction work has already chipped at it).
