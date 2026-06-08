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
use super::redirects::{emit_redirects, emit_restore, ir_expr_to_ssa_value};
use super::types::{emit_int_to_str, InstCtx};
use super::values::{emit_strlen, lower_value, lower_value_as_int};
use crate::ssa::*;

/// Handle SsaInst::Pipeline — fork/exec multiple stages connected by pipes.
///
/// TRUST: Each stage name was resolved at compile time (DD#23).
/// Stages are connected via anonymous pipes (pipe/dup2/close).
/// No shell interpretation — each stage is a direct execve.
pub(super) fn handle_pipeline(
    stages: &[(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)],
    capture_read_fd: bool,
    checked: bool,
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

    let final_worst = wait_for_children(builder, n, checked, pid_slot, &pe);
    update_pipeline_status(builder, final_worst, ctx);

    if checked {
        emit_pipeline_abort_check(builder, module, ctx, final_worst);
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
        let fd = builder.ins().iconst(I64, redir.fd.unwrap_or(1) as i64);
        let flags = builder.ins().iconst(
            I64,
            match redir.op {
                crate::ir::RedirectOp::Write => 1,
                crate::ir::RedirectOp::Append => 2,
                _ => 0,
            },
        );
        if let crate::ir::ExprKind::Literal(s) = &redir.target.kind {
            if let Some(&data_id) = ctx.literals.get(s.as_str()) {
                let gv = module.declare_data_in_func(data_id, builder.func);
                let path_ptr = builder.ins().global_value(I64, gv);
                builder.ins().call(pe.redirect, &[fd, path_ptr, flags]);
            }
        } else {
            let target_ssa = ir_expr_to_ssa_value(&redir.target);
            let (ptr, len) = lower_value(
                &target_ssa,
                builder,
                module,
                ctx.vars,
                ctx.literals,
                &std::collections::HashMap::new(),
                ctx.externs,
                Some(ctx.exit_status_var),
            );
            let redirect_s_ref = module.declare_func_in_func(ctx.externs.redirect_s, builder.func);
            builder.ins().call(redirect_s_ref, &[fd, ptr, len, flags]);
        }
    }
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
    if !matches!(name, "echo" | "true" | "false" | ":") {
        return false;
    }
    if name == "echo" {
        lower_echo(
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
    let exit_code = if name == "false" {
        builder.ins().iconst(I64, 1)
    } else {
        builder.ins().iconst(I64, 0)
    };
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
    null: cranelift_codegen::ir::Value,
) {
    let argc = 1 + args.len();
    let argv_size = ((argc + 1) * 8) as u32;
    let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        argv_size,
        3,
    ));
    let cmd_val = SsaValue::Literal(name.to_string());
    let (cmd_ptr, _) = lower_value(
        &cmd_val,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        &std::collections::HashMap::new(),
        ctx.externs,
        Some(ctx.exit_status_var),
    );
    builder.ins().stack_store(cmd_ptr, argv_slot, 0);
    for (j, arg) in args.iter().enumerate() {
        let offset = ((j + 1) * 8) as i32;
        let (ptr, _) = lower_value(
            arg,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            &std::collections::HashMap::new(),
            ctx.externs,
            Some(ctx.exit_status_var),
        );
        builder.ins().stack_store(ptr, argv_slot, offset);
    }
    builder
        .ins()
        .stack_store(null, argv_slot, (argc * 8) as i32);
    let envp_gv = module.declare_data_in_func(ctx.externs.envp, builder.func);
    let envp_addr = builder.ins().global_value(I64, envp_gv);
    let envp = builder.ins().load(I64, MemFlags::new(), envp_addr, 0);
    let argv_addr = builder.ins().stack_addr(I64, argv_slot, 0);
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
/// as a Cranelift Value. DD#55.10: worst = first nonzero; DD#55: stage 0
/// is skipped when the pipeline is suppression-marked (`checked=false`).
fn wait_for_children(
    builder: &mut FunctionBuilder,
    n: usize,
    checked: bool,
    pid_slot: cranelift_codegen::ir::StackSlot,
    pe: &PipelineExterns,
) -> cranelift_codegen::ir::Value {
    let worst_var = builder.declare_var(I64);
    let init_zero = builder.ins().iconst(I64, 0);
    builder.def_var(worst_var, init_zero);

    for i in 0..n {
        let pid = builder.ins().stack_load(I64, pid_slot, (i * 8) as i32);
        let call = builder.ins().call(pe.wait, &[pid]);
        let status = builder.inst_results(call)[0];
        if i == 0 && !checked {
            continue;
        }
        // Update worst: if worst == 0 && status != 0, worst = status
        let cur_worst = builder.use_var(worst_var);
        let zero = builder.ins().iconst(I64, 0);
        let worst_is_zero = builder.ins().icmp(IntCC::Equal, cur_worst, zero);
        let status_nonzero = builder.ins().icmp(IntCC::NotEqual, status, zero);
        let should_update = builder.ins().band(worst_is_zero, status_nonzero);
        let new_worst = builder.ins().select(should_update, status, cur_worst);
        builder.def_var(worst_var, new_worst);
    }
    builder.use_var(worst_var)
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
    let vars = &mut *ctx.vars;
    let literals = ctx.literals;
    let externs = ctx.externs;
    let _space_id = ctx.space_id;
    let _nl_id = ctx.nl_id;
    let _test_result_var = ctx.test_result_var;
    let exit_status_var = ctx.exit_status_var;
    let _pipe_read_fd_var = ctx.pipe_read_fd_var;
    let _capture_saved_var = ctx.capture_saved_var;
    let _capture_pipe_var = ctx.capture_pipe_var;
    let func_constants = ctx.func_constants;
    let _redirect_stack = &mut *ctx.redirect_stack;
    let _end_cl_block = ctx.end_cl_block;
    let _program = ctx.program;
    let _ssa_id = ctx.ssa_id;
    let _saved_end_status_var = ctx.saved_end_status_var;
    let _cl_blocks = ctx.cl_blocks;
    let _trace = ctx.trace;

    // DD#30: unset on collection → new empty table
    if let SsaValue::Literal(s) = value {
        if s == "__new_table__" {
            let sv = vars[&target.display()];
            let cap = builder.ins().iconst(I64, 0);
            let new_ref = module.declare_func_in_func(externs.table_new, builder.func);
            let call = builder.ins().call(new_ref, &[cap]);
            let tbl_ptr = builder.inst_results(call)[0];
            builder.def_var(sv.str_ptr(), tbl_ptr);
            let zero_len = builder.ins().iconst(I64, 0);
            builder.def_var(sv.str_len(), zero_len);
            return false;
        }
    }
    // Special: assignment to $? updates the exit_status_var directly
    let base = {
        let d = target.display();
        if let Some(idx) = d.rfind('_') {
            let suffix = &d[idx + 1..];
            if suffix.chars().all(|c| c.is_ascii_digit()) {
                d[..idx].to_string()
            } else {
                d.to_string()
            }
        } else {
            d.to_string()
        }
    };
    if base == "?" {
        // Parse the literal value as an integer for exit_status_var
        let int_val = match value {
            SsaValue::Literal(s) => {
                let n = s.parse::<i64>().unwrap_or(0);
                builder.ins().iconst(I64, n)
            }
            _ => {
                let (ptr, len) = lower_value(
                    value,
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                    Some(exit_status_var),
                );
                let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                let call = builder.ins().call(atoi_ref, &[ptr, len]);
                builder.inst_results(call)[0]
            }
        };
        builder.def_var(exit_status_var, int_val);
    } else {
        let sv = vars[&target.display()];
        if sv.is_int() {
            // Int var: evaluate value as i64 and store directly
            let int_val = lower_value_as_int(
                value,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            builder.def_var(*sv.as_int(), int_val);
        } else {
            let (ptr_val, len_val) = lower_value(
                value,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                Some(exit_status_var),
            );
            builder.def_var(sv.str_ptr(), ptr_val);
            builder.def_var(sv.str_len(), len_val);
        }
    }
    false
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
    let vars = &mut *ctx.vars;
    let literals = ctx.literals;
    let externs = ctx.externs;
    let _space_id = ctx.space_id;
    let _nl_id = ctx.nl_id;
    let test_result_var = ctx.test_result_var;
    let exit_status_var = ctx.exit_status_var;
    let _pipe_read_fd_var = ctx.pipe_read_fd_var;
    let _capture_saved_var = ctx.capture_saved_var;
    let _capture_pipe_var = ctx.capture_pipe_var;
    let _func_constants = ctx.func_constants;
    let _redirect_stack = &mut *ctx.redirect_stack;
    let end_cl_block = ctx.end_cl_block;
    let program = ctx.program;
    let ssa_id = ctx.ssa_id;
    let _saved_end_status_var = ctx.saved_end_status_var;
    let _cl_blocks = ctx.cl_blocks;
    let _trace = ctx.trace;

    let restore = emit_redirects(
        redirects,
        builder,
        module,
        externs,
        literals,
        vars,
        Some(exit_status_var),
    );
    lower_exec(
        name,
        args,
        builder,
        module,
        vars,
        literals,
        externs,
        test_result_var,
        exit_status_var,
    );
    emit_restore(restore, builder, module, externs);

    // DD#55: implicit exit check — abort if nonzero
    if *checked {
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

    // DD#56: signal EXIT check point after external command
    if !program.exiting_signals.is_empty() {
        if let Some(end_cl) = end_cl_block {
            if program.end_block.is_none_or(|eb| ssa_id < eb) {
                let signum_gv = module.declare_data_in_func(externs.signum, builder.func);
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
                builder.def_var(exit_status_var, exit_code);
                builder.ins().jump(end_cl, &[]);
                builder.switch_to_block(sig_cont_block);
                builder.seal_block(sig_cont_block);
            }
        }
    }
    false
}

/// Handle `cd` builtin. Per DD#53, $PWD/$OLDPWD/$HOME are plain sash
/// variables (not magic); cd only calls chdir(2) and sets $?. With no
/// argument or with `-`, behavior is unspecified — fall back to a
/// no-op-success.
pub(super) fn handle_cd(
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let Some(arg) = args.first() else {
        // `cd` with no arg — no $HOME magic under DD#53; succeed silently.
        let zero = builder.ins().iconst(I64, 0);
        builder.def_var(ctx.exit_status_var, zero);
        return false;
    };
    let (ptr, len) = lower_value(
        arg,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        ctx.func_constants,
        ctx.externs,
        Some(ctx.exit_status_var),
    );
    let chdir_ref = module.declare_func_in_func(ctx.externs.chdir, builder.func);
    let call = builder.ins().call(chdir_ref, &[ptr, len]);
    let result = builder.inst_results(call)[0];
    // chdir returns 0 on success, -1 on failure → $? = 0 or 1
    let zero = builder.ins().iconst(I64, 0);
    let failed = builder.ins().icmp(IntCC::SignedLessThan, result, zero);
    let es = builder.ins().uextend(I64, failed);
    builder.def_var(ctx.exit_status_var, es);
    false
}

/// Handle `shift` builtin.
pub(super) fn handle_shift(
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let vars = &mut *ctx.vars;
    let literals = ctx.literals;
    let externs = ctx.externs;
    let _space_id = ctx.space_id;
    let _nl_id = ctx.nl_id;
    let _test_result_var = ctx.test_result_var;
    let _exit_status_var = ctx.exit_status_var;
    let _pipe_read_fd_var = ctx.pipe_read_fd_var;
    let _capture_saved_var = ctx.capture_saved_var;
    let _capture_pipe_var = ctx.capture_pipe_var;
    let _func_constants = ctx.func_constants;
    let _redirect_stack = &mut *ctx.redirect_stack;
    let _end_cl_block = ctx.end_cl_block;
    let _program = ctx.program;
    let _ssa_id = ctx.ssa_id;
    let _saved_end_status_var = ctx.saved_end_status_var;
    let _cl_blocks = ctx.cl_blocks;
    let _trace = ctx.trace;

    // shift N: sash_argc -= N; sash_argv += N*8
    let shift_n = if let Some(SsaValue::Literal(s)) = args.first() {
        s.parse::<i64>().unwrap_or(1)
    } else {
        1
    };
    let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
    let argc_addr = builder.ins().global_value(I64, argc_gv);
    let old_argc = builder.ins().load(I64, MemFlags::trusted(), argc_addr, 0);
    let n_val = builder.ins().iconst(I64, shift_n);
    let new_argc = builder.ins().isub(old_argc, n_val);
    builder
        .ins()
        .store(MemFlags::trusted(), new_argc, argc_addr, 0);

    let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
    let argv_addr = builder.ins().global_value(I64, argv_gv);
    let old_argv = builder.ins().load(I64, MemFlags::trusted(), argv_addr, 0);
    let offset = builder.ins().iconst(I64, shift_n * 8);
    let new_argv = builder.ins().iadd(old_argv, offset);
    builder
        .ins()
        .store(MemFlags::trusted(), new_argv, argv_addr, 0);

    // Reload all positional params from updated argv/argc
    let fresh_argv = builder.ins().load(I64, MemFlags::trusted(), argv_addr, 0);
    let fresh_argc = builder.ins().load(I64, MemFlags::trusted(), argc_addr, 0);
    for (key, sv) in vars.iter() {
        // key is like "1_0", "2_0", "#_0" — extract base name
        let base = key.split('_').next().unwrap_or("");
        if let Ok(idx) = base.parse::<i64>() {
            // Guard: only reload if idx < fresh_argc (arg still exists)
            let idx_val = builder.ins().iconst(I64, idx);
            let in_bounds = builder
                .ins()
                .icmp(IntCC::SignedLessThan, idx_val, fresh_argc);

            let reload_block = builder.create_block();
            let skip_block = builder.create_block();
            let merge_block = builder.create_block();

            builder
                .ins()
                .brif(in_bounds, reload_block, &[], skip_block, &[]);

            builder.switch_to_block(reload_block);
            builder.seal_block(reload_block);
            let offset = builder.ins().imul_imm(idx_val, 8);
            let arg_ptr_addr = builder.ins().iadd(fresh_argv, offset);
            let arg_ptr = builder
                .ins()
                .load(I64, MemFlags::trusted(), arg_ptr_addr, 0);
            let len = emit_strlen(builder, module, externs.strlen, arg_ptr);
            builder.def_var(sv.str_ptr(), arg_ptr);
            builder.def_var(sv.str_len(), len);
            builder.ins().jump(merge_block, &[]);

            builder.switch_to_block(skip_block);
            builder.seal_block(skip_block);
            let null_ptr = builder.ins().iconst(I64, 0);
            let zero_len = builder.ins().iconst(I64, 0);
            builder.def_var(sv.str_ptr(), null_ptr);
            builder.def_var(sv.str_len(), zero_len);
            builder.ins().jump(merge_block, &[]);

            builder.switch_to_block(merge_block);
            builder.seal_block(merge_block);
        } else if base == "#" {
            let one = builder.ins().iconst(I64, 1);
            let count = builder.ins().isub(fresh_argc, one);
            let (str_ptr, str_len) = emit_int_to_str(builder, module, externs, count);
            builder.def_var(sv.str_ptr(), str_ptr);
            builder.def_var(sv.str_len(), str_len);
        } else if base == "@" {
            let space_id = literals.get(" ").expect("space not interned");
            let space_gv = module.declare_data_in_func(*space_id, builder.func);
            let space_ptr = builder.ins().global_value(I64, space_gv);
            let space_len = builder.ins().iconst(I64, 1);
            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let join_ref = module.declare_func_in_func(externs.join_args, builder.func);
            let call = builder.ins().call(
                join_ref,
                &[fresh_argv, fresh_argc, space_ptr, space_len, out_addr],
            );
            let str_ptr = builder.inst_results(call)[0];
            let str_len = builder.ins().stack_load(I64, out_slot, 0);
            builder.def_var(sv.str_ptr(), str_ptr);
            builder.def_var(sv.str_len(), str_len);
        }
    }
    false
}

/// Handle SsaInst::Mapfile.
pub(super) fn handle_mapfile(
    array: &str,
    fd: &i64,
    map_mode: &bool,
    separator: &char,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let vars = &mut *ctx.vars;
    let _literals = ctx.literals;
    let externs = ctx.externs;
    let _space_id = ctx.space_id;
    let _nl_id = ctx.nl_id;
    let _test_result_var = ctx.test_result_var;
    let _exit_status_var = ctx.exit_status_var;
    let pipe_read_fd_var = ctx.pipe_read_fd_var;
    let _capture_saved_var = ctx.capture_saved_var;
    let _capture_pipe_var = ctx.capture_pipe_var;
    let _func_constants = ctx.func_constants;
    let _redirect_stack = &mut *ctx.redirect_stack;
    let _end_cl_block = ctx.end_cl_block;
    let _program = ctx.program;
    let _ssa_id = ctx.ssa_id;
    let _saved_end_status_var = ctx.saved_end_status_var;
    let _cl_blocks = ctx.cl_blocks;
    let _trace = ctx.trace;

    // Read all lines from fd into an array (or map in map_mode)
    let sv = vars
        .keys()
        .filter(|k| k.starts_with(&format!("{}_", array)))
        .max_by_key(|k| {
            k.rsplit('_')
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0)
        })
        .and_then(|k| vars.get(k))
        .unwrap_or_else(|| panic!("ICE: undefined array: {}", array));
    let table_ptr = builder.use_var(sv.str_ptr());

    let read_line_ref = module.declare_func_in_func(externs.read_line, builder.func);
    let fd_val = if *fd == -1 {
        builder.use_var(pipe_read_fd_var)
    } else {
        builder.ins().iconst(I64, *fd)
    };

    let loop_hdr = builder.create_block();
    let loop_body = builder.create_block();
    let loop_exit = builder.create_block();
    builder.ins().jump(loop_hdr, &[]);

    builder.switch_to_block(loop_hdr);
    let call = builder.ins().call(read_line_ref, &[fd_val]);
    let line_ptr = builder.inst_results(call)[0];
    let line_len = builder.inst_results(call)[1];
    let zero = builder.ins().iconst(I64, 0);
    let got_data = builder.ins().icmp(IntCC::NotEqual, line_ptr, zero);
    builder.ins().brif(got_data, loop_body, &[], loop_exit, &[]);

    builder.switch_to_block(loop_body);
    builder.seal_block(loop_body);

    if *map_mode {
        // Split line on separator, insert key/value into map
        let delim_val = builder.ins().iconst(I64, *separator as i64);
        let rest_ptr_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let rest_len_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let rest_ptr_addr = builder.ins().stack_addr(I64, rest_ptr_slot, 0);
        let rest_len_addr = builder.ins().stack_addr(I64, rest_len_slot, 0);
        let split_ref = module.declare_func_in_func(externs.split_field, builder.func);
        let split_call = builder.ins().call(
            split_ref,
            &[line_ptr, line_len, delim_val, rest_ptr_addr, rest_len_addr],
        );
        let key_ptr = builder.inst_results(split_call)[0];
        let key_len = builder.inst_results(split_call)[1];
        let val_ptr = builder.ins().stack_load(I64, rest_ptr_slot, 0);
        let val_len = builder.ins().stack_load(I64, rest_len_slot, 0);

        // Skip empty keys (blank lines)
        let key_empty = builder.ins().icmp(IntCC::Equal, key_len, zero);
        let skip_block = builder.create_block();
        let insert_block = builder.create_block();
        builder
            .ins()
            .brif(key_empty, skip_block, &[], insert_block, &[]);

        builder.switch_to_block(insert_block);
        builder.seal_block(insert_block);
        // Insert key/value into table
        let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
        builder
            .ins()
            .call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
        builder.ins().jump(loop_hdr, &[]);

        builder.switch_to_block(skip_block);
        builder.seal_block(skip_block);
        builder.ins().jump(loop_hdr, &[]);
    } else {
        // Key = current table length (next integer index)
        let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
        let len_call = builder.ins().call(len_ref, &[table_ptr]);
        let next_idx = builder.inst_results(len_call)[0];
        let idx_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        builder.ins().stack_store(next_idx, idx_slot, 0);
        let kp = builder.ins().stack_addr(I64, idx_slot, 0);
        let kl = builder.ins().iconst(I64, 8);
        let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
        builder
            .ins()
            .call(set_ref, &[table_ptr, kp, kl, line_ptr, line_len]);
        builder.ins().jump(loop_hdr, &[]);
    }
    builder.seal_block(loop_hdr);

    builder.switch_to_block(loop_exit);
    builder.seal_block(loop_exit);
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
    let _space_id = ctx.space_id;
    let _nl_id = ctx.nl_id;
    let _test_result_var = ctx.test_result_var;
    let exit_status_var = ctx.exit_status_var;
    let _pipe_read_fd_var = ctx.pipe_read_fd_var;
    let _capture_saved_var = ctx.capture_saved_var;
    let _capture_pipe_var = ctx.capture_pipe_var;
    let func_constants = ctx.func_constants;
    let _redirect_stack = &mut *ctx.redirect_stack;
    let _end_cl_block = ctx.end_cl_block;
    let _program = ctx.program;
    let _ssa_id = ctx.ssa_id;
    let _saved_end_status_var = ctx.saved_end_status_var;
    let _cl_blocks = ctx.cl_blocks;
    let _trace = ctx.trace;

    let array_key = format!("{}_0", array);
    let sv = vars.get(&array_key).unwrap_or_else(|| {
        vars.keys()
            .find(|k| k.starts_with(&format!("{}_", array)))
            .and_then(|k| vars.get(k))
            .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
    });
    let table_ptr = builder.use_var(sv.str_ptr());

    let (key_ptr, key_len) = if *is_integer_key {
        let idx_int = lower_value_as_int(
            index,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
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
            func_constants,
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
        func_constants,
        externs,
        Some(exit_status_var),
    );
    let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
    builder
        .ins()
        .call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
    false
}
