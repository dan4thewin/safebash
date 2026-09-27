//! Extracted instruction handlers for lower_inst dispatch.
//!
//! Each function handles one SsaInst match arm.
//! Parameters mirror the match arm destructuring (references to SSA fields).

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{InstBuilder, MemFlags, StackSlotData, StackSlotKind, TrapCode};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;
use cranelift_object::ObjectModule;

use super::commands::{lower_echo, lower_exec};
use super::redirects::{
    emit_redirects_guarded, emit_restore, lower_ir_redirect_expr, RedirectFailure,
};
use super::types::{find_array_slot, ExternFuncs, InstCtx, VarSlot};
use super::values::{lower_value, lower_value_as_int};
use crate::ssa::*;

/// Handle SsaInst::Pipeline — fork/exec multiple stages connected by pipes.
///
/// TRUST: Each stage name was resolved at compile time (DD#23).
/// Stages are connected via anonymous pipes (pipe/dup2/close).
/// No shell interpretation — each stage is a direct execve.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_pipeline(
    stages: &[(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)],
    capture_read_fd: bool,
    checked: bool,
    suppressed: &[bool],
    negated: &[bool],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let n = stages.len();
    let pe = resolve_pipeline_externs(builder, module, ctx.externs);

    // Pipe-while-read needs one extra pipe (last stage → parent).
    let num_pipes = if capture_read_fd { n } else { n - 1 };
    let pipe_slots = create_pipe_slots(builder, num_pipes, pe.pipe);

    let pid_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        (n * 8) as u32,
        3,
    ));

    for (i, (name, args, redirects)) in stages.iter().enumerate() {
        lower_pipeline_stage(
            i,
            n,
            name,
            args,
            redirects,
            capture_read_fd,
            &pipe_slots,
            pid_slot,
            &pe,
            builder,
            module,
            ctx,
        );
    }

    parent_close_pipes(builder, capture_read_fd, &pipe_slots, &pe);
    save_capture_fd(builder, capture_read_fd, &pipe_slots, ctx.pipe_read_fd_var);

    // DD#55: materialize each stage's name for the abort diagnostic
    // (`<argv0>: <failing-stage>: command failed (exit N)`).
    let mut stage_names = Vec::with_capacity(n);
    for (name, _, _) in stages {
        stage_names.push(diag_static_seg(name, ctx.literals, module, builder));
    }
    let (final_worst, fail_name_ptr, fail_name_len) =
        wait_for_children(builder, suppressed, negated, pid_slot, &pe, &stage_names);
    update_pipeline_status(builder, final_worst, ctx);

    if checked {
        emit_pipeline_abort_check(
            builder,
            module,
            ctx,
            final_worst,
            fail_name_ptr,
            fail_name_len,
        );
    }
    false
}

/// Pre-resolved Cranelift FuncRefs for the runtime calls a pipeline needs.
struct PipelineExterns {
    fork: cranelift_codegen::ir::FuncRef,
    pipe: cranelift_codegen::ir::FuncRef,
    dup2: cranelift_codegen::ir::FuncRef,
    close: cranelift_codegen::ir::FuncRef,
    exec: cranelift_codegen::ir::FuncRef,
    wait: cranelift_codegen::ir::FuncRef,
    redirect: cranelift_codegen::ir::FuncRef,
}

fn resolve_pipeline_externs(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &super::types::ExternFuncs,
) -> PipelineExterns {
    PipelineExterns {
        fork: module.declare_func_in_func(externs.fork, builder.func),
        pipe: module.declare_func_in_func(externs.pipe, builder.func),
        dup2: module.declare_func_in_func(externs.dup2, builder.func),
        close: module.declare_func_in_func(externs.close, builder.func),
        exec: module.declare_func_in_func(externs.exec, builder.func),
        wait: module.declare_func_in_func(externs.wait, builder.func),
        redirect: module.declare_func_in_func(externs.redirect, builder.func),
    }
}

fn create_pipe_slots(
    builder: &mut FunctionBuilder,
    num_pipes: usize,
    pipe_ref: cranelift_codegen::ir::FuncRef,
) -> Vec<cranelift_codegen::ir::StackSlot> {
    let mut slots = Vec::with_capacity(num_pipes);
    for _ in 0..num_pipes {
        let slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 3));
        let addr = builder.ins().stack_addr(I64, slot, 0);
        builder.ins().call(pipe_ref, &[addr]);
        slots.push(slot);
    }
    slots
}

#[allow(clippy::too_many_arguments)]
fn lower_pipeline_stage(
    i: usize,
    n: usize,
    name: &str,
    args: &[SsaValue],
    redirects: &[crate::ir::Redirect],
    capture_read_fd: bool,
    pipe_slots: &[cranelift_codegen::ir::StackSlot],
    pid_slot: cranelift_codegen::ir::StackSlot,
    pe: &PipelineExterns,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let null = builder.ins().iconst(I64, 0);
    let call = builder.ins().call(pe.fork, &[]);
    let pid = builder.inst_results(call)[0];
    builder.ins().stack_store(pid, pid_slot, (i * 8) as i32);
    let is_child = builder.ins().icmp(IntCC::Equal, pid, null);

    let child_block = builder.create_block();
    let parent_block = builder.create_block();
    builder
        .ins()
        .brif(is_child, child_block, &[], parent_block, &[]);

    // Child block
    builder.switch_to_block(child_block);
    builder.seal_block(child_block);

    wire_child_pipes(builder, i, n, capture_read_fd, pipe_slots, pe);
    emit_stage_redirects(redirects, builder, module, ctx, pe);

    if lower_pipeline_builtin(name, args, builder, module, ctx) {
        // builtin exited inline
    } else if let Some(suffix) = name.strip_prefix("__compound_") {
        // DD#14: compound command as pipeline stage
        let compound_block_id: usize = suffix.parse().unwrap();
        let compound_cl = ctx.cl_blocks[&compound_block_id];
        builder.ins().jump(compound_cl, &[]);
    } else {
        lower_exec_stage(name, args, builder, module, ctx, pe, null);
    }

    // Parent block
    builder.switch_to_block(parent_block);
    builder.seal_block(parent_block);
}

