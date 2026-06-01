# 068 — extracting allocate_variables passes

## Background

`allocate_variables` in `sash/src/codegen/setup.rs` was a CC 36,
A-verdict target. The audit (line 99/151, pattern #6) named the
fix specifically:

> Two-pass loop with `if let SsaInst::X` instead of `match inst`
> (pattern #6). Convert to match + extract `declare_base_var`
> and `map_versioned`.

The function was a 165-line monolith doing:

1. Build cl_blocks for reachable BlockIds.
2. ABI setup (entry block, end-block lookup).
3. **First pass**: walk every inst across every block, scattering
   `if let SsaInst::A {…} = inst { … }` checks to declare base
   `VarSlot`s. Four such if-let chains plus a plain `if let
   SsaInst::Command { name, .. }` filter for `__store_bg_pid`.
4. **Second pass**: same shape, walking every versioned target's
   `display()` name into the `vars` map.
5. Declare six ambient Cranelift Variables (`test_result_var`,
   `exit_status_var`, `saved_end_status_var`, `pipe_read_fd_var`,
   `capture_saved_var`, `capture_pipe_var`).
6. Pack everything into `FuncState`.

## What changed

Refactored into orchestrator + eight helpers + one private
struct, applying pattern #6 (convert `if let` chains to `match
inst`):

- `build_cl_blocks(builder, program, reachable) ->
  HashMap<BlockId, cl::Block>` — phase 1.
- `make_str_slot(builder) -> VarSlot` — shared `Str { ptr, len }`
  constructor that was duplicated five times inline.
- `slot_for_var_type(builder, &VarType) -> VarSlot` — picks
  `Int` vs `Str` slot per the target's declared type. Was
  duplicated twice inline (Assign-arm and phi-arm).
- `collect_base_vars(builder, program) -> HashMap<…>` — phase 3
  outer loop. Delegates per-inst work to:
- `declare_inst_bases(builder, inst, base_vars)` — single `match
  inst` over the five base-defining variants plus the
  `__store_bg_pid` guard arm.
- `map_versioned_names(program, &base_vars) -> HashMap<…>` —
  phase 4 outer loop. Delegates per-inst work to:
- `map_inst_versioned(inst, base_vars, vars)` — single `match
  inst` over the three versioned-target variants plus the
  `__store_bg_pid` guard arm.
- `struct AmbientVars { test_result_var, exit_status_var,
  saved_end_status_var, pipe_read_fd_var, capture_saved_var,
  capture_pipe_var }` — named bundle returned by:
- `declare_ambient_vars(builder) -> AmbientVars` — declares all
  six and inits them to zero, via a local `decl` closure.

Parent `allocate_variables` is now 33 lines of straight-line
orchestration ending in a single `FuncState { … }` literal.

## Pattern win

Pattern #6 (`if let SsaInst::X = inst` → `match inst`) is more
than cosmetic. The old code had four independent `if let` chains
per pass, ordered by no rule. The new `match` lays out exactly
which variants take part:

```rust
match inst {
    SsaInst::ReadLineMulti { targets, .. } => …,
    SsaInst::Mapfile { array, .. } | SsaInst::ReadArray { array, .. } => …,
    SsaInst::Command { name, args, .. } if name == "__store_bg_pid" => …,
    SsaInst::Assign { target, .. }
    | SsaInst::ReadLine { target, .. }
    | SsaInst::CaptureEnd { target } => …,
    _ => {}
}
```

Adding a new `SsaInst` variant that introduces a base variable
now requires editing two named functions, both with the same
shape. Before, you had to remember to add another `if let` to
two distant blocks of code in the same monolith.

## Results

| function               | before | after |
| ---------------------- | -----: | ----: |
| `allocate_variables`   |     36 |     2 |
| `declare_inst_bases`   |      — |    12 |
| `map_inst_versioned`   |      — |    10 |
| `map_versioned_names`  |      — |     7 |
| `collect_base_vars`    |      — |     5 |
| `build_cl_blocks`      |      — |     3 |
| `slot_for_var_type`    |      — |     2 |
| `declare_ambient_vars` |      — |     2 |
| `make_str_slot`        |      — |     1 |

Parent CC **36 → 2**. Max helper CC 12 (the 5-arm match — one
CC per variant; intrinsic).

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
| 36 | `verify_ssa_taint` | ssa_taint.rs | (unaudited) |
| 35 | `classify_word_token` | parse/lexer.rs | intrinsic |
| 34 | `handle_pipeline` | codegen/inst_handlers.rs | A |
| 34 | `expr_to_value` | transform/expressions.rs | A |
| 32 | `lower_test_unary` | codegen/tests.rs | A |

Remaining top A-verdicts: `handle_pipeline` (codegen, 34),
`expr_to_value` (34), `lower_test_unary` (32).
