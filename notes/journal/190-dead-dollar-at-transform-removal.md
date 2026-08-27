# 190 — remove dead $@ machinery (transform + codegen)

Continuation of the coverage grind (journal 189). After DD#81 universally
rejected `$@`/`$*` at validation (they never reach transform or codegen), two
whole machineries went dead. Removed in two batches:

## codegen side (`ed95432`, prior)

`emit_exec_with_dollar_at` / `has_dollar_at` / `is_dollar_at` /
`splice_in_program_argv` in `codegen/dispatch.rs` — the exec path that spliced
the program argv in place of a `$@` splat. Never reachable post-drop.

## transform side (this commit)

`expand_dollar_at_in_body` + `_in_if` + `_in_stmt` + `expand_dollar_at_words`
+ `is_dollar_at_word` in `transform/cmd_handlers.rs` (~82 lines). This was
DD#60's inline-site rewrite: inside a function `$@`/`$*` expanded to the
caller's `$1..$N`. Now that `$@`/`$*` are rejected before transform,
`is_dollar_at_word` never matched, so `expand_dollar_at_words` was a pure
pass-through and the whole recursion a no-op.

Three call sites removed (all no-ops):
- `cmd_handlers.rs` `inline_function_call` — dropped the clone+expand; the
  body param is now lowered directly (no needless `to_vec`).
- `statements.rs` `lower_function_stmt` — template body lowered as-is.
- `stmt_handlers.rs` `build_function_pipeline_stage` — dropped `mut` + call +
  the now-unused import.

## Verification

`cargo clippy --release -p sash` clean (no unused imports/mut). Full 3-mode
sweep: **0 passing→failing** in native/c/janet; passing counts identical
(native 1177, c 1056, janet 819). Pure behaviour-preserving removal, as
expected for dead code.

## Why this is high-yield for coverage

Recent feature *removals* ($@, earlier `case`→SB4029, pruned var-modifiers)
leave whole functions dead but still line-counted. Deleting them raises the
covered fraction without writing a single spec, and — more importantly —
keeps the tree honest: no compiled-in code paths that no input can reach.
Continuing to hunt these across transform/codegen/emit_c.

## Follow-on: arith-operator coverage (coverage.sh cov.29-48)

Fresh release re-measure after the removal: **88.55% lines** (2544 missed),
up from 87.69%. Triaging the map, the largest contiguous missing run in
`transform/expr_handlers.rs` (465-542) was five arith lowering fns —
`unaryop_to_ssa`/`try_fold_unop`/`incdec_to_ssa`/`compound_assign_to_ssa`/
`comma_to_ssa` — all *reachable* but exercised by no feature spec with a
variable operand. Same for `validate_arith_comma_in_condition`'s accept path.

Added cov.29-48 (native+c): unary `-`/`~`/`!` on a runtime var and on a
constant (fold), postfix `(( x++ ))`/`(( x-- ))`, all ten compound-assign ops
(`+= -= *= /= %= <<= >>= &= |= ^=`), the `$(( a, b ))` comma value, and the
`(( a += 1, a ))` comma-in-condition accept path. +20 native, +20 c, 0
regressions. These also incidentally cover the matching operator paths in
`parse/arith.rs` and `emit_c.rs`.