fn wire_child_pipes(
    builder: &mut FunctionBuilder,
    i: usize,
    n: usize,
    capture_read_fd: bool,
    pipe_slots: &[cranelift_codegen::ir::StackSlot],
    pe: &PipelineExterns,
) {
    // Wire stdin from previous pipe
    if i > 0 {
        let prev_read = builder.ins().stack_load(I64, pipe_slots[i - 1], 0);
        let stdin = builder.ins().iconst(I64, 0);
        builder.ins().call(pe.dup2, &[prev_read, stdin]);
    }
    // Wire stdout to next pipe (or capture pipe for last stage)
    if i < n - 1 || capture_read_fd {
        let pipe_idx = if i < pipe_slots.len() {
            i
        } else {
            pipe_slots.len() - 1
        };
        let next_write = builder.ins().stack_load(I64, pipe_slots[pipe_idx], 8);
        let stdout = builder.ins().iconst(I64, 1);
        builder.ins().call(pe.dup2, &[next_write, stdout]);
    }
    // Close all pipe fds in child
    for slot in pipe_slots {
        let r = builder.ins().stack_load(I64, *slot, 0);
        let w = builder.ins().stack_load(I64, *slot, 8);
        builder.ins().call(pe.close, &[r]);
        builder.ins().call(pe.close, &[w]);
    }
}

fn emit_stage_redirects(
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
    pe: &PipelineExterns,
) {
    for redir in redirects {
        emit_one_stage_redirect(redir, builder, module, ctx, pe);
    }
}

/// open(2) mode for a stage redirect: 0 = read, 1 = write/truncate, 2 = append.
/// `Read` falls in the `_` arm deliberately — 0 is its mode.
fn stage_redirect_flags(op: &crate::ir::RedirectOp) -> i64 {
    match op {
        crate::ir::RedirectOp::Write => 1,
        crate::ir::RedirectOp::Append => 2,
        _ => 0,
    }
}

fn emit_one_stage_redirect(
    redir: &crate::ir::Redirect,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
    pe: &PipelineExterns,
) {
    // The implicit fd depends on the DIRECTION: `< f` is fd 0, `> f` is fd 1.
    // This hardcoded 1 pointed an input redirect at the stage's STDOUT, so
    // `cmd < f | consumer` fed the stage nothing — silently, or hanging on a
    // terminal stdin. Shared with the non-pipeline path so they cannot drift.
    let fd = builder.ins().iconst(
        I64,
        redir
            .fd
            .unwrap_or_else(|| super::redirects::default_fd_for_op(&redir.op)) as i64,
    );
    let flags = builder.ins().iconst(I64, stage_redirect_flags(&redir.op));
    // Literal fast-path: use the interned constant directly.
    if let crate::ir::ExprKind::Literal(s) = &redir.target.kind {
        if let Some(&data_id) = ctx.literals.get(s.as_str()) {
            let gv = module.declare_data_in_func(data_id, builder.func);
            let path_ptr = builder.ins().global_value(I64, gv);
            builder.ins().call(pe.redirect, &[fd, path_ptr, flags]);
        }
        return;
    }
    // Lower the target the same way a top-level redirect does
    // (find_highest_version for `$f`, concat for `"$x.txt"`); the old ad-hoc
    // ir_expr_to_ssa_value hardcoded version 0 and dropped `${x:-f}` / `$a[i]`,
    // silently redirecting a pipeline stage to an empty filename.
    // ParameterExpansion / Subscript targets are spilled to a `$__redir_target_N`
    // scalar in the transform, so only the handled scalar kinds reach here.
    let (ptr, len) = lower_ir_redirect_expr(
        &redir.target,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        ctx.externs,
    );
    let redirect_s_ref = module.declare_func_in_func(ctx.externs.redirect_s, builder.func);
    builder.ins().call(redirect_s_ref, &[fd, ptr, len, flags]);
}

/// Returns true if `name` is an inline pipeline-stage builtin.
/// On true, emits the builtin body and the exit/trap epilogue.
fn lower_pipeline_builtin(
    name: &str,
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    if !matches!(name, "echo" | ":") {
        return false;
    }
    if name == "echo" {
        // Pipeline-stage echo runs in a forked child; its write status
        // feeds the child exit, handled by the pipeline path — ignore the
        // returned write-failure flag here.
        let _ = lower_echo(
            args,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            ctx.externs,
            ctx.space_id,
            ctx.nl_id,
            Some(ctx.exit_status_var),
        );
    }
    // echo / `:` in a pipeline stage exit 0 (true/false are now external).
    let exit_code = builder.ins().iconst(I64, 0);
    let exit_ref = module.declare_func_in_func(ctx.externs.exit, builder.func);
    builder.ins().call(exit_ref, &[exit_code]);
    builder.ins().trap(TrapCode::unwrap_user(1));
    true
}

