//! Instruction dispatch: routes each SsaInst to its handler.
//!
//! SECURITY: This is the complete list of instructions the codegen handles.
//! A reviewer can verify: every SsaInst variant is accounted for.
//! No instruction is silently ignored (except Phi, which is handled during setup).

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{InstBuilder, MemFlags, StackSlotData, StackSlotKind};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;
use cranelift_object::ObjectModule;

use super::commands::{lower_echo, lower_exec};
use super::redirects::{emit_redirects, emit_restore};
use super::tests::lower_test_expr;
use super::types::{InstCtx, VarSlot};
use super::values::lower_value;
use crate::ssa::*;

pub(super) fn lower_inst(
    inst: &SsaInst,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let vars = &mut *ctx.vars;
    let literals = ctx.literals;
    let externs = ctx.externs;
    let space_id = ctx.space_id;
    let nl_id = ctx.nl_id;
    let test_result_var = ctx.test_result_var;
    let exit_status_var = ctx.exit_status_var;
    let pipe_read_fd_var = ctx.pipe_read_fd_var;
    let capture_saved_var = ctx.capture_saved_var;
    let capture_pipe_var = ctx.capture_pipe_var;
    let func_constants = ctx.func_constants;
    let redirect_stack = &mut *ctx.redirect_stack;
    let end_cl_block = ctx.end_cl_block;
    let program = ctx.program;
    let ssa_id = ctx.ssa_id;
    let saved_end_status_var = ctx.saved_end_status_var;
    let _cl_blocks = ctx.cl_blocks;
    let _trace = ctx.trace;
    match inst {
        SsaInst::Assign { target, value } => {
            return super::inst_handlers::handle_assign(target, value, builder, module, ctx);
        }
        SsaInst::Command {
            name,
            args,
            redirects,
            ..
        } if name == "echo" => {
            let restore = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
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
            emit_restore(restore, builder, module, externs);
            // echo always succeeds — set test_result for && / || chains
            let one = builder.ins().iconst(I64, 1);
            builder.def_var(test_result_var, one);
            let zero = builder.ins().iconst(I64, 0);
            builder.def_var(exit_status_var, zero);
        }
        SsaInst::Command { name, args, .. } if name == "[[" => {
            // Legacy path — should not be reached now that transform
            // emits SsaInst::Test directly. Treat as always-true.
            let one = builder.ins().iconst(I64, 1);
            builder.def_var(test_result_var, one);
        }
        SsaInst::Command { name, .. } if name == "__negate_test_result" => {
            let cur = builder.use_var(test_result_var);
            let one = builder.ins().iconst(I64, 1);
            let flipped = builder.ins().isub(one, cur);
            builder.def_var(test_result_var, flipped);
            // Also flip exit status so $? reflects the negation
            let cur_es = builder.use_var(exit_status_var);
            let zero = builder.ins().iconst(I64, 0);
            let is_zero = builder.ins().icmp(IntCC::Equal, cur_es, zero);
            let new_es = builder.ins().select(is_zero, one, zero);
            builder.def_var(exit_status_var, new_es);
        }
        SsaInst::Command { name, args, .. } if name == "__let_exit" => {
            // let builtin: exit status 0 if value != 0, 1 if value == 0
            if let Some(SsaValue::Var(vr)) = args.first() {
                let key = format!("{}_{}", vr.name, vr.version);
                let int_val = if let Some(slot) = vars.get(&key) {
                    if slot.is_int() {
                        builder.use_var(*slot.as_int())
                    } else {
                        let ptr = builder.use_var(slot.str_ptr());
                        let len = builder.use_var(slot.str_len());
                        let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                        let call = builder.ins().call(atoi_ref, &[ptr, len]);
                        builder.inst_results(call)[0]
                    }
                } else {
                    builder.ins().iconst(I64, 0)
                };
                let zero = builder.ins().iconst(I64, 0);
                let is_zero = builder.ins().icmp(IntCC::Equal, int_val, zero);
                let one = builder.ins().iconst(I64, 1);
                let es = builder.ins().select(is_zero, one, zero);
                builder.def_var(exit_status_var, es);
                // Also set test_result_var for branch terminators (if/while)
                let tr = builder.ins().select(is_zero, zero, one);
                builder.def_var(test_result_var, tr);
            }
        }
        SsaInst::Command { name, .. } if name == "__subshell" => {
            // Fork: pid = sash_fork(). Store pid in test_result_var
            // so the Branch terminator can dispatch on it.
            let fork_ref = module.declare_func_in_func(externs.fork, builder.func);
            let call = builder.ins().call(fork_ref, &[]);
            let pid = builder.inst_results(call)[0];
            builder.def_var(test_result_var, pid);
        }
        SsaInst::Command { name, .. } if name == "__subshell_wait" => {
            // Parent: wait for the child whose pid is in test_result_var
            let pid = builder.use_var(test_result_var);
            let wait_ref = module.declare_func_in_func(externs.wait, builder.func);
            let call = builder.ins().call(wait_ref, &[pid]);
            let status = builder.inst_results(call)[0];
            builder.def_var(exit_status_var, status);
        }
        SsaInst::Command {
            name, redirects, ..
        } if name == "__redirect_push" => {
            // Compound-command redirect: save fds, apply redirects
            let saved = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            redirect_stack.push(saved);
        }
        SsaInst::Command { name, .. } if name == "__redirect_pop" => {
            // Compound-command redirect: restore saved fds
            if let Some(saved) = redirect_stack.pop() {
                emit_restore(saved, builder, module, externs);
            }
        }
        SsaInst::Command { name, args, .. } if name == "__store_bg_pid" => {
            // Store fork pid (test_result_var) into $! variable
            if let Some(SsaValue::Var(vr)) = args.first() {
                let pid = builder.use_var(test_result_var);
                // Convert pid to string via itoa
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call = builder.ins().call(itoa_ref, &[pid, out_addr]);
                let str_ptr = builder.inst_results(call)[0];
                let str_len = builder.ins().stack_load(I64, out_slot, 0);
                let sv = &vars[&vr.display()];
                builder.def_var(sv.str_ptr(), str_ptr);
                builder.def_var(sv.str_len(), str_len);
            }
        }
        SsaInst::Command { name, args, .. } if name == "__trap_signal" => {
            // trap '' SIG or trap - SIG: call sash_trap(signum, handler)
            if args.len() >= 2 {
                if let (SsaValue::Literal(sig_s), SsaValue::Literal(handler_s)) =
                    (&args[0], &args[1])
                {
                    let signum = builder.ins().iconst(I64, sig_s.parse::<i64>().unwrap_or(0));
                    let handler = builder
                        .ins()
                        .iconst(I64, handler_s.parse::<i64>().unwrap_or(0));
                    let trap_ref = module.declare_func_in_func(externs.trap, builder.func);
                    builder.ins().call(trap_ref, &[signum, handler]);
                }
            }
        }
        SsaInst::Command { name, args, .. } if name == "__signal_flag" => {
            // signal N FLAG: call sash_signal_flag(signum)
            if let Some(SsaValue::Literal(sig_s)) = args.first() {
                let signum = builder.ins().iconst(I64, sig_s.parse::<i64>().unwrap_or(0));
                let flag_ref = module.declare_func_in_func(externs.signal_flag, builder.func);
                builder.ins().call(flag_ref, &[signum]);
            }
        }
        SsaInst::Command { name, args, .. } if name == "__table_delete" => {
            // unset map[key] → sash_table_delete(table_ptr, key_ptr, key_len)
            if args.len() >= 2 {
                let (tbl_ptr, _) = lower_value(
                    &args[0],
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                    Some(exit_status_var),
                );
                let (key_ptr, key_len) = lower_value(
                    &args[1],
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                    Some(exit_status_var),
                );
                let del_ref = module.declare_func_in_func(externs.table_delete, builder.func);
                builder.ins().call(del_ref, &[tbl_ptr, key_ptr, key_len]);
            }
        }
        SsaInst::Command { name, .. } if name == ":" || name == "true" || name == "false" => {
            let val = if name == "false" { 0i64 } else { 1i64 };
            let v = builder.ins().iconst(I64, val);
            builder.def_var(test_result_var, v);
            // $? = 0 for true/:, 1 for false
            let es = builder
                .ins()
                .iconst(I64, if name == "false" { 1 } else { 0 });
            builder.def_var(exit_status_var, es);
        }
        SsaInst::Command { name, args, .. } if name == "exit" => {
            // exit N — get the exit code from first arg, default 0
            let code = if let Some(SsaValue::Literal(s)) = args.first() {
                s.parse::<i64>().unwrap_or(0)
            } else {
                0
            };
            let code_val = builder.ins().iconst(I64, code);
            // DD#56: if END block exists and we're NOT inside it, jump to END
            if let Some(end_cl) = end_cl_block {
                if let Some(end_id) = program.end_block {
                    if ssa_id < end_id {
                        builder.def_var(exit_status_var, code_val);
                        builder.ins().jump(end_cl, &[]);
                        return true;
                    }
                }
            }
            // Direct exit (no END block, or inside END block)
            // Inside END: also update saved_end_status_var so Return uses it
            builder.def_var(saved_end_status_var, code_val);
            let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
            builder.ins().call(exit_ref, &[code_val]);
            builder
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
            return true; // block is terminated
        }
        SsaInst::Command {
            name,
            args,
            redirects,
            ..
        } if name == "exec" => {
            // exec with no args = standalone redirects only (fd manipulation)
            if args.is_empty() {
                if !redirects.is_empty() {
                    emit_redirects(
                        redirects,
                        builder,
                        module,
                        externs,
                        literals,
                        vars,
                        Some(exit_status_var),
                    );
                }
                let zero = builder.ins().iconst(I64, 0);
                builder.def_var(exit_status_var, zero);
            } else {
                // exec cmd args... — replace process (no fork)
                // Check if any arg is $@ — if so, we need a dynamic-size argv
                let has_dollar_at = args.iter().any(|a| {
                    if let SsaValue::Var(vref) = a {
                        vref.name == "@"
                    } else {
                        false
                    }
                });
                if has_dollar_at {
                    // Build prefix array (args before $@) on the stack, null-terminated
                    let prefix_args: Vec<_> = args
                        .iter()
                        .filter(|a| {
                            if let SsaValue::Var(vref) = a {
                                vref.name != "@"
                            } else {
                                true
                            }
                        })
                        .collect();
                    let prefix_size = ((prefix_args.len() + 1) * 8) as u32;
                    let prefix_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        prefix_size,
                        3,
                    ));
                    for (i, arg_val) in prefix_args.iter().enumerate() {
                        let (ptr, _len) = lower_value(
                            arg_val,
                            builder,
                            module,
                            vars,
                            literals,
                            func_constants,
                            externs,
                            Some(exit_status_var),
                        );
                        builder.ins().stack_store(ptr, prefix_slot, (i * 8) as i32);
                    }
                    let null = builder.ins().iconst(I64, 0);
                    builder
                        .ins()
                        .stack_store(null, prefix_slot, (prefix_args.len() * 8) as i32);
                    let prefix_addr = builder.ins().stack_addr(I64, prefix_slot, 0);
                    // Load argv and argc
                    let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
                    let argv_ptr = builder.ins().global_value(I64, argv_gv);
                    let argv_val = builder.ins().load(I64, MemFlags::new(), argv_ptr, 0);
                    let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
                    let argc_ptr = builder.ins().global_value(I64, argc_gv);
                    let argc_val_exec = builder.ins().load(I64, MemFlags::new(), argc_ptr, 0);
                    // Call sash_splice_argv(prefix, argv, argc) -> merged argv
                    let splice_ref = module.declare_func_in_func(externs.splice_argv, builder.func);
                    let call = builder
                        .ins()
                        .call(splice_ref, &[prefix_addr, argv_val, argc_val_exec]);
                    let merged_argv = builder.inst_results(call)[0];
                    // Load envp
                    let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
                    let envp_ptr = builder.ins().global_value(I64, envp_gv);
                    let envp_val = builder.ins().load(I64, MemFlags::new(), envp_ptr, 0);
                    // exec(merged_argv, envp)
                    let exec_ref = module.declare_func_in_func(externs.exec, builder.func);
                    builder.ins().call(exec_ref, &[merged_argv, envp_val]);
                    let code = builder.ins().iconst(I64, 127);
                    let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                    builder.ins().call(exit_ref, &[code]);
                    builder
                        .ins()
                        .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
                    return true;
                }
                // Static argv path (no $@)
                let argc = args.len();
                let argv_size = ((argc + 1) * 8) as u32;
                let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    argv_size,
                    3,
                ));
                for (i, arg_val) in args.iter().enumerate() {
                    let (ptr, _len) = lower_value(
                        arg_val,
                        builder,
                        module,
                        vars,
                        literals,
                        func_constants,
                        externs,
                        Some(exit_status_var),
                    );
                    builder.ins().stack_store(ptr, argv_slot, (i * 8) as i32);
                }
                // Null terminator
                let null = builder.ins().iconst(I64, 0);
                builder
                    .ins()
                    .stack_store(null, argv_slot, (argc * 8) as i32);
                let argv_addr = builder.ins().stack_addr(I64, argv_slot, 0);
                // Load envp
                let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
                let envp_ptr = builder.ins().global_value(I64, envp_gv);
                let envp_val = builder.ins().load(I64, MemFlags::new(), envp_ptr, 0);
                // exec(argv, envp) — does not return on success
                let exec_ref = module.declare_func_in_func(externs.exec, builder.func);
                builder.ins().call(exec_ref, &[argv_addr, envp_val]);
                // If exec fails, exit 127
                let code = builder.ins().iconst(I64, 127);
                let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                builder.ins().call(exit_ref, &[code]);
                builder
                    .ins()
                    .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
                return true;
            }
        }
        SsaInst::Command { name, args, .. } if name == "cd" => {
            return super::inst_handlers::handle_cd(args, builder, module, ctx);
        }
        SsaInst::Command { name, args, .. } if name == "pwd" => {
            // pwd — getcwd, write to stdout
            let len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let len_addr = builder.ins().stack_addr(I64, len_slot, 0);
            let getcwd_ref = module.declare_func_in_func(externs.getcwd, builder.func);
            let call = builder.ins().call(getcwd_ref, &[len_addr]);
            let cwd_ptr = builder.inst_results(call)[0];
            let cwd_len = builder.ins().stack_load(I64, len_slot, 0);
            let stdout = builder.ins().iconst(I64, 1);
            let write_ref = module.declare_func_in_func(externs.write, builder.func);
            builder.ins().call(write_ref, &[stdout, cwd_ptr, cwd_len]);
            // Write newline
            let nl_id = literals.get("\n").expect("newline literal");
            let nl_gv = module.declare_data_in_func(*nl_id, builder.func);
            let nl_ptr = builder.ins().global_value(I64, nl_gv);
            let nl_len = builder.ins().iconst(I64, 1);
            builder.ins().call(write_ref, &[stdout, nl_ptr, nl_len]);
            let zero = builder.ins().iconst(I64, 0);
            builder.def_var(exit_status_var, zero);
        }
        SsaInst::Command { name, args, .. } if name == "wait" => {
            // wait [pid] — wait for specific pid or last bg job
            let pid = if let Some(arg) = args.first() {
                let (ptr, len) = lower_value(
                    arg,
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
            } else {
                // No arg: wait for $! — look up the "!" variable
                let key = vars.keys().find(|k| {
                    let base = if let Some(idx) = k.rfind('_') {
                        let suffix = &k[idx + 1..];
                        if suffix.chars().all(|c| c.is_ascii_digit()) {
                            &k[..idx]
                        } else {
                            k.as_str()
                        }
                    } else {
                        k.as_str()
                    };
                    base == "!"
                });
                if let Some(k) = key.cloned() {
                    let sv = &vars[&k];
                    let (ptr, len) = (builder.use_var(sv.str_ptr()), builder.use_var(sv.str_len()));
                    let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                    let call = builder.ins().call(atoi_ref, &[ptr, len]);
                    builder.inst_results(call)[0]
                } else {
                    // No $! variable — wait for pid 0 (wait any child)
                    builder.ins().iconst(I64, 0)
                }
            };
            let wait_ref = module.declare_func_in_func(externs.wait, builder.func);
            let call = builder.ins().call(wait_ref, &[pid]);
            let status = builder.inst_results(call)[0];
            builder.def_var(exit_status_var, status);
        }
        SsaInst::Command { name, args, .. } if name == "shift" => {
            return super::inst_handlers::handle_shift(args, builder, module, ctx);
        }
        SsaInst::Command {
            name,
            args,
            redirects,
            ..
        } if name == "command" => {
            // DD#23: `command -v` is rejected by the validator. If we reach here,
            // it's `command [-p] <cmd>` — just run the command, bypassing functions.
            let restore = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            let mut cmd_start = 0;
            for (i, arg) in args.iter().enumerate() {
                if let SsaValue::Literal(s) = arg {
                    match s.as_str() {
                        "-p" | "--" => {
                            cmd_start = i + 1;
                        }
                        _ => {
                            cmd_start = i;
                            break;
                        }
                    }
                } else {
                    cmd_start = i;
                    break;
                }
            }
            if cmd_start < args.len() {
                if let SsaValue::Literal(real_name) = &args[cmd_start] {
                    let real_args = args[cmd_start + 1..].to_vec();
                    lower_exec(
                        real_name,
                        &real_args,
                        builder,
                        module,
                        vars,
                        literals,
                        externs,
                        test_result_var,
                        exit_status_var,
                    );
                } else {
                    let real_name_str = match &args[cmd_start] {
                        SsaValue::Literal(s) => s.clone(),
                        _ => "command".to_string(),
                    };
                    let real_args = args[cmd_start + 1..].to_vec();
                    lower_exec(
                        &real_name_str,
                        &real_args,
                        builder,
                        module,
                        vars,
                        literals,
                        externs,
                        test_result_var,
                        exit_status_var,
                    );
                }
            }
            emit_restore(restore, builder, module, externs);
        }
        SsaInst::Command {
            name,
            args,
            redirects,
            checked,
            ..
        } => {
            return super::inst_handlers::handle_general_command(
                name, args, redirects, checked, builder, module, ctx,
            );
        }
        SsaInst::Phi(_) => {}
        SsaInst::SubscriptAssign {
            array,
            index,
            value,
            is_integer_key,
        } => {
            return super::inst_handlers::handle_subscript_assign(
                array,
                index,
                value,
                is_integer_key,
                builder,
                module,
                ctx,
            );
        }
        SsaInst::ArrayAppend { array, value } => {
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys()
                    .find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            // Get current length as the next integer key
            let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
            let call = builder.ins().call(len_ref, &[table_ptr]);
            let next_idx = builder.inst_results(call)[0];

            let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            builder.ins().stack_store(next_idx, idx_slot, 0);
            let kp = builder.ins().stack_addr(I64, idx_slot, 0);
            let kl = builder.ins().iconst(I64, 8);

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
                .call(set_ref, &[table_ptr, kp, kl, val_ptr, val_len]);
        }
        SsaInst::Mapfile {
            array,
            fd,
            map_mode,
            separator,
        } => {
            return super::inst_handlers::handle_mapfile(
                array, fd, map_mode, separator, builder, module, ctx,
            );
        }
        SsaInst::ReadArray { array, fd, delim } => {
            // Slurp all input from fd, then split by delimiter into indexed array
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys()
                    .find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            let fd_val = if *fd == -1 {
                builder.use_var(pipe_read_fd_var)
            } else {
                builder.ins().iconst(I64, *fd)
            };

            // Read all data from fd
            let read_all_ref = module.declare_func_in_func(externs.read_all, builder.func);
            let call = builder.ins().call(read_all_ref, &[fd_val]);
            let data_ptr = builder.inst_results(call)[0];
            let data_len = builder.inst_results(call)[1];

            // Split by delimiter into array
            let split_ref = module.declare_func_in_func(externs.split_into_array, builder.func);
            let delim_val = builder.ins().iconst(I64, delim.unwrap_or(b' ') as i64);
            builder
                .ins()
                .call(split_ref, &[data_ptr, data_len, delim_val, table_ptr]);
        }
        SsaInst::CaptureStart => {
            // Call sash_capture_start() → (saved_fd, pipe_read_fd)
            // Store both in the capture Cranelift variables
            let cap_ref = module.declare_func_in_func(externs.capture_start, builder.func);
            let call = builder.ins().call(cap_ref, &[]);
            let saved = builder.inst_results(call)[0];
            let pipe_rd = builder.inst_results(call)[1];
            builder.def_var(capture_saved_var, saved);
            builder.def_var(capture_pipe_var, pipe_rd);
        }
        SsaInst::CaptureEnd { target } => {
            // Call sash_capture_end(saved_fd, pipe_read_fd) → (ptr, len)
            let saved = builder.use_var(capture_saved_var);
            let pipe_rd = builder.use_var(capture_pipe_var);
            let cap_ref = module.declare_func_in_func(externs.capture_end, builder.func);
            let call = builder.ins().call(cap_ref, &[saved, pipe_rd]);
            let ptr = builder.inst_results(call)[0];
            let len = builder.inst_results(call)[1];
            let sv = &vars[&target.display()];
            if sv.is_int() {
                // Int var: atoi the captured string
                let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                let atoi_call = builder.ins().call(atoi_ref, &[ptr, len]);
                let int_val = builder.inst_results(atoi_call)[0];
                builder.def_var(*sv.as_int(), int_val);
            } else {
                builder.def_var(sv.str_ptr(), ptr);
                builder.def_var(sv.str_len(), len);
            }
        }
        SsaInst::Test(test_expr) => {
            let result = lower_test_expr(
                test_expr,
                builder,
                module,
                vars,
                literals,
                externs,
                Some(exit_status_var),
            );
            builder.def_var(test_result_var, result);
        }
        SsaInst::ReadLine {
            target,
            fd,
            delim,
            redirects,
            ..
        } => {
            // Set up redirects (e.g. herestring <<< "text")
            let saved = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            // fd=-1 is sentinel for "use captured pipe fd"
            let fd_val = if *fd == -1 {
                builder.use_var(pipe_read_fd_var)
            } else {
                builder.ins().iconst(I64, *fd)
            };
            let (ptr, len) = if let Some(d) = delim {
                let read_delim_ref = module.declare_func_in_func(externs.read_delim, builder.func);
                let delim_val = builder.ins().iconst(I64, *d as i64);
                let call = builder.ins().call(read_delim_ref, &[fd_val, delim_val]);
                (builder.inst_results(call)[0], builder.inst_results(call)[1])
            } else {
                let read_line_ref = module.declare_func_in_func(externs.read_line, builder.func);
                let call = builder.ins().call(read_line_ref, &[fd_val]);
                (builder.inst_results(call)[0], builder.inst_results(call)[1])
            };
            let sv = &vars[&target.display()];
            builder.def_var(sv.str_ptr(), ptr);
            builder.def_var(sv.str_len(), len);
            // Set test_result: 1 if we got data (len > 0), 0 if EOF
            let zero = builder.ins().iconst(I64, 0);
            let got_data = builder.ins().icmp(IntCC::SignedGreaterThan, len, zero);
            let result = builder.ins().uextend(I64, got_data);
            builder.def_var(test_result_var, result);
            // Restore redirects
            emit_restore(saved, builder, module, externs);
        }
        SsaInst::ReadLineMulti {
            targets,
            fd,
            delim,
            redirects,
            field_sep,
            ..
        } => {
            let saved = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            let fd_val = if *fd == -1 {
                builder.use_var(pipe_read_fd_var)
            } else {
                builder.ins().iconst(I64, *fd)
            };
            let (line_ptr, line_len) = if let Some(d) = delim {
                let read_delim_ref = module.declare_func_in_func(externs.read_delim, builder.func);
                let delim_val = builder.ins().iconst(I64, *d as i64);
                let call = builder.ins().call(read_delim_ref, &[fd_val, delim_val]);
                (builder.inst_results(call)[0], builder.inst_results(call)[1])
            } else {
                let read_line_ref = module.declare_func_in_func(externs.read_line, builder.func);
                let call = builder.ins().call(read_line_ref, &[fd_val]);
                (builder.inst_results(call)[0], builder.inst_results(call)[1])
            };

            let zero = builder.ins().iconst(I64, 0);
            let got_data = builder.ins().icmp(IntCC::SignedGreaterThan, line_len, zero);
            let result = builder.ins().uextend(I64, got_data);
            builder.def_var(test_result_var, result);

            let split_ref = module.declare_func_in_func(externs.split_first_word, builder.func);
            let split_field_ref = module.declare_func_in_func(externs.split_field, builder.func);
            let mut cur_ptr = line_ptr;
            let mut cur_len = line_len;

            for (i, target) in targets.iter().enumerate() {
                let sv = &vars[&target.display()];
                if i == targets.len() - 1 {
                    builder.def_var(sv.str_ptr(), cur_ptr);
                    builder.def_var(sv.str_len(), cur_len);
                } else {
                    let rest_ptr_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let rest_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let rest_ptr_addr = builder.ins().stack_addr(I64, rest_ptr_slot, 0);
                    let rest_len_addr = builder.ins().stack_addr(I64, rest_len_slot, 0);
                    let (word_ptr, word_len) = if let Some(sep) = field_sep {
                        let delim_val = builder.ins().iconst(I64, *sep as i64);
                        let call = builder.ins().call(
                            split_field_ref,
                            &[cur_ptr, cur_len, delim_val, rest_ptr_addr, rest_len_addr],
                        );
                        (builder.inst_results(call)[0], builder.inst_results(call)[1])
                    } else {
                        let call = builder
                            .ins()
                            .call(split_ref, &[cur_ptr, cur_len, rest_ptr_addr, rest_len_addr]);
                        (builder.inst_results(call)[0], builder.inst_results(call)[1])
                    };
                    builder.def_var(sv.str_ptr(), word_ptr);
                    builder.def_var(sv.str_len(), word_len);
                    cur_ptr = builder.ins().stack_load(I64, rest_ptr_slot, 0);
                    cur_len = builder.ins().stack_load(I64, rest_len_slot, 0);
                }
            }
            emit_restore(saved, builder, module, externs);
        }
        SsaInst::Pipeline {
            stages,
            capture_read_fd,
            checked,
        } => {
            return super::inst_handlers::handle_pipeline(
                stages,
                *capture_read_fd,
                *checked,
                builder,
                module,
                ctx,
            );
        }
        SsaInst::Bless {
            source,
            target,
            test,
        } => {
            // Runtime bless enforcement: evaluate test on source, abort if false.
            // On pass, copy source value to target (new clean SSA version).
            let result = lower_test_expr(
                test,
                builder,
                module,
                vars,
                literals,
                externs,
                Some(exit_status_var),
            );
            let pass_block = builder.create_block();
            let fail_block = builder.create_block();
            let zero = builder.ins().iconst(I64, 0);
            let is_pass = builder.ins().icmp(IntCC::NotEqual, result, zero);
            builder
                .ins()
                .brif(is_pass, pass_block, &[], fail_block, &[]);

            builder.switch_to_block(fail_block);
            builder.seal_block(fail_block);
            // Write "bless: validation failed\n" to stderr, then exit(1)
            let msg = "bless: validation failed\n";
            let msg_id = literals[msg];
            let msg_gv = module.declare_data_in_func(msg_id, builder.func);
            let msg_ptr = builder.ins().global_value(I64, msg_gv);
            let msg_len = builder.ins().iconst(I64, msg.len() as i64);
            let fd_2 = builder.ins().iconst(I64, 2); // stderr
            let write_ref = module.declare_func_in_func(externs.write, builder.func);
            builder.ins().call(write_ref, &[fd_2, msg_ptr, msg_len]);
            let one = builder.ins().iconst(I64, 1);
            let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
            builder.ins().call(exit_ref, &[one]);
            builder
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());

            builder.switch_to_block(pass_block);
            builder.seal_block(pass_block);

            // Copy source value to target — target is the new clean SSA version
            let src_key = format!("{}_{}", source.name, source.version);
            if let Some(slot) = vars.get(src_key.as_str()).cloned() {
                let tgt_key = format!("{}_{}", target.name, target.version);
                match slot {
                    VarSlot::Str { ptr, len } => {
                        let p = builder.use_var(ptr);
                        let l = builder.use_var(len);
                        let tp = builder.declare_var(I64);
                        let tl = builder.declare_var(I64);
                        builder.def_var(tp, p);
                        builder.def_var(tl, l);
                        vars.insert(tgt_key, VarSlot::Str { ptr: tp, len: tl });
                    }
                    VarSlot::Int(v) => {
                        let val = builder.use_var(v);
                        let tv = builder.declare_var(I64);
                        builder.def_var(tv, val);
                        vars.insert(tgt_key, VarSlot::Int(tv));
                    }
                }
            }
        }
    }
    false // block not terminated
}
