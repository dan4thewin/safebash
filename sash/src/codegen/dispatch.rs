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
use super::types::{emit_int_to_str, find_array_slot, ExternFuncs, InstCtx, VarSlot};
use super::values::lower_value;
use crate::ssa::*;

impl SsaInst {
    /// Emit Cranelift IR for this instruction.
    ///
    /// SECURITY: Every SsaInst variant is handled. None are silently dropped.
    pub(super) fn emit(
        &self,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let literals = ctx.literals;
        let externs = ctx.externs;
        let func_constants = ctx.func_constants;
        let test_result_var = ctx.test_result_var;
        let exit_status_var = ctx.exit_status_var;
        let capture_saved_var = ctx.capture_saved_var;
        let capture_pipe_var = ctx.capture_pipe_var;
        match self {
            SsaInst::Assign { target, value } => {
                return super::inst_handlers::handle_assign(target, value, builder, module, ctx);
            }
            SsaInst::Command {
                name,
                kind,
                args,
                redirects,
                checked,
            } => {
                return self
                    .emit_command_kind(name, kind, args, redirects, checked, builder, module, ctx);
            }
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
                return self.emit_array_append(array, value, builder, module, ctx);
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
                return self.emit_read_array(array, fd, delim, builder, module, ctx);
            }
            SsaInst::MatchInto {
                subject,
                pattern,
                array,
            } => {
                return super::inst_handlers::handle_match_into(
                    subject, pattern, array, builder, module, ctx,
                );
            }
            SsaInst::ArgvFill { array, start } => {
                return super::inst_handlers::handle_argv_fill(array, *start, builder, module, ctx);
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
            SsaInst::CaptureEnd { target, .. } => {
                return self.emit_capture_end(target, builder, module, ctx);
            }
            SsaInst::Test(test_expr) => {
                let result = lower_test_expr(
                    test_expr,
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
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
                return self.emit_readline(target, fd, delim, redirects, builder, module, ctx);
            }
            SsaInst::ReadLineMulti {
                targets,
                fd,
                delim,
                redirects,
                field_sep,
                ..
            } => {
                return self.emit_readline_multi(
                    targets, fd, delim, redirects, field_sep, builder, module, ctx,
                );
            }
            SsaInst::Pipeline {
                stages,
                capture_read_fd,
                checked,
                suppressed,
            } => {
                return super::inst_handlers::handle_pipeline(
                    stages,
                    *capture_read_fd,
                    *checked,
                    suppressed,
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
                return self.emit_bless(source, target, test, builder, module, ctx);
            }
            SsaInst::BlessAll {
                source,
                binders,
                test,
                is_map,
                if_form,
            } => {
                return self.emit_bless_all(
                    source, binders, test, *is_map, *if_form, builder, module, ctx,
                );
            }
            SsaInst::BlessRetag { source, target } => {
                return self.emit_bless_retag(source, target, builder, module, ctx);
            }
        }
        false // block not terminated
    }

    /// Dispatch an `SsaInst::Command` on its `BuiltinCmd` kind.
    ///
    /// SECURITY: Every BuiltinCmd variant is handled. Variants not listed
    /// explicitly fall through to `handle_general_command`, which dispatches
    /// on `name` (printf, len/keys/join intrinsics, exec lookup, …).
    #[allow(clippy::too_many_arguments)]
    fn emit_command_kind(
        &self,
        name: &str,
        kind: &BuiltinCmd,
        args: &[SsaValue],
        redirects: &[crate::ir::Redirect],
        checked: &bool,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        match kind {
            BuiltinCmd::Echo => {
                return self.emit_echo(args, redirects, *checked, builder, module, ctx)
            }
            BuiltinCmd::Test => {
                // Legacy path — should not be reached now that transform
                // emits SsaInst::Test directly. Treat as always-true.
                let one = builder.ins().iconst(I64, 1);
                builder.def_var(ctx.test_result_var, one);
            }
            BuiltinCmd::NegateTestResult => self.emit_negate_test_result(builder, ctx),
            BuiltinCmd::LetExit => return self.emit_let_exit(args, builder, module, ctx),
            BuiltinCmd::SubshellStart => self.emit_subshell_start(builder, module, ctx),
            BuiltinCmd::RedirectPush => self.emit_redirect_push(redirects, builder, module, ctx),
            BuiltinCmd::RedirectPop => self.emit_redirect_pop(builder, module, ctx),
            BuiltinCmd::StoreBgPid => return self.emit_store_bg_pid(args, builder, module, ctx),
            BuiltinCmd::TrapSignal => self.emit_trap_signal(args, builder, module, ctx),
            BuiltinCmd::SignalFlag => self.emit_signal_flag(args, builder, module, ctx),
            BuiltinCmd::TableDelete => return self.emit_table_delete(args, builder, module, ctx),
            BuiltinCmd::Noop => self.emit_noop(builder, ctx),
            BuiltinCmd::Match => self.emit_match(args, builder, module, ctx),
            BuiltinCmd::Exit => return self.emit_exit(args, builder, module, ctx),
            BuiltinCmd::Exec => return self.emit_exec(name, args, redirects, builder, module, ctx),
            BuiltinCmd::Cd => {
                return super::inst_handlers::handle_cd(args, *checked, builder, module, ctx)
            }
            BuiltinCmd::Wait => return self.emit_wait(args, builder, module, ctx),
            BuiltinCmd::CommandBuiltin => {
                return self.emit_command_builtin(name, args, redirects, builder, module, ctx);
            }
            // All other builtins (read, mapfile, printf, bless, etc.) and
            // external commands / user functions fall through to the general
            // command handler, which dispatches on `name` for the remaining
            // cases (printf, len/keys/join intrinsics, exec lookup, …).
            _ => {
                return super::inst_handlers::handle_general_command(
                    name, args, redirects, checked, builder, module, ctx,
                );
            }
        }
        false
    }

    fn emit_negate_test_result(&self, builder: &mut FunctionBuilder, ctx: &mut InstCtx) {
        let test_result_var = ctx.test_result_var;
        let exit_status_var = ctx.exit_status_var;
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

    fn emit_subshell_start(
        &self,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) {
        // Fork: pid = sash_fork(). Store pid in test_result_var
        // so the Branch terminator can dispatch on it.
        let fork_ref = module.declare_func_in_func(ctx.externs.fork, builder.func);
        let call = builder.ins().call(fork_ref, &[]);
        let pid = builder.inst_results(call)[0];
        builder.def_var(ctx.test_result_var, pid);
    }

    fn emit_redirect_push(
        &self,
        redirects: &[crate::ir::Redirect],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) {
        // Compound-command redirect: save fds, apply redirects
        let saved = emit_redirects(
            redirects,
            builder,
            module,
            ctx.externs,
            ctx.literals,
            ctx.vars,
            Some(ctx.exit_status_var),
        );
        ctx.redirect_stack.push(saved);
    }

    fn emit_redirect_pop(
        &self,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) {
        // Compound-command redirect: restore saved fds
        if let Some(saved) = ctx.redirect_stack.pop() {
            emit_restore(saved, builder, module, ctx.externs);
        }
    }

    fn emit_trap_signal(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) {
        // trap '' SIG or trap - SIG: call sash_trap(signum, handler)
        if args.len() >= 2 {
            if let (SsaValue::Literal(sig_s), SsaValue::Literal(handler_s)) = (&args[0], &args[1]) {
                let signum = builder.ins().iconst(I64, sig_s.parse::<i64>().unwrap_or(0));
                let handler = builder
                    .ins()
                    .iconst(I64, handler_s.parse::<i64>().unwrap_or(0));
                let trap_ref = module.declare_func_in_func(ctx.externs.trap, builder.func);
                builder.ins().call(trap_ref, &[signum, handler]);
            }
        }
    }

    fn emit_signal_flag(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) {
        // signal N FLAG: call sash_signal_flag(signum)
        if let Some(SsaValue::Literal(sig_s)) = args.first() {
            let signum = builder.ins().iconst(I64, sig_s.parse::<i64>().unwrap_or(0));
            let flag_ref = module.declare_func_in_func(ctx.externs.signal_flag, builder.func);
            builder.ins().call(flag_ref, &[signum]);
        }
    }

    fn emit_noop(&self, builder: &mut FunctionBuilder, ctx: &mut InstCtx) {
        // `:` succeeds silently: test_result = 1, $? = 0. (true/false are
        // external programs now.)
        let one = builder.ins().iconst(I64, 1);
        builder.def_var(ctx.test_result_var, one);
        let zero = builder.ins().iconst(I64, 0);
        builder.def_var(ctx.exit_status_var, zero);
    }

    /// DD#63: `match SUBJECT PATTERN` boolean command. Calls the real
    /// `sash_regex_match` (1 = matched, 0 = not) and sets `test_result`
    /// (the boolean for if/while/&&/||) and `$?` (0 matched / 1 not). A
    /// (B) must-consume result (DD#70) — the front-end rejects a bare or
    /// `-`-suppressed `match`, so this only runs in a consuming position.
    fn emit_match(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) {
        // Arity is validated up front; defensively treat a malformed call
        // as a non-match rather than indexing out of bounds.
        let (Some(subject), Some(pattern)) = (args.first(), args.get(1)) else {
            let zero = builder.ins().iconst(I64, 0);
            builder.def_var(ctx.test_result_var, zero);
            let one = builder.ins().iconst(I64, 1);
            builder.def_var(ctx.exit_status_var, one);
            return;
        };
        let (s_ptr, s_len) = lower_value(
            subject,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            ctx.func_constants,
            ctx.externs,
            Some(ctx.exit_status_var),
        );
        let (p_ptr, p_len) = lower_value(
            pattern,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            ctx.func_constants,
            ctx.externs,
            Some(ctx.exit_status_var),
        );
        let regex_ref = module.declare_func_in_func(ctx.externs.regex_match, builder.func);
        let call = builder.ins().call(regex_ref, &[s_ptr, s_len, p_ptr, p_len]);
        let matched = builder.inst_results(call)[0]; // 1 = matched, 0 = not
        builder.def_var(ctx.test_result_var, matched);
        // $? = matched ? 0 : 1  (command exit status; 1 - matched, matched is 0/1)
        let one = builder.ins().iconst(I64, 1);
        let es = builder.ins().isub(one, matched);
        builder.def_var(ctx.exit_status_var, es);
    }

    fn emit_exec(
        &self,
        _name: &str,
        args: &[SsaValue],
        redirects: &[crate::ir::Redirect],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        if args.is_empty() {
            emit_exec_redirects_only(redirects, builder, module, ctx);
            false
        } else if has_dollar_at(args) {
            emit_exec_with_dollar_at(args, builder, module, ctx);
            true
        } else {
            emit_exec_static_argv(args, builder, module, ctx);
            true
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_readline_multi(
        &self,
        targets: &[VarRef],
        fd: &i64,
        delim: &Option<u8>,
        redirects: &[crate::ir::Redirect],
        field_sep: &Option<u8>,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let literals = ctx.literals;
        let externs = ctx.externs;
        let exit_status_var = ctx.exit_status_var;
        let test_result_var = ctx.test_result_var;
        let pipe_read_fd_var = ctx.pipe_read_fd_var;
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
        false
    }

    /// TRUST: bless is the ONLY mechanism to move tainted → clean.
    fn emit_bless(
        &self,
        source: &VarRef,
        target: &VarRef,
        test: &crate::ssa::SsaTest,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let literals = ctx.literals;
        let externs = ctx.externs;
        let func_constants = ctx.func_constants;
        let exit_status_var = ctx.exit_status_var;
        // Runtime bless enforcement: evaluate test on source, abort if false.
        // On pass, copy source value to target (new clean SSA version).
        let result = lower_test_expr(
            test,
            builder,
            module,
            vars,
            literals,
            func_constants,
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
        // bless failed: `<argv0>: bless: <name> = '<val>' failed the gate`,
        // exit(1). Name + value tell the user which gate failed and why.
        let name_id = literals[source.name.as_str()];
        let name_gv = module.declare_data_in_func(name_id, builder.func);
        let name_ptr = builder.ins().global_value(I64, name_gv);
        let name_len = builder.ins().iconst(I64, source.name.len() as i64);
        // The source value (for `= '<val>'`) if it is a string slot.
        let src_key = format!("{}_{}", source.name, source.version);
        let (val_ptr, val_len) = match vars.get(src_key.as_str()) {
            Some(VarSlot::Str { ptr, len }) => {
                let (p, l) = (*ptr, *len);
                (builder.use_var(p), builder.use_var(l))
            }
            _ => {
                let z = builder.ins().iconst(I64, 0);
                (z, z)
            }
        };
        let bless_fail_ref = module.declare_func_in_func(externs.bless_fail, builder.func);
        builder
            .ins()
            .call(bless_fail_ref, &[name_ptr, name_len, val_ptr, val_len]);
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
        false
    }

    /// DD#61: retag an array/map clean after a successful `BlessAll`.
    /// Copies the source slot (the collection's backing table pointer)
    /// to a new clean SSA version. No runtime validation.
    fn emit_bless_retag(
        &self,
        source: &VarRef,
        target: &VarRef,
        builder: &mut FunctionBuilder,
        _module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let src_key = format!("{}_{}", source.name, source.version);
        // Tables live in `_0`; fall back to that if the exact version slot
        // isn't present (arrays are mutated in place, not re-versioned).
        let slot = vars
            .get(src_key.as_str())
            .or_else(|| vars.get(format!("{}_0", source.name).as_str()))
            .cloned();
        if let Some(slot) = slot {
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
        false
    }

    /// DD#61: `bless x in a [[…]]` / `bless k v in m [[…]]` — validate every
    /// element of the array/map against the per-element `test`. Iterates the
    /// backing table, binding the element value (and key, for maps) to fresh
    /// codegen slots so the predicate can read them, and evaluates `test`.
    ///
    /// Statement form (`if_form == false`): a failing element writes the
    /// bless error and exits 1; reaching the end sets the test-result var to
    /// true. If form: AND-accumulates each result and leaves the boolean in
    /// the test-result var for the surrounding branch. An empty collection
    /// is vacuously true.
    #[allow(clippy::too_many_arguments)]
    fn emit_bless_all(
        &self,
        source: &VarRef,
        binders: &[VarRef],
        test: &crate::ssa::SsaTest,
        is_map: bool,
        if_form: bool,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let externs = ctx.externs;
        let func_constants = ctx.func_constants;
        let exit_status_var = ctx.exit_status_var;
        let test_result_var = ctx.test_result_var;

        // Backing table pointer (tables live in `_0` slots).
        let table_ptr = {
            let sv = find_array_slot(ctx.vars, &source.name, "bless-all array");
            builder.use_var(sv.str_ptr())
        };
        let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
        let len_call = builder.ins().call(len_ref, &[table_ptr]);
        let len = builder.inst_results(len_call)[0];

        // Loop-carried Variables: counter and (if-form) the all-passed flag.
        let counter = builder.declare_var(I64);
        let zero = builder.ins().iconst(I64, 0);
        builder.def_var(counter, zero);
        let all_passed = builder.declare_var(I64);
        let init_true = builder.ins().iconst(I64, 1);
        builder.def_var(all_passed, init_true);

        // Binder slots use the binders' own (globally-unique) SSA versions,
        // so the predicate's pre-lowered operand refs resolve to exactly
        // these slots. Removed after the loop so the outer scope is
        // unaffected (DD#61 shadow semantics).
        let (binder_keys, binder_ptrs, binder_lens) = declare_bless_binders(binders, ctx, builder);

        let header = builder.create_block();
        let body = builder.create_block();
        let after = builder.create_block();
        builder.ins().jump(header, &[]);

        // header: counter < len ? body : after
        builder.switch_to_block(header);
        let c = builder.use_var(counter);
        let cond = builder.ins().icmp(IntCC::SignedLessThan, c, len);
        builder.ins().brif(cond, body, &[], after, &[]);

        // body: bind element(s), evaluate the predicate.
        builder.switch_to_block(body);
        builder.seal_block(body);
        let ci = builder.use_var(counter);
        if is_map {
            let (kp, kl) = emit_table_lookup(externs.table_key, table_ptr, ci, builder, module);
            builder.def_var(binder_ptrs[0], kp);
            builder.def_var(binder_lens[0], kl);
            let (vp, vl) = emit_table_lookup(externs.table_value, table_ptr, ci, builder, module);
            builder.def_var(binder_ptrs[1], vp);
            builder.def_var(binder_lens[1], vl);
        } else {
            let (vp, vl) = emit_table_lookup(externs.table_value, table_ptr, ci, builder, module);
            builder.def_var(binder_ptrs[0], vp);
            builder.def_var(binder_lens[0], vl);
        }
        let result = lower_test_expr(
            test,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            func_constants,
            externs,
            Some(exit_status_var),
        );

        if if_form {
            let ap = builder.use_var(all_passed);
            let ap2 = builder.ins().band(ap, result);
            builder.def_var(all_passed, ap2);
            let cnext = builder.ins().iadd_imm(ci, 1);
            builder.def_var(counter, cnext);
            builder.ins().jump(header, &[]);
        } else {
            let cont = builder.create_block();
            let fail = builder.create_block();
            builder.ins().brif(result, cont, &[], fail, &[]);
            // fail: "bless: validation failed" → stderr, exit(1)
            builder.switch_to_block(fail);
            builder.seal_block(fail);
            self.emit_bless_fail(&source.name, builder, module, ctx.literals, externs);
            // cont: counter++ ; loop
            builder.switch_to_block(cont);
            builder.seal_block(cont);
            let cnext = builder.ins().iadd_imm(ci, 1);
            builder.def_var(counter, cnext);
            builder.ins().jump(header, &[]);
        }

        // header's predecessors (entry + back-edge) are now complete.
        builder.seal_block(header);

        builder.switch_to_block(after);
        builder.seal_block(after);
        let passed = if if_form {
            builder.use_var(all_passed)
        } else {
            builder.ins().iconst(I64, 1)
        };
        builder.def_var(test_result_var, passed);

        for key in binder_keys {
            ctx.vars.remove(&key);
        }
        false
    }

    /// Emit the bless failure path: `<argv0>: bless: <name> failed the gate`
    /// to stderr, then exit(1). Shared by the statement-form `BlessAll`
    /// (no single scalar value — the value clause is omitted). Scalar
    /// `Bless` emits its own call inline (it also passes the value).
    fn emit_bless_fail(
        &self,
        name: &str,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        literals: &std::collections::HashMap<String, cranelift_module::DataId>,
        externs: &ExternFuncs,
    ) {
        let name_id = literals[name];
        let name_gv = module.declare_data_in_func(name_id, builder.func);
        let name_ptr = builder.ins().global_value(I64, name_gv);
        let name_len = builder.ins().iconst(I64, name.len() as i64);
        let zero = builder.ins().iconst(I64, 0);
        let bless_fail_ref = module.declare_func_in_func(externs.bless_fail, builder.func);
        builder
            .ins()
            .call(bless_fail_ref, &[name_ptr, name_len, zero, zero]);
        builder
            .ins()
            .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
    }

    fn emit_wait(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let literals = ctx.literals;
        let externs = ctx.externs;
        let exit_status_var = ctx.exit_status_var;
        let func_constants = ctx.func_constants;
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
        false
    }

    fn emit_command_builtin(
        &self,
        _name: &str,
        args: &[SsaValue],
        redirects: &[crate::ir::Redirect],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let literals = ctx.literals;
        let externs = ctx.externs;
        let exit_status_var = ctx.exit_status_var;
        let test_result_var = ctx.test_result_var;
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
        false
    }

    fn emit_array_append(
        &self,
        array: &str,
        value: &SsaValue,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let literals = ctx.literals;
        let externs = ctx.externs;
        let exit_status_var = ctx.exit_status_var;
        let func_constants = ctx.func_constants;
        let sv = find_array_slot(vars, array, "array");
        let table_ptr = builder.use_var(sv.str_ptr());

        // Get current length as the next integer key
        let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
        let call = builder.ins().call(len_ref, &[table_ptr]);
        let next_idx = builder.inst_results(call)[0];

        let idx_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
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
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_readline(
        &self,
        target: &VarRef,
        fd: &i64,
        delim: &Option<u8>,
        redirects: &[crate::ir::Redirect],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let literals = ctx.literals;
        let externs = ctx.externs;
        let exit_status_var = ctx.exit_status_var;
        let test_result_var = ctx.test_result_var;
        let pipe_read_fd_var = ctx.pipe_read_fd_var;
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
        false
    }

    fn emit_echo(
        &self,
        args: &[SsaValue],
        redirects: &[crate::ir::Redirect],
        checked: bool,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let in_condition = ctx.in_condition;
        // DD#55: a redirect open(2) failure skips the echo body (no
        // misroute) and aborts unless caught; on success the echo runs and
        // its own write verdict applies.
        super::inst_handlers::emit_redirect_guarded_body(
            redirects,
            in_condition,
            builder,
            module,
            ctx,
            |b, m, c| emit_echo_body(args, checked, b, m, c),
        );
        false
    }

    fn emit_let_exit(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let externs = ctx.externs;
        let literals = ctx.literals;
        let func_constants = ctx.func_constants;
        let exit_status_var = ctx.exit_status_var;
        let test_result_var = ctx.test_result_var;
        // let builtin / loop bound: exit status 0 if value != 0, 1 if == 0.
        // The arg is any integer-valued SsaValue (a `(( ))` result, or a
        // for-loop bound comparison `__ai < ${arr arrlen}`).
        if let Some(arg) = args.first() {
            let int_val = super::values::lower_value_as_int(
                arg,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                Some(exit_status_var),
            );
            let zero = builder.ins().iconst(I64, 0);
            let is_zero = builder.ins().icmp(IntCC::Equal, int_val, zero);
            let one = builder.ins().iconst(I64, 1);
            let es = builder.ins().select(is_zero, one, zero);
            builder.def_var(exit_status_var, es);
            // Also set test_result_var for branch terminators (if/while/for).
            let tr = builder.ins().select(is_zero, zero, one);
            builder.def_var(test_result_var, tr);
        }
        false
    }

    fn emit_store_bg_pid(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let test_result_var = ctx.test_result_var;
        let externs = ctx.externs;
        // Store fork pid (test_result_var) into $! variable
        if let Some(SsaValue::Var(vr)) = args.first() {
            let pid = builder.use_var(test_result_var);
            // Convert pid to string via itoa
            let (str_ptr, str_len) = emit_int_to_str(builder, module, externs, pid);
            let sv = &vars[&vr.display()];
            builder.def_var(sv.str_ptr(), str_ptr);
            builder.def_var(sv.str_len(), str_len);
        }
        false
    }

    fn emit_table_delete(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let literals = ctx.literals;
        let externs = ctx.externs;
        let exit_status_var = ctx.exit_status_var;
        let func_constants = ctx.func_constants;
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
        false
    }

    fn emit_exit(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let externs = ctx.externs;
        let exit_status_var = ctx.exit_status_var;
        let end_cl_block = ctx.end_cl_block;
        let saved_end_status_var = ctx.saved_end_status_var;
        let program = ctx.program;
        let ssa_id = ctx.ssa_id;
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
        true // block is terminated
    }

    fn emit_read_array(
        &self,
        array: &str,
        fd: &i64,
        delim: &Option<u8>,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let externs = ctx.externs;
        let pipe_read_fd_var = ctx.pipe_read_fd_var;
        // Slurp all input from fd, then split by delimiter into indexed array
        let sv = find_array_slot(vars, array, "array");
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
        false
    }

    fn emit_capture_end(
        &self,
        target: &VarRef,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let externs = ctx.externs;
        let capture_saved_var = ctx.capture_saved_var;
        let capture_pipe_var = ctx.capture_pipe_var;
        // Call sash_capture_end(saved_fd, pipe_read_fd) → (ptr, len)
        let saved = builder.use_var(capture_saved_var);
        let pipe_rd = builder.use_var(capture_pipe_var);
        let cap_ref = module.declare_func_in_func(externs.capture_end, builder.func);
        let call = builder.ins().call(cap_ref, &[saved, pipe_rd]);
        let ptr = builder.inst_results(call)[0];
        let len = builder.inst_results(call)[1];
        let sv = vars[&target.display()];
        match sv {
            VarSlot::Int(v) => {
                // Int var: atoi the captured string
                let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                let atoi_call = builder.ins().call(atoi_ref, &[ptr, len]);
                let int_val = builder.inst_results(atoi_call)[0];
                builder.def_var(v, int_val);
            }
            VarSlot::Str {
                ptr: dst_ptr,
                len: dst_len,
            } => {
                builder.def_var(dst_ptr, ptr);
                builder.def_var(dst_len, len);
            }
        }
        false
    }
}

/// `exec` with no args: standalone fd manipulation. Applies the
/// redirects in the current process and sets $? = 0.
fn emit_exec_redirects_only(
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    if !redirects.is_empty() {
        emit_redirects(
            redirects,
            builder,
            module,
            ctx.externs,
            ctx.literals,
            ctx.vars,
            Some(ctx.exit_status_var),
        );
    }
    let zero = builder.ins().iconst(I64, 0);
    builder.def_var(ctx.exit_status_var, zero);
}

fn has_dollar_at(args: &[SsaValue]) -> bool {
    args.iter().any(is_dollar_at)
}

fn is_dollar_at(v: &SsaValue) -> bool {
    matches!(v, SsaValue::Var(vref) if vref.name == "@")
}

/// `exec cmd args... $@ ...` — splice the program argv into the args
/// vector at runtime via sash_splice_argv.
fn emit_exec_with_dollar_at(
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let prefix_args: Vec<&SsaValue> = args.iter().filter(|a| !is_dollar_at(a)).collect();
    let prefix_addr = build_argv_on_stack(&prefix_args, builder, module, ctx);
    let merged_argv = splice_in_program_argv(prefix_addr, builder, module, ctx.externs);
    finalize_exec(merged_argv, builder, module, ctx.externs);
}

/// `exec cmd args...` (no $@) — build a static argv on the stack and
/// hand it straight to exec.
fn emit_exec_static_argv(
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let arg_refs: Vec<&SsaValue> = args.iter().collect();
    let argv_addr = build_argv_on_stack(&arg_refs, builder, module, ctx);
    finalize_exec(argv_addr, builder, module, ctx.externs);
}

/// Allocate a stack-resident argv (lower each arg, store pointers,
/// null-terminate) and return its base address.
fn build_argv_on_stack(
    args: &[&SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> cranelift_codegen::ir::Value {
    let argv_size = ((args.len() + 1) * 8) as u32;
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
            ctx.vars,
            ctx.literals,
            ctx.func_constants,
            ctx.externs,
            Some(ctx.exit_status_var),
        );
        builder.ins().stack_store(ptr, argv_slot, (i * 8) as i32);
    }
    let null = builder.ins().iconst(I64, 0);
    builder
        .ins()
        .stack_store(null, argv_slot, (args.len() * 8) as i32);
    builder.ins().stack_addr(I64, argv_slot, 0)
}

/// Load the program's argv/argc and call sash_splice_argv to merge
/// them with the caller's prefix. Returns the merged argv pointer.
fn splice_in_program_argv(
    prefix_addr: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
    let argv_ptr = builder.ins().global_value(I64, argv_gv);
    let argv_val = builder.ins().load(I64, MemFlags::new(), argv_ptr, 0);
    let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
    let argc_ptr = builder.ins().global_value(I64, argc_gv);
    let argc_val = builder.ins().load(I64, MemFlags::new(), argc_ptr, 0);
    let splice_ref = module.declare_func_in_func(externs.splice_argv, builder.func);
    let call = builder
        .ins()
        .call(splice_ref, &[prefix_addr, argv_val, argc_val]);
    builder.inst_results(call)[0]
}

/// Common tail for both exec paths: call exec(argv, envp); if it returns,
/// exit 127, then trap (exec is no-return on success).
fn finalize_exec(
    argv_addr: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) {
    let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
    let envp_ptr = builder.ins().global_value(I64, envp_gv);
    let envp_val = builder.ins().load(I64, MemFlags::new(), envp_ptr, 0);
    let exec_ref = module.declare_func_in_func(externs.exec, builder.func);
    builder.ins().call(exec_ref, &[argv_addr, envp_val]);
    let code = builder.ins().iconst(I64, 127);
    let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
    builder.ins().call(exit_ref, &[code]);
    builder
        .ins()
        .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
}

/// Call `sash_table_value` / `sash_table_key` `(table, index)` and return
/// the `(ptr, len)` pair, using a fresh stack slot for the out-length.
/// DD#61: declare per-binder `(ptr,len)` slots for a bless-all predicate and
/// register them in `ctx.vars` under the binders' globally-unique SSA keys
/// (so the pre-lowered predicate operands resolve to exactly these slots).
/// Returns `(keys, ptrs, lens)` for the caller to bind and later remove.
#[allow(clippy::type_complexity)]
fn declare_bless_binders(
    binders: &[VarRef],
    ctx: &mut InstCtx,
    builder: &mut FunctionBuilder,
) -> (
    Vec<String>,
    Vec<cranelift_frontend::Variable>,
    Vec<cranelift_frontend::Variable>,
) {
    let mut keys = Vec::new();
    let mut ptrs = Vec::new();
    let mut lens = Vec::new();
    for b in binders {
        let key = b.display();
        let bp = builder.declare_var(I64);
        let bl = builder.declare_var(I64);
        ctx.vars
            .insert(key.clone(), VarSlot::Str { ptr: bp, len: bl });
        keys.push(key);
        ptrs.push(bp);
        lens.push(bl);
    }
    (keys, ptrs, lens)
}

fn emit_table_lookup(
    func: cranelift_module::FuncId,
    table_ptr: cranelift_codegen::ir::Value,
    index: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let out_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
    let fref = module.declare_func_in_func(func, builder.func);
    let call = builder.ins().call(fref, &[table_ptr, index, out_addr]);
    let ptr = builder.inst_results(call)[0];
    let len = builder.ins().stack_load(I64, out_slot, 0);
    (ptr, len)
}

/// DD#55: the echo command body (write + verdict + diagnostic) run inside
/// the redirect-open guard's success path. Lowers the args, sets `$?`/
/// test_result from the write status, and applies the (A)-channel abort
/// with the `<argv0>: echo: write error: <reason>` diagnostic.
fn emit_echo_body(
    args: &[SsaValue],
    checked: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let failed = lower_echo(
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
    builder.def_var(ctx.exit_status_var, failed);
    let zero = builder.ins().iconst(I64, 0);
    let ok = builder.ins().icmp(IntCC::Equal, failed, zero);
    let tr = builder.ins().uextend(I64, ok);
    builder.def_var(ctx.test_result_var, tr);
    let (s1p, s1l) =
        super::inst_handlers::diag_static_seg("echo: write error", ctx.literals, module, builder);
    let (s2p, s2l) = super::inst_handlers::diag_empty_seg(builder);
    let msg = super::inst_handlers::AbortMsg::Errno {
        seg1_ptr: s1p,
        seg1_len: s1l,
        seg2_ptr: s2p,
        seg2_len: s2l,
    };
    super::inst_handlers::emit_status_abort_check(
        builder,
        module,
        ctx.externs,
        ctx.exit_status_var,
        ctx.end_cl_block,
        checked,
        Some(msg),
    );
}