#[allow(clippy::too_many_arguments)]
fn lower_exec_stage(
    name: &str,
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
    pe: &PipelineExterns,
    _null: cranelift_codegen::ir::Value,
) {
    let argv_addr = super::commands::build_exec_argv(
        name,
        args,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        ctx.externs,
        ctx.exit_status_var,
    );
    let envp_gv = module.declare_data_in_func(ctx.externs.envp, builder.func);
    let envp_addr = builder.ins().global_value(I64, envp_gv);
    let envp = builder.ins().load(I64, MemFlags::new(), envp_addr, 0);
    builder.ins().call(pe.exec, &[argv_addr, envp]);
    // exec doesn't return on success; if it does, exit 127
    let code = builder.ins().iconst(I64, 127);
    let exit_ref = module.declare_func_in_func(ctx.externs.exit, builder.func);
    builder.ins().call(exit_ref, &[code]);
    builder.ins().trap(TrapCode::unwrap_user(1));
}

fn parent_close_pipes(
    builder: &mut FunctionBuilder,
    capture_read_fd: bool,
    pipe_slots: &[cranelift_codegen::ir::StackSlot],
    pe: &PipelineExterns,
) {
    for (idx, slot) in pipe_slots.iter().enumerate() {
        let r = builder.ins().stack_load(I64, *slot, 0);
        let w = builder.ins().stack_load(I64, *slot, 8);
        let is_last_read = capture_read_fd && idx == pipe_slots.len() - 1;
        if !is_last_read {
            builder.ins().call(pe.close, &[r]);
        }
        builder.ins().call(pe.close, &[w]);
    }
}

fn save_capture_fd(
    builder: &mut FunctionBuilder,
    capture_read_fd: bool,
    pipe_slots: &[cranelift_codegen::ir::StackSlot],
    pipe_read_fd_var: cranelift_frontend::Variable,
) {
    if capture_read_fd && !pipe_slots.is_empty() {
        let last_slot = pipe_slots[pipe_slots.len() - 1];
        let pipe_r = builder.ins().stack_load(I64, last_slot, 0);
        builder.def_var(pipe_read_fd_var, pipe_r);
    }
}

/// Wait for all N children, returning the pipefail "worst" exit status
/// as a Cranelift Value. DD#55: aggregate = worst (first-nonzero) status
/// over the **non-suppressed** stages. A `-`-suppressed stage is excluded
/// from the aggregate (its status doesn't count), but it still ran.
fn wait_for_children(
    builder: &mut FunctionBuilder,
    suppressed: &[bool],
    negated: &[bool],
    pid_slot: cranelift_codegen::ir::StackSlot,
    pe: &PipelineExterns,
    names: &[(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)],
) -> (
    cranelift_codegen::ir::Value,
    cranelift_codegen::ir::Value,
    cranelift_codegen::ir::Value,
) {
    let worst_var = builder.declare_var(I64);
    let name_ptr_var = builder.declare_var(I64);
    let name_len_var = builder.declare_var(I64);
    let init_zero = builder.ins().iconst(I64, 0);
    builder.def_var(worst_var, init_zero);
    builder.def_var(name_ptr_var, init_zero);
    builder.def_var(name_len_var, init_zero);

    for (i, &(np, nl)) in names.iter().enumerate() {
        let pid = builder.ins().stack_load(I64, pid_slot, (i * 8) as i32);
        let call = builder.ins().call(pe.wait, &[pid]);
        let raw_status = builder.inst_results(call)[0];
        // DD#70: a `!`-negated stage inverts its status (0↔nonzero) before it
        // joins the aggregate, so `a | ! b` reports true exactly when b failed.
        let status = if negated.get(i).copied().unwrap_or(false) {
            let zero = builder.ins().iconst(I64, 0);
            let is_zero = builder.ins().icmp(IntCC::Equal, raw_status, zero);
            let one = builder.ins().iconst(I64, 1);
            builder.ins().select(is_zero, one, zero)
        } else {
            raw_status
        };
        // DD#55: a `-`-suppressed stage still ran, but its status is excluded
        // from the aggregate.
        if suppressed.get(i).copied().unwrap_or(false) {
            continue;
        }
        // Update worst: if worst == 0 && status != 0, worst = status (and
        // record that stage's name for the DD#55 diagnostic).
        let cur_worst = builder.use_var(worst_var);
        let zero = builder.ins().iconst(I64, 0);
        let worst_is_zero = builder.ins().icmp(IntCC::Equal, cur_worst, zero);
        let status_nonzero = builder.ins().icmp(IntCC::NotEqual, status, zero);
        let should_update = builder.ins().band(worst_is_zero, status_nonzero);
        let new_worst = builder.ins().select(should_update, status, cur_worst);
        builder.def_var(worst_var, new_worst);
        let cur_np = builder.use_var(name_ptr_var);
        let cur_nl = builder.use_var(name_len_var);
        let new_np = builder.ins().select(should_update, np, cur_np);
        let new_nl = builder.ins().select(should_update, nl, cur_nl);
        builder.def_var(name_ptr_var, new_np);
        builder.def_var(name_len_var, new_nl);
    }
    (
        builder.use_var(worst_var),
        builder.use_var(name_ptr_var),
        builder.use_var(name_len_var),
    )
}

fn update_pipeline_status(
    builder: &mut FunctionBuilder,
    final_worst: cranelift_codegen::ir::Value,
    ctx: &mut InstCtx,
) {
    builder.def_var(ctx.exit_status_var, final_worst);
    let zero = builder.ins().iconst(I64, 0);
    let ok = builder.ins().icmp(IntCC::Equal, final_worst, zero);
    let tr = builder.ins().uextend(I64, ok);
    builder.def_var(ctx.test_result_var, tr);
}

