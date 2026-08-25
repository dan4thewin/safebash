# 179 — DD#81: remove the dead func_constants machinery

Second DD#81 stage. With Part A (c09a4a3) rejecting forward `$(f)`, a bare
`SsaValue::FuncCall` can no longer reach codegen: `$(f)` resolves to a function
only when `f` is defined-so-far, and such a call is always lifted to a captured
temp by `lift_int_func_calls` (capture_func_into finds the body). A
not-yet-defined name resolves external. So the `func_constants` path — which
existed only to (badly) handle a name used before its function definition — is
provably dead.

## Confirmed dead first

Instrumented `lower_func_call_value` to `panic!` unconditionally and ran the full
native sweep: **0 regressions** — nothing reaches it.

## Removed

- `entry.rs::fold_function_outputs` + its only helper `try_fold_block` (the
  constant-folder that built the map).
- `values.rs::lower_func_call_value` (the sole consumer).
- The `setup.rs::intern_literals` loop that interned the folded constants.
- The `func_constants: &HashMap<String,String>` parameter threaded through the
  entire value-lowering tree — **139 threading lines across 8 files** (values,
  value_handlers, tests, commands, dispatch, inst_handlers, setup, entry) plus
  the `InstCtx` field.

The `SsaValue::FuncCall` arm in `lower_value` now `panic!`s "should have been
lifted during transform", joining its `IntFuncCall` / `CmdSubstBody` siblings —
a transform invariant, not a user-reachable path.

## Result

Net −216 lines. `cargo build` + `clippy --tests -D warnings` clean. Full
`SPEC_MODES="native c janet"` sweep vs base `c09a4a3`: **0 regressions, 0 new**
(behavior-preserving — the code was dead). Spot-checked: `$(f)` capture, assoc
`map`, `$(join)`, `echo` all compile+run.

## Next (DD#81)

`$(f arg)` arg passing (a defined function called with args in `$( )` still
drops the args and ICEs); Part B uniform arity; the `bug: bare command` →
`command not found` message cleanup.
