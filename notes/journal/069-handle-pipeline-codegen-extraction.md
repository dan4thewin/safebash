# 069 — extracting handle_pipeline (codegen)

## Background

`handle_pipeline` in `sash/src/codegen/inst_handlers.rs` was a
~300-line A-verdict function at CC 34. The audit (line 145)
named the extractions:

> Massive procedural fork/pipe/dup/wait codegen. Extract
> `create_pipes`, `wire_child_io`, `lower_pipeline_builtin`,
> `lower_compound_stage`, `lower_exec_stage`, `wait_for_children`.

This is the *codegen* handle_pipeline (Cranelift fork/exec
emission), not the transform-side `handle_pipeline` that was
done earlier (journal 060). The two share a name but live in
different layers.

The original was a single monolith doing seven phases:

1. Destructure `ctx` (12 lines of vestigial bindings, several
   with `_` prefixes).
2. Resolve seven `declare_func_in_func` calls.
3. Create N-1 (or N) pipes + pid stack slot.
4. Fork loop, per stage: pipe wiring, redirects, dispatch to
   one of three stage kinds (builtin / compound / exec), parent
   switch.
5. Parent: close pipes, optionally save capture fd.
6. Wait loop with DD#55.10 pipefail worst-tracking.
7. Optional abort check (DD#55).

## What changed

Reworked into a 50-line orchestrator + 12 helpers + one private
struct:

- `struct PipelineExterns` — seven pre-resolved Cranelift
  `FuncRef`s.
- `resolve_pipeline_externs(builder, module, externs) ->
  PipelineExterns`.
- `create_pipe_slots(builder, num_pipes, pipe_ref) ->
  Vec<StackSlot>`.
- `lower_pipeline_stage(i, n, name, args, redirects,
  capture_read_fd, pipe_slots, pid_slot, pe, builder, module,
  ctx)` — the entire per-stage body (fork, branch, wire,
  redirect, dispatch, parent switch).
- `wire_child_pipes(builder, i, n, capture_read_fd, pipe_slots,
  pe)` — stdin/stdout dup2 + close-all.
- `emit_stage_redirects(redirects, builder, module, ctx, pe)`.
- `lower_pipeline_builtin(name, args, builder, module, ctx) ->
  bool` — returns `true` after emitting the builtin body and
  the exit/trap epilogue (echo/true/false/:).
- `lower_exec_stage(name, args, builder, module, ctx, pe, null)`
  — argv build + `execve` call + exit-127 fallback.
- `parent_close_pipes(builder, capture_read_fd, pipe_slots, pe)`.
- `save_capture_fd(builder, capture_read_fd, pipe_slots,
  pipe_read_fd_var)`.
- `wait_for_children(builder, n, checked, pid_slot, pe) ->
  Value` — DD#55.10 pipefail "worst" computation.
- `update_pipeline_status(builder, final_worst, ctx)` —
  `exit_status_var` and `test_result_var` updates.
- `emit_pipeline_abort_check(builder, module, ctx, final_worst)`
  — DD#55 implicit-exit-on-failure path.

The compound-stage case stays inline in `lower_pipeline_stage`
(four lines: parse suffix, look up CL block, `jump`) — too
small for its own helper without becoming silly.

The vestigial `for (_n, _, _r) in stages.iter() {}` and the
nine `let _ = ctx.field` bindings at the top of the original
function are gone — they were leftover from an earlier
refactor where ctx was eagerly destructured.

## Pre-resolved externs

Original code mixed two styles: most `FuncRef`s were resolved
once up-front, but `redirect_s` and `exit` were resolved inline
inside the fork loop (per child, per iteration!). The `exit_ref`
was even resolved twice in two different arms of the same
iteration. The new `PipelineExterns` struct captures the seven
that are used multiple times; `redirect_s` and `exit` stay
inline because their use sites are conditional. No functional
change — `declare_func_in_func` is idempotent — but the
intent is clearer.

## Results

| function                    | before | after |
| --------------------------- | -----: | ----: |
| `handle_pipeline`           |     34 |     4 |
| `emit_stage_redirects`      |      — |     7 |
| `wire_child_pipes`          |      — |     6 |
| `lower_pipeline_builtin`    |      — |     4 |
| `parent_close_pipes`        |      — |     4 |
| `wait_for_children`         |      — |     4 |
| `lower_pipeline_stage`      |      — |     3 |
| `save_capture_fd`           |      — |     3 |
| `create_pipe_slots`         |      — |     2 |
| `lower_exec_stage`          |      — |     2 |
| `emit_pipeline_abort_check` |      — |     2 |
| `resolve_pipeline_externs`  |      — |     1 |
| `update_pipeline_status`    |      — |     1 |

Parent CC **34 → 4**. Max helper CC 7.

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
| 34 | `expr_to_value` | transform/expressions.rs | A |
| 32 | `lower_test_unary` | codegen/tests.rs | A |
| 30 | `arith_to_ssa` | transform/expr_handlers.rs | A |

Remaining top A-verdicts: `expr_to_value` (34),
`lower_test_unary` (32), `arith_to_ssa` (30).