/// DD#55: implicit exit check for pipelines. On failure, jump to
/// the END block if one exists; otherwise call `exit(worst)`.
fn emit_pipeline_abort_check(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
    final_worst: cranelift_codegen::ir::Value,
    fail_name_ptr: cranelift_codegen::ir::Value,
    fail_name_len: cranelift_codegen::ir::Value,
) {
    let abort_block = builder.create_block();
    let cont_block = builder.create_block();
    let zero = builder.ins().iconst(I64, 0);
    let failed = builder.ins().icmp(IntCC::NotEqual, final_worst, zero);
    builder
        .ins()
        .brif(failed, abort_block, &[], cont_block, &[]);

    builder.switch_to_block(abort_block);
    builder.seal_block(abort_block);
    // DD#55 diagnostic: `<argv0>: <failing-stage>: command failed (exit N)`.
    let emsg_status_ref = module.declare_func_in_func(ctx.externs.emsg_status, builder.func);
    builder.ins().call(
        emsg_status_ref,
        &[fail_name_ptr, fail_name_len, final_worst],
    );
    if let Some(end_cl) = ctx.end_cl_block {
        builder.def_var(ctx.exit_status_var, final_worst);
        builder.ins().jump(end_cl, &[]);
    } else {
        let exit_ref = module.declare_func_in_func(ctx.externs.exit, builder.func);
        builder.ins().call(exit_ref, &[final_worst]);
        builder
            .ins()
            .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
    }

    builder.switch_to_block(cont_block);
    builder.seal_block(cont_block);
}

/// Handle SsaInst::Assign.
pub(super) fn handle_assign(
    target: &VarRef,
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    if is_new_table_sentinel(value) {
        handle_new_table_assign(target, builder, module, ctx);
        return false;
    }
    let base = &target.name;
    if base == "?" {
        handle_exit_status_assign(value, builder, module, ctx);
    } else {
        handle_regular_assign(target, value, builder, module, ctx);
    }
    false
}

/// DD#30: `unset arr` lowers to `arr = "__new_table__"`, which means
/// 'allocate a fresh empty table and put it in this slot'.
fn is_new_table_sentinel(value: &SsaValue) -> bool {
    matches!(value, SsaValue::Literal(s) if s == "__new_table__")
}

fn handle_new_table_assign(
    target: &VarRef,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let sv = ctx.vars[&target.display()];
    let cap = builder.ins().iconst(I64, 0);
    let new_ref = module.declare_func_in_func(ctx.externs.table_new, builder.func);
    let call = builder.ins().call(new_ref, &[cap]);
    let tbl_ptr = builder.inst_results(call)[0];
    builder.def_var(sv.str_ptr(), tbl_ptr);
    let zero_len = builder.ins().iconst(I64, 0);
    builder.def_var(sv.str_len(), zero_len);
}

/// Assignment to `$?` — the value goes into the dedicated
/// exit_status_var (an i64), not into a regular var slot.
///
/// The value is lowered **as an int**, not as a string-then-`atoi`. The
/// distinction is not cosmetic: `lower_value_as_int` is type-directed and
/// routes a DD#77 disposition to `sash_atoi_or`/`sash_atoi_or_abort`, whereas
/// string lowering evaluates `${s:-7}` in *word* context — where the trigger is
/// unset-or-empty (DD#65), not "not a valid integer" — and then leniently
/// `atoi`s the result. For a set-but-non-numeric `s` that silently produced
/// **0**, so `return ${s:?msg}` returned 0 instead of aborting: an explicit
/// guard that did nothing. `emit_exit` (DD#89) and the c backend's `$?` arm
/// (`emit_c.rs::emit_assign`) already lowered as int; this is native catching
/// up, and the two backends now agree.
fn handle_exit_status_assign(
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let int_val = lower_value_as_int(
        value,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        ctx.externs,
        Some(ctx.exit_status_var),
    );
    builder.def_var(ctx.exit_status_var, int_val);
    // DD#74: keep test_result_var in sync with $? so a function's `return N`
    // drives an enclosing if/while/!/&&/|| condition (test_result = 1 iff the
    // status is 0), exactly as a command does (see commands.rs run path).
    let zero = builder.ins().iconst(I64, 0);
    let ok = builder.ins().icmp(IntCC::Equal, int_val, zero);
    let result = builder.ins().uextend(I64, ok);
    builder.def_var(ctx.test_result_var, result);
}

fn handle_regular_assign(
    target: &VarRef,
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let sv = ctx.vars[&target.display()];
    match sv {
        VarSlot::Int(v) => {
            let int_val = lower_value_as_int(
                value,
                builder,
                module,
                ctx.vars,
                ctx.literals,
                ctx.externs,
                Some(ctx.exit_status_var),
            );
            builder.def_var(v, int_val);
        }
        VarSlot::Str { ptr, len } => {
            let (ptr_val, len_val) = lower_value(
                value,
                builder,
                module,
                ctx.vars,
                ctx.literals,
                ctx.externs,
                Some(ctx.exit_status_var),
            );
            builder.def_var(ptr, ptr_val);
            builder.def_var(len, len_val);
        }
    }
}

/// Handle general external command — fork, exec, wait.
/// TRUST: Command name is resolved at compile time (DD#23).
pub(super) fn handle_general_command(
    name: &str,
    args: &[SsaValue],
    redirects: &[crate::ir::Redirect],
    checked: &bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let in_condition = ctx.in_condition;
    let checked = *checked;
    // DD#55: a redirect open(2) failure skips the exec (no misroute) and
    // aborts unless caught; on success the command runs and its own exit
    // status applies.
    emit_redirect_guarded_body(redirects, in_condition, builder, module, ctx, |b, m, c| {
        lower_exec(
            name,
            args,
            b,
            m,
            c.vars,
            c.literals,
            c.externs,
            c.test_result_var,
            c.exit_status_var,
        );
        // DD#55 (A): implicit exit check — abort if the command's status is
        // nonzero, unless it was caught (if/||) or suppressed (-). The
        // external diagnostic is `<argv0>: <name>: command failed (exit N)`.
        let (np, nl) = diag_static_seg(name, c.literals, m, b);
        emit_status_abort_check(
            b,
            m,
            c.externs,
            c.exit_status_var,
            c.end_cl_block,
            checked,
            Some(AbortMsg::Status {
                name_ptr: np,
                name_len: nl,
            }),
        );
        emit_signal_exit_check(b, m, c);
    });
    false
}

