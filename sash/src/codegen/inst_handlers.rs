//! Extracted instruction handlers for the largest lower_inst match arms.
//!
//! Each handler takes (specific fields, builder, module, ctx) and returns bool
//! (true if the block was terminated, false otherwise).

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{InstBuilder, MemFlags, StackSlotData, StackSlotKind, TrapCode};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::commands::{lower_echo, lower_exec};
use super::redirects::{emit_redirects, emit_restore, ir_expr_to_ssa_value, SavedFd};
use super::tests::lower_test_expr;
use super::types::{ExternFuncs, InstCtx, VarSlot};
use super::values::{
    emit_strlen, lower_expr_value, lower_expr_value_as_int, lower_value, lower_value_as_int,
};
use crate::ir::TestExpr;
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
    let vars = &mut *ctx.vars;
    let literals = ctx.literals;
    let externs = ctx.externs;
    let test_result_var = ctx.test_result_var;
    let exit_status_var = ctx.exit_status_var;
    let pipe_read_fd_var = ctx.pipe_read_fd_var;
    let func_constants = ctx.func_constants;
    let end_cl_block = ctx.end_cl_block;
    let program = ctx.program;
    let cl_blocks = ctx.cl_blocks;
    let space_id = ctx.space_id;
    let nl_id = ctx.nl_id;
    let _trace = ctx.trace;
    for (_n, _, _r) in stages.iter() {}
    // N-stage pipeline: fork N children, wire pipes between them
    // For each adjacent pair, create a pipe.
    // Child i: stdin from pipe[i-1] read end, stdout to pipe[i] write end
    // Parent: close all pipe ends, wait for all children
    let n = stages.len();
    let fork_ref = module.declare_func_in_func(externs.fork, builder.func);
    let pipe_ref = module.declare_func_in_func(externs.pipe, builder.func);
    let dup2_ref = module.declare_func_in_func(externs.dup2, builder.func);
    let close_ref = module.declare_func_in_func(externs.close, builder.func);
    let exec_ref = module.declare_func_in_func(externs.exec, builder.func);
    let wait_ref = module.declare_func_in_func(externs.wait, builder.func);
    let redirect_ref = module.declare_func_in_func(externs.redirect, builder.func);
    let null = builder.ins().iconst(I64, 0);

    // Create n-1 pipes, store fd pairs on stack
    let mut pipe_slots = vec![];
    // For pipe-while-read, need one extra pipe (last stage → parent)
    let num_pipes = if capture_read_fd { n } else { n - 1 };
    for _ in 0..num_pipes {
        let slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 3));
        let addr = builder.ins().stack_addr(I64, slot, 0);
        builder.ins().call(pipe_ref, &[addr]);
        pipe_slots.push(slot);
    }

    // Stack slots for child pids
    let pid_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        (n * 8) as u32,
        3,
    ));

    // Fork each stage
    for (i, (name, args, redirects)) in stages.iter().enumerate() {
        let call = builder.ins().call(fork_ref, &[]);
        let pid = builder.inst_results(call)[0];
        // Store pid for later wait
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

        // Wire stdin from previous pipe
        if i > 0 {
            let prev_read = builder.ins().stack_load(I64, pipe_slots[i - 1], 0);
            let stdin = builder.ins().iconst(I64, 0);
            builder.ins().call(dup2_ref, &[prev_read, stdin]);
        }
        // Wire stdout to next pipe
        // Wire stdout to next pipe (or capture pipe for last stage)
        if i < n - 1 || capture_read_fd {
            let pipe_idx = if i < pipe_slots.len() {
                i
            } else {
                pipe_slots.len() - 1
            };
            let next_write = builder.ins().stack_load(I64, pipe_slots[pipe_idx], 8);
            let stdout = builder.ins().iconst(I64, 1);
            builder.ins().call(dup2_ref, &[next_write, stdout]);
        }
        // Close all pipe fds in child
        for slot in &pipe_slots {
            let r = builder.ins().stack_load(I64, *slot, 0);
            let w = builder.ins().stack_load(I64, *slot, 8);
            builder.ins().call(close_ref, &[r]);
            builder.ins().call(close_ref, &[w]);
        }
        // Apply redirects for this stage
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
                if let Some(&data_id) = literals.get(s.as_str()) {
                    let gv = module.declare_data_in_func(data_id, builder.func);
                    let path_ptr = builder.ins().global_value(I64, gv);
                    builder.ins().call(redirect_ref, &[fd, path_ptr, flags]);
                }
            } else {
                let target_ssa = ir_expr_to_ssa_value(&redir.target);
                let (ptr, len) = lower_value(
                    &target_ssa,
                    builder,
                    module,
                    vars,
                    literals,
                    &std::collections::HashMap::new(),
                    externs,
                    Some(exit_status_var),
                );
                let redirect_s_ref = module.declare_func_in_func(externs.redirect_s, builder.func);
                builder.ins().call(redirect_s_ref, &[fd, ptr, len, flags]);
            }
        }
        // Run stage: builtin inline or exec external
        let is_pipeline_builtin = matches!(name.as_str(), "echo" | "true" | "false" | ":");
        if is_pipeline_builtin {
            // Run builtin inline in the forked child, then _exit(0)
            match name.as_str() {
                "echo" => {
                    lower_echo(
                        args,
                        builder,
                        module,
                        vars,
                        literals,
                        externs,
                        space_id,
                        nl_id,
                        Some(exit_status_var),
                    );
                }
                "true" | ":" => {}
                "false" => {}
                _ => {}
            }
            let exit_code = if name == "false" {
                builder.ins().iconst(I64, 1)
            } else {
                builder.ins().iconst(I64, 0)
            };
            let exit_ref2 = module.declare_func_in_func(externs.exit, builder.func);
            builder.ins().call(exit_ref2, &[exit_code]);
            builder.ins().trap(TrapCode::unwrap_user(1));
        } else if let Some(suffix) = name.strip_prefix("__compound_") {
            // DD#14: compound command as pipeline stage
            // Jump to the pre-lowered SSA blocks, which end with Return
            let compound_block_id: usize = suffix.parse().unwrap();
            let compound_cl = cl_blocks[&compound_block_id];
            builder.ins().jump(compound_cl, &[]);
        } else {
            // Build argv and exec
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
                vars,
                literals,
                &std::collections::HashMap::new(),
                externs,
                Some(exit_status_var),
            );
            builder.ins().stack_store(cmd_ptr, argv_slot, 0);
            for (j, arg) in args.iter().enumerate() {
                let offset = ((j + 1) * 8) as i32;
                let (ptr, _) = lower_value(
                    arg,
                    builder,
                    module,
                    vars,
                    literals,
                    &std::collections::HashMap::new(),
                    externs,
                    Some(exit_status_var),
                );
                builder.ins().stack_store(ptr, argv_slot, offset);
            }
            builder
                .ins()
                .stack_store(null, argv_slot, (argc * 8) as i32);
            let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
            let envp_addr = builder.ins().global_value(I64, envp_gv);
            let envp = builder.ins().load(I64, MemFlags::new(), envp_addr, 0);
            let argv_addr = builder.ins().stack_addr(I64, argv_slot, 0);
            builder.ins().call(exec_ref, &[argv_addr, envp]);
            // exec doesn't return on success; if it does, exit 127
            let code = builder.ins().iconst(I64, 127);
            let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
            builder.ins().call(exit_ref, &[code]);
            builder.ins().trap(TrapCode::unwrap_user(1));
        } // end else (non-builtin exec path)

        // Parent block
        builder.switch_to_block(parent_block);
        builder.seal_block(parent_block);
    }

    // Parent: close all pipe fds (except last read end if capturing for while-read)
    for (idx, slot) in pipe_slots.iter().enumerate() {
        let r = builder.ins().stack_load(I64, *slot, 0);
        let w = builder.ins().stack_load(I64, *slot, 8);
        let is_last_read = capture_read_fd && idx == pipe_slots.len() - 1;
        if !is_last_read {
            builder.ins().call(close_ref, &[r]);
        }
        builder.ins().call(close_ref, &[w]);
    }
    // If capturing pipe fd, store the last read end in pipe_read_fd_var
    if capture_read_fd && !pipe_slots.is_empty() {
        let last_slot = pipe_slots[pipe_slots.len() - 1];
        let pipe_r = builder.ins().stack_load(I64, last_slot, 0);
        builder.def_var(pipe_read_fd_var, pipe_r);
    }
    // Wait for all children, capture last child's exit status
    // DD#55.10: pipefail — track worst (first nonzero) exit status
    let worst_var = builder.declare_var(I64);
    let init_zero = builder.ins().iconst(I64, 0);
    builder.def_var(worst_var, init_zero);

    for i in 0..n {
        let pid = builder.ins().stack_load(I64, pid_slot, (i * 8) as i32);
        let call = builder.ins().call(wait_ref, &[pid]);
        let status = builder.inst_results(call)[0];
        // Update worst: if worst==0 && status!=0, worst=status
        let cur_worst = builder.use_var(worst_var);
        let zero = builder.ins().iconst(I64, 0);
        let worst_is_zero = builder.ins().icmp(IntCC::Equal, cur_worst, zero);
        let status_nonzero = builder.ins().icmp(IntCC::NotEqual, status, zero);
        let should_update = builder.ins().band(worst_is_zero, status_nonzero);
        let new_worst = builder.ins().select(should_update, status, cur_worst);
        builder.def_var(worst_var, new_worst);
    }
    // exit_status = worst (pipefail semantics)
    let final_worst = builder.use_var(worst_var);
    builder.def_var(exit_status_var, final_worst);
    // test_result: 1 if exit==0, 0 otherwise
    let zero2 = builder.ins().iconst(I64, 0);
    let ok = builder.ins().icmp(IntCC::Equal, final_worst, zero2);
    let tr = builder.ins().uextend(I64, ok);
    builder.def_var(test_result_var, tr);

    // DD#55: implicit exit check for pipelines
    if checked {
        let abort_block = builder.create_block();
        let cont_block = builder.create_block();
        let failed = builder.ins().icmp(IntCC::NotEqual, final_worst, zero2);
        builder
            .ins()
            .brif(failed, abort_block, &[], cont_block, &[]);

        builder.switch_to_block(abort_block);
        builder.seal_block(abort_block);
        if let Some(end_cl) = end_cl_block {
            builder.def_var(exit_status_var, final_worst);
            builder.ins().jump(end_cl, &[]);
        } else {
            let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
            builder.ins().call(exit_ref, &[final_worst]);
            builder
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
        }

        builder.switch_to_block(cont_block);
        builder.seal_block(cont_block);
    }
    false
}