/// DD#56: signal EXIT check point after an external command. If a trap'd
/// signal fired, jump to the END block with status 128+signum. Flattened
/// with early returns to keep callers under the complexity gate.
fn emit_signal_exit_check(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    if ctx.program.exiting_signals.is_empty() {
        return;
    }
    let Some(end_cl) = ctx.end_cl_block else {
        return;
    };
    if ctx.program.end_block.is_some_and(|eb| ctx.ssa_id >= eb) {
        return;
    }
    let signum_gv = module.declare_data_in_func(ctx.externs.signum, builder.func);
    let signum_base = builder.ins().global_value(I64, signum_gv);
    let signum_val = builder.ins().load(I64, MemFlags::new(), signum_base, 0);
    let zero_sig = builder.ins().iconst(I64, 0);
    let sig_fired = builder.ins().icmp(IntCC::NotEqual, signum_val, zero_sig);
    let sig_end_block = builder.create_block();
    let sig_cont_block = builder.create_block();
    builder
        .ins()
        .brif(sig_fired, sig_end_block, &[], sig_cont_block, &[]);
    builder.switch_to_block(sig_end_block);
    builder.seal_block(sig_end_block);
    // Set exit status to 128+signum (standard convention)
    let c128 = builder.ins().iconst(I64, 128);
    let exit_code = builder.ins().iadd(signum_val, c128);
    builder.def_var(ctx.exit_status_var, exit_code);
    builder.ins().jump(end_cl, &[]);
    builder.switch_to_block(sig_cont_block);
    builder.seal_block(sig_cont_block);
}

/// DD#55 abort diagnostic. Either an errno-based message rendered by
/// `sash_emsg` as `<argv0>: <seg1>[: <seg2>]: <strerror>`, or a
/// status-based message rendered by `sash_emsg_status` as
/// `<argv0>: <name>: command failed (exit N)` (external/pipeline/cmdsub).
pub(super) enum AbortMsg {
    Errno {
        seg1_ptr: cranelift_codegen::ir::Value,
        seg1_len: cranelift_codegen::ir::Value,
        seg2_ptr: cranelift_codegen::ir::Value,
        seg2_len: cranelift_codegen::ir::Value,
    },
    Status {
        name_ptr: cranelift_codegen::ir::Value,
        name_len: cranelift_codegen::ir::Value,
    },
}

/// Materialize a pre-interned static diagnostic segment ("cd",
/// "echo: write error") as `(ptr, len)` Values. The string must have been
/// interned at setup (see the build-literals pre-intern list).
pub(super) fn diag_static_seg(
    s: &str,
    literals: &std::collections::HashMap<String, cranelift_module::DataId>,
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let id = literals
        .get(s)
        .copied()
        .expect("diagnostic segment must be interned at setup");
    let gv = module.declare_data_in_func(id, builder.func);
    let ptr = builder.ins().global_value(I64, gv);
    let len = builder.ins().iconst(I64, s.len() as i64);
    (ptr, len)
}

/// An empty diagnostic segment (null ptr / 0 len → skipped by sash_emsg).
pub(super) fn diag_empty_seg(
    builder: &mut FunctionBuilder,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let z = builder.ins().iconst(I64, 0);
    (z, z)
}

/// DD#55 redirect open(2) guard. Applies `redirects`; if a file `open(2)`
/// setup-failure is surfaced, skips `body` (no misroute), sets a failure
/// status, and aborts — emitting `<argv0>: <path>: <reason>` — unless
/// `in_condition` (if/||), where the failed status flows to the condition
/// instead. On success runs `body` then restores the saved fds. The
/// `body` emits the actual command (echo write / exec) and its own status
/// and abort handling.
/// DD#55: *report* an uncaught redirect `open(2)` failure —
/// `<argv0>: <path>: <reason>` — and set the failing status.
///
/// **Ordering against `emit_restore` no longer decides where this lands.**
/// It used to: bash performs redirections left to right and writes the
/// setup-failure diagnostic through the ones that already succeeded, so
/// `cmd 2> capt.txt > /bad` puts the message in `capt.txt`. DD#120 rejects
/// that model — the redirect is a channel for *the command's* output, and a
/// command that could not be set up produced none — so `sash_emsg` writes to
/// the invoker's scope fd and a per-command redirect cannot reach it.
///
/// Kept split from [`emit_open_failure_terminate`] so the restore can still
/// sit *between* the two: on the caught (`in_condition`) path control falls
/// through to the merge, and the fds must be back by then.
fn emit_open_failure_msg(
    rf: &RedirectFailure,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let emsg_ref = module.declare_func_in_func(ctx.externs.emsg, builder.func);
    let z = builder.ins().iconst(I64, 0);
    builder
        .ins()
        .call(emsg_ref, &[rf.path_ptr, rf.path_len, z, z]);
    let one = builder.ins().iconst(I64, 1);
    builder.def_var(ctx.exit_status_var, one);
}

/// DD#55: abort after [`emit_open_failure_msg`] reported the failure.
/// Routes the abort through the END block (DD#56) when the program has
/// one, so cleanup still runs; otherwise it exits directly. Shared by the
/// per-command guard and the scope push, which differ only in *when* they
/// decide a failure is uncaught.
///
/// Returns true when it did **not** terminate the block, which happens
/// inside END itself: there the abort would jump to the block already
/// executing (an unbounded loop), and END is cleanup that has to finish.
/// The failure is still reported and still sets a failing status — only
/// the abort is dropped, matching an exec failure in the same position.
#[must_use]
fn emit_open_failure_terminate(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    if ctx.in_end_block {
        return true;
    }
    if let Some(end_cl) = ctx.end_cl_block {
        builder.ins().jump(end_cl, &[]);
    } else {
        let one = builder.ins().iconst(I64, 1);
        let exit_ref = module.declare_func_in_func(ctx.externs.exit, builder.func);
        builder.ins().call(exit_ref, &[one]);
        builder
            .ins()
            .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
    }
    false
}

pub(super) fn emit_redirect_guarded_body(
    redirects: &[crate::ir::Redirect],
    in_condition: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
    body: impl FnOnce(&mut FunctionBuilder, &mut ObjectModule, &mut InstCtx),
) {
    let (saved, failure) = emit_redirects_guarded(
        redirects,
        builder,
        module,
        ctx.externs,
        ctx.literals,
        ctx.vars,
    );
    let Some(rf) = failure else {
        body(builder, module, ctx);
        emit_restore(saved, builder, module, ctx.externs);
        return;
    };

    let fail_block = builder.create_block();
    let ok_block = builder.create_block();
    let merge_block = builder.create_block();
    builder
        .ins()
        .brif(rf.failed, fail_block, &[], ok_block, &[]);

    // Fail path: the command could not set up its redirect (no misroute —
    // the body never runs). Status is failure.
    builder.switch_to_block(fail_block);
    builder.seal_block(fail_block);
    let one = builder.ins().iconst(I64, 1);
    let zero = builder.ins().iconst(I64, 0);
    builder.def_var(ctx.exit_status_var, one);
    builder.def_var(ctx.test_result_var, zero);
    // Caught by if/|| — the failure decides the branch; no diagnostic
    // (55.25/55.50) and no abort. Otherwise report it on the invoker's scope
    // fd (DD#120 — the redirect belongs to the command, which never ran),
    // then put the fds back, then abort — falling through to the merge only
    // when the abort was dropped (inside END), which is why the restore
    // stays between the two calls.
    if !in_condition {
        emit_open_failure_msg(&rf, builder, module, ctx);
    }
    emit_restore(saved.clone(), builder, module, ctx.externs);
    if in_condition || emit_open_failure_terminate(builder, module, ctx) {
        builder.ins().jump(merge_block, &[]);
    }

    // Ok path: open succeeded — run the command body normally.
    builder.switch_to_block(ok_block);
    builder.seal_block(ok_block);
    body(builder, module, ctx);
    emit_restore(saved, builder, module, ctx.externs);
    builder.ins().jump(merge_block, &[]);

    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
}

/// `f > out.txt`: open a redirect *scope* around an inlined function body.
///
/// Saves each affected fd into this site's slots (so the paired
/// `RedirectPop` can restore them from another block), applies the
/// redirects, and leaves the test result set to whether the scope opened.
/// The SSA block ends in a Branch on that result, so a failed `open(2)`
/// jumps straight to the pop and the body never runs on the wrong fds —
/// the cross-block form of the if/else `emit_redirect_guarded_body` builds
/// around a single command.
///
/// `checked` is DD#55: outside condition position a failed open restores
/// the fds, writes `<argv0>: <path>: <reason>` and aborts, so the skip edge
/// is never actually taken there.
pub(super) fn emit_redirect_push(
    site: u32,
    redirects: &[crate::ir::Redirect],
    checked: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let (saved, failure) = emit_redirects_guarded(
        redirects,
        builder,
        module,
        ctx.externs,
        ctx.literals,
        ctx.vars,
    );
    for (slot, s) in ctx.redirect_slots[site as usize].iter().zip(saved.iter()) {
        builder.def_var(*slot, s.saved_copy);
    }
    let one = builder.ins().iconst(I64, 1);
    let Some(rf) = failure else {
        // Nothing here can fail (a dup-only scope, or no redirects at all):
        // the scope is open, so the branch must take the body.
        builder.def_var(ctx.test_result_var, one);
        return;
    };

    let fail_block = builder.create_block();
    let ok_block = builder.create_block();
    let merge_block = builder.create_block();
    builder
        .ins()
        .brif(rf.failed, fail_block, &[], ok_block, &[]);

    builder.switch_to_block(fail_block);
    builder.seal_block(fail_block);
    let zero = builder.ins().iconst(I64, 0);
    builder.def_var(ctx.exit_status_var, one);
    builder.def_var(ctx.test_result_var, zero);
    // Inside END the scope is unchecked (DD#56) but the failure is still
    // reported — silence is for a *caught* failure (55.25), not a cleanup
    // one, and the plain-command path in END reports too.
    let report = checked || ctx.in_end_block;
    let mut terminated = false;
    if report {
        // Report first, while the scope's successful redirects are still in
        // effect — bash writes the diagnostic through them (`f 2> capt > /bad`
        // puts it in `capt`). Only then put the fds back: the pop is on the
        // skip path, which an abort never reaches, so the restore has to
        // happen here — an END block (DD#56) still runs.
        emit_open_failure_msg(&rf, builder, module, ctx);
        emit_restore(saved.clone(), builder, module, ctx.externs);
        terminated = !emit_open_failure_terminate(builder, module, ctx);
    }
    if !terminated {
        builder.ins().jump(merge_block, &[]);
    }

    builder.switch_to_block(ok_block);
    builder.seal_block(ok_block);
    builder.def_var(ctx.test_result_var, one);
    builder.ins().jump(merge_block, &[]);

    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
}

/// Close the scope `RedirectPush { site }` opened: put each saved fd back,
/// in reverse, and drop the copy. Reached both by falling out of the body
/// and by the skip edge a failed open takes.
pub(super) fn emit_redirect_pop(
    site: u32,
    fds: &[i64],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let saved: Vec<super::redirects::SavedFd> = ctx.redirect_slots[site as usize]
        .iter()
        .zip(fds.iter())
        .map(|(slot, fd)| super::redirects::SavedFd {
            original_fd: *fd,
            saved_copy: builder.use_var(*slot),
        })
        .collect();
    emit_restore(saved, builder, module, ctx.externs);
}

/// DD#55 (A)-channel: emit the implicit exit check. When `abort` is true
/// (the command was neither caught by `if`/`||` nor suppressed by `-`),
/// branch on `exit_status_var != 0` to either the EXIT/END block (DD#56,
/// runs traps) or a direct `sash_exit(status)`. When `msg` is provided,
/// a `sash_emsg` diagnostic is written to stderr first (so caught/`-`
/// suppressed failures, which never reach the abort block, stay silent).
/// Shared by external commands and internal syscall-wrapping builtins
/// (cd, echo, …) so they abort uniformly on failure.
pub(super) fn emit_status_abort_check(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    exit_status_var: cranelift_frontend::Variable,
    end_cl_block: Option<cranelift_codegen::ir::Block>,
    abort: bool,
    msg: Option<AbortMsg>,
) {
    if !abort {
        return;
    }
    let status = builder.use_var(exit_status_var);
    let zero = builder.ins().iconst(I64, 0);
    let failed = builder.ins().icmp(IntCC::NotEqual, status, zero);

    let abort_block = builder.create_block();
    let cont_block = builder.create_block();
    builder
        .ins()
        .brif(failed, abort_block, &[], cont_block, &[]);

    builder.switch_to_block(abort_block);
    builder.seal_block(abort_block);
    // DD#55: descriptive diagnostic to stderr, only on the actual abort.
    if let Some(m) = msg {
        match m {
            AbortMsg::Errno {
                seg1_ptr,
                seg1_len,
                seg2_ptr,
                seg2_len,
            } => {
                let emsg_ref = module.declare_func_in_func(externs.emsg, builder.func);
                builder
                    .ins()
                    .call(emsg_ref, &[seg1_ptr, seg1_len, seg2_ptr, seg2_len]);
            }
            AbortMsg::Status { name_ptr, name_len } => {
                let emsg_status_ref =
                    module.declare_func_in_func(externs.emsg_status, builder.func);
                builder
                    .ins()
                    .call(emsg_status_ref, &[name_ptr, name_len, status]);
            }
        }
    }
    if let Some(end_cl) = end_cl_block {
        // DD#56: jump to END block instead of exiting directly
        builder.def_var(exit_status_var, status);
        builder.ins().jump(end_cl, &[]);
    } else {
        let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
        builder.ins().call(exit_ref, &[status]);
        builder
            .ins()
            .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
    }

    builder.switch_to_block(cont_block);
    builder.seal_block(cont_block);
}

/// Handle `cd` builtin. Per DD#53, $PWD/$OLDPWD/$HOME are plain sash
/// variables (not magic); cd only calls chdir(2) and sets $?. With no
/// argument or with `-`, behavior is unspecified — fall back to a
/// no-op-success.
pub(super) fn handle_cd(
    args: &[SsaValue],
    checked: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let arg = args
        .first()
        .expect("SB4066 validated `cd` as taking exactly one argument");
    let (ptr, len) = lower_value(
        arg,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        ctx.externs,
        Some(ctx.exit_status_var),
    );
    let chdir_ref = module.declare_func_in_func(ctx.externs.chdir, builder.func);
    let call = builder.ins().call(chdir_ref, &[ptr, len]);
    let result = builder.inst_results(call)[0];
    // chdir returns 0 on success, -1 on failure → $? = 0 or 1.
    let zero = builder.ins().iconst(I64, 0);
    let failed = builder.ins().icmp(IntCC::SignedLessThan, result, zero);
    let es = builder.ins().uextend(I64, failed);
    builder.def_var(ctx.exit_status_var, es);
    // DD#55/DD#62: cd's verdict must be visible to catching positions
    // (if/while/&&/||) — set test_result = success, not the old "always
    // false" left-over that made `if cd /valid` take the else branch.
    let success = builder
        .ins()
        .icmp(IntCC::SignedGreaterThanOrEqual, result, zero);
    let tr = builder.ins().uextend(I64, success);
    builder.def_var(ctx.test_result_var, tr);
    // DD#55 (A): a failed chdir is a failed syscall — bare cd aborts;
    // caught (if/||) or suppressed (-) cd does not. Diagnostic on abort:
    // `<argv0>: cd: <path>: <reason>`.
    let (s1p, s1l) = diag_static_seg("cd", ctx.literals, module, builder);
    let msg = AbortMsg::Errno {
        seg1_ptr: s1p,
        seg1_len: s1l,
        seg2_ptr: ptr,
        seg2_len: len,
    };
    emit_status_abort_check(
        builder,
        module,
        ctx.externs,
        ctx.exit_status_var,
        ctx.end_cl_block,
        checked,
        Some(msg),
    );
    false
}

/// `close N` (DD#14 fd-manipulation) — `close(2)` the fd. Void; sets `$?` from
/// the result (0 ok / 1 fail) and, per DD#55, a bare failed close aborts
/// (a caught `if/||` or `-`-suppressed close does not). Abort diagnostic:
/// `<argv0>: close: <fd>: <reason>`.
pub(super) fn handle_close(
    args: &[SsaValue],
    checked: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let arg = args
        .first()
        .expect("SB4052 validated `close` as taking exactly one argument");
    let fd_val = lower_value_as_int(
        arg,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        ctx.externs,
        Some(ctx.exit_status_var),
    );
    let close_ref = module.declare_func_in_func(ctx.externs.close, builder.func);
    let call = builder.ins().call(close_ref, &[fd_val]);
    let result = builder.inst_results(call)[0];
    // sash_close returns 0 on success, -1 on failure → $? = 0 or 1.
    let zero = builder.ins().iconst(I64, 0);
    let failed = builder.ins().icmp(IntCC::SignedLessThan, result, zero);
    let es = builder.ins().uextend(I64, failed);
    builder.def_var(ctx.exit_status_var, es);
    let success = builder
        .ins()
        .icmp(IntCC::SignedGreaterThanOrEqual, result, zero);
    let tr = builder.ins().uextend(I64, success);
    builder.def_var(ctx.test_result_var, tr);
    // DD#55: diagnostic segments are "close" + the fd number.
    let (s1p, s1l) = diag_static_seg("close", ctx.literals, module, builder);
    // SB4052 requires a literal fd, so there is always a spelling to quote.
    // The wildcard that stood here handed the diagnostic a null segment.
    let SsaValue::Literal(fd_spelling) = arg else {
        panic!("SB4052 validated `close`'s argument as a literal fd number")
    };
    let (s2p, s2l) = diag_static_seg(fd_spelling, ctx.literals, module, builder);
    let msg = AbortMsg::Errno {
        seg1_ptr: s1p,
        seg1_len: s1l,
        seg2_ptr: s2p,
        seg2_len: s2l,
    };
    emit_status_abort_check(
        builder,
        module,
        ctx.externs,
        ctx.exit_status_var,
        ctx.end_cl_block,
        checked,
        Some(msg),
    );
    false
}

/// Handle SsaInst::MatchInto — DD#63 `match S P ARR` out-array form.
///
/// Lowers the subject/pattern to (ptr, len), resolves the destination
/// array's table pointer (the array must be declared, e.g. `array g=()`),
/// and calls `sash_regex_match_into`, which clears the table on entry and
/// fills it with `[whole, g1..gN]` (bash-parity length). Also sets the
/// boolean test result / `$?` like the 2-arg `match`, so `if match S P g`
/// works. The subject is always clean (a tainted subject is rejected at
/// the read site), so the array is clean.
pub(super) fn handle_match_into(
    subject: &SsaValue,
    pattern: &SsaValue,
    array: &str,
    all: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let (s_ptr, s_len) = lower_value(
        subject,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        ctx.externs,
        Some(ctx.exit_status_var),
    );
    let (p_ptr, p_len) = lower_value(
        pattern,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        ctx.externs,
        Some(ctx.exit_status_var),
    );
    let sv = find_array_slot(ctx.vars, array, "array");
    let table_ptr = builder.use_var(sv.str_ptr());

    let func = if all {
        ctx.externs.regex_matchall_into
    } else {
        ctx.externs.regex_match_into
    };
    let into_ref = module.declare_func_in_func(func, builder.func);
    let call = builder
        .ins()
        .call(into_ref, &[s_ptr, s_len, p_ptr, p_len, table_ptr]);
    let matched = builder.inst_results(call)[0]; // 1 = matched, 0 = not
    builder.def_var(ctx.test_result_var, matched);
    // $? = matched ? 0 : 1
    let one = builder.ins().iconst(I64, 1);
    let es = builder.ins().isub(one, matched);
    builder.def_var(ctx.exit_status_var, es);
    false
}

/// Handle SsaInst::ArgvFill — DD#60 `argv [N] a` out-param form. Calls
/// `sash_argv_snapshot(start)` (builds a fresh indexed table from the
/// program argv slice argv[start..argc]) and stores it into the
/// destination array's slot. Auto-declares the array (the slot was
/// reserved by the setup scan); the array is tainted (DD#61), governed by
/// the validate CST pass.
pub(super) fn handle_argv_fill(
    array: &str,
    start: i64,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let start_val = builder.ins().iconst(I64, start);
    let snap_ref = module.declare_func_in_func(ctx.externs.argv_snapshot, builder.func);
    let call = builder.ins().call(snap_ref, &[start_val]);
    let table_ptr = builder.inst_results(call)[0];
    let sv = find_array_slot(ctx.vars, array, "array");
    builder.def_var(sv.str_ptr(), table_ptr);
    false
}

/// Handle SsaInst::SubscriptAssign.
pub(super) fn handle_subscript_assign(
    array: &str,
    index: &SsaValue,
    value: &SsaValue,
    is_integer_key: &bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let vars = &mut *ctx.vars;
    let literals = ctx.literals;
    let externs = ctx.externs;
    let exit_status_var = ctx.exit_status_var;

    let sv = find_array_slot(vars, array, "array");
    let table_ptr = builder.use_var(sv.str_ptr());

    let (key_ptr, key_len) = if *is_integer_key {
        let idx_int = lower_value_as_int(
            index,
            builder,
            module,
            vars,
            literals,
            externs,
            Some(exit_status_var),
        );
        let idx_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        builder.ins().stack_store(idx_int, idx_slot, 0);
        let kp = builder.ins().stack_addr(I64, idx_slot, 0);
        let kl = builder.ins().iconst(I64, 8);
        (kp, kl)
    } else {
        lower_value(
            index,
            builder,
            module,
            vars,
            literals,
            externs,
            Some(exit_status_var),
        )
    };

    let (val_ptr, val_len) = lower_value(
        value,
        builder,
        module,
        vars,
        literals,
        externs,
        Some(exit_status_var),
    );
    let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
    builder
        .ins()
        .call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
    false
}
