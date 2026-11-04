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

use super::commands::lower_echo;
use super::redirects::emit_discard_saved;
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
        let test_result_var = ctx.test_result_var;
        let exit_status_var = ctx.exit_status_var;
        let capture_slots = ctx.capture_slots;
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
            SsaInst::ReadArray {
                target,
                fd,
                redirects,
                ..
            } => {
                return self.emit_read_array(&target.name, fd, redirects, builder, module, ctx);
            }
            SsaInst::SplitValue { source, target } => {
                return self.emit_split_value(source, &target.name, builder, module, ctx);
            }
            SsaInst::MapFromArray {
                source,
                target,
                mode,
                key,
                ..
            } => {
                return self.emit_map_from_array(
                    source,
                    &target.name,
                    *mode,
                    key,
                    builder,
                    module,
                    ctx,
                );
            }
            SsaInst::MatchInto {
                subject,
                pattern,
                array,
                all,
            } => {
                return super::inst_handlers::handle_match_into(
                    subject, pattern, array, *all, builder, module, ctx,
                );
            }
            SsaInst::ArgvFill { target, start } => {
                return super::inst_handlers::handle_argv_fill(
                    &target.name,
                    *start,
                    builder,
                    module,
                    ctx,
                );
            }
            SsaInst::KeysFill { source, target } => {
                return super::inst_handlers::handle_keys_fill(
                    source,
                    &target.name,
                    builder,
                    module,
                    ctx,
                );
            }
            SsaInst::CaptureStart {
                site,
                buffered: true,
            } => {
                // Pure site: collect into a userspace buffer. No fd is
                // created and no slot is used, so stdout is never redirected
                // — `sash_write` diverts at the fd-1 boundary instead.
                let _ = site;
                let cap_ref = module.declare_func_in_func(externs.capture_start_buf, builder.func);
                builder.ins().call(cap_ref, &[]);
            }
            SsaInst::CaptureStart { site, .. } => {
                // Call sash_capture_start() → (saved_fd, capture_handle).
                // Each site owns its slot pair, so an inner capture cannot
                // overwrite the handles an enclosing one still needs.
                let (saved_var, handle_var) = capture_slots[*site as usize];
                let cap_ref = module.declare_func_in_func(externs.capture_start, builder.func);
                let call = builder.ins().call(cap_ref, &[]);
                let saved = builder.inst_results(call)[0];
                let handle = builder.inst_results(call)[1];
                builder.def_var(saved_var, saved);
                builder.def_var(handle_var, handle);
            }
            SsaInst::CaptureEnd {
                site,
                buffered,
                target,
                ..
            } => {
                return self.emit_capture_end(*site, *buffered, target, builder, module, ctx);
            }
            SsaInst::RedirectPush {
                site,
                redirects,
                checked,
            } => {
                super::inst_handlers::emit_redirect_push(
                    *site, redirects, *checked, builder, module, ctx,
                );
            }
            SsaInst::RedirectPop { site, fds } => {
                super::inst_handlers::emit_redirect_pop(*site, fds, builder, module, ctx);
            }
            SsaInst::Test(test_expr, _) => {
                let result = lower_test_expr(
                    test_expr,
                    builder,
                    module,
                    vars,
                    literals,
                    externs,
                    exit_status_var,
                );
                builder.def_var(test_result_var, result);
                // A `[[ … ]]` publishes its status like every other producer,
                // so a guard that does not fire (`[[ a == b ]] && cmd`) leaves
                // rc 1 rather than whatever status happened to precede it.
                // `if`/`while` reset to 0 on their own false path, so only the
                // `&&`/`||` guard forms observe this. Provisional (DG).
                let passed = builder.ins().icmp_imm(IntCC::NotEqual, result, 0);
                let zero = builder.ins().iconst(I64, 0);
                let one = builder.ins().iconst(I64, 1);
                let status = builder.ins().select(passed, zero, one);
                builder.def_var(exit_status_var, status);
            }
            SsaInst::ReadLine {
                target,
                fd,
                redirects,
                ..
            } => {
                return self.emit_readline(target, fd, redirects, builder, module, ctx);
            }
            SsaInst::Pipeline {
                stages,
                capture,
                checked,
                suppressed,
                negated,
            } => {
                return super::inst_handlers::handle_pipeline(
                    stages, capture, *checked, suppressed, negated, builder, module, ctx,
                );
            }
            SsaInst::PipelineFinish {
                site,
                checked,
                preserve_status,
                verdict_joins,
            } => {
                return super::inst_handlers::handle_pipeline_finish(
                    *site,
                    *checked,
                    *preserve_status,
                    *verdict_joins,
                    builder,
                    module,
                    ctx,
                );
            }
            SsaInst::Attest {
                source,
                target,
                test,
            } => {
                return self.emit_attest(source, target, test, builder, module, ctx);
            }
            SsaInst::AttestAll {
                source,
                binders,
                test,
                is_map,
                if_form,
            } => {
                return self.emit_attest_all(
                    source, binders, test, *is_map, *if_form, builder, module, ctx,
                );
            }
            SsaInst::AttestRetag { source, target } => {
                return self.emit_attest_retag(source, target, ctx);
            }
        }
        false // block not terminated
    }

    /// Dispatch an `SsaInst::Command` on its `BuiltinCmd` kind.
    ///
    /// **Total over `BuiltinCmd` on purpose — do not add a wildcard.** Only
    /// `Other` reaches external-command emission; rewritten/lowered builtins
    /// must not become bare exec targets when a compiler invariant breaks.
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
            BuiltinCmd::NegateTestResult => self.emit_negate_test_result(builder, ctx),
            BuiltinCmd::SetTestFromStatus => self.emit_set_test_from_status(builder, ctx),
            BuiltinCmd::SetTestFromPipe => {
                let [arg] = BuiltinCmd::SetTestFromPipe.expect_ssa_args(args);
                let fd = BuiltinCmd::SetTestFromPipe.expect_literal_int(arg);
                let fd = super::inst_handlers::read_fd(fd, builder, ctx);
                let ready = builder
                    .ins()
                    .icmp_imm(IntCC::SignedGreaterThanOrEqual, fd, 0);
                let ready = builder.ins().uextend(I64, ready);
                builder.def_var(ctx.test_result_var, ready);
            }
            BuiltinCmd::LetExit => return self.emit_let_exit(args, builder, module, ctx),
            BuiltinCmd::StoreBgPid => {
                return self.emit_store_bg_pid(args, *checked, builder, module, ctx)
            }
            BuiltinCmd::TrapSignal => self.emit_trap_signal(args, builder, module, ctx),
            BuiltinCmd::SignalFlag => self.emit_signal_flag(args, builder, module, ctx),
            BuiltinCmd::SignalCheckpoint => {
                let [] = BuiltinCmd::SignalCheckpoint.expect_ssa_args(args);
                super::inst_handlers::emit_signal_exit_check(builder, module, ctx);
            }
            BuiltinCmd::TableDelete | BuiltinCmd::TableDeleteIdx => {
                return self.emit_table_delete(args, *kind, builder, module, ctx)
            }
            BuiltinCmd::Noop => self.emit_noop(args, redirects, builder, module, ctx),
            BuiltinCmd::Match => self.emit_match(args, builder, module, ctx),
            BuiltinCmd::Exit => return self.emit_exit(args, builder, module, ctx),
            BuiltinCmd::Exec => return self.emit_exec(name, args, redirects, builder, module, ctx),
            BuiltinCmd::Cd => {
                return super::inst_handlers::handle_cd(args, *checked, builder, module, ctx)
            }
            BuiltinCmd::Close => {
                return super::inst_handlers::handle_close(args, *checked, builder, module, ctx)
            }
            BuiltinCmd::Wait => return self.emit_wait(args, builder, module, ctx),
            BuiltinCmd::Other => {
                return super::inst_handlers::handle_general_command(
                    name, args, redirects, checked, builder, module, ctx,
                );
            }
            BuiltinCmd::Write
            | BuiltinCmd::Len
            | BuiltinCmd::Strlen
            | BuiltinCmd::Keys
            | BuiltinCmd::Join
            | BuiltinCmd::Sub
            | BuiltinCmd::Gsub
            | BuiltinCmd::MatchAll
            | BuiltinCmd::Toupper
            | BuiltinCmd::Tolower
            | BuiltinCmd::Basename
            | BuiltinCmd::Dirname
            | BuiltinCmd::Argv
            | BuiltinCmd::Test
            | BuiltinCmd::Attest
            | BuiltinCmd::Read
            | BuiltinCmd::Mapfile
            | BuiltinCmd::Break
            | BuiltinCmd::Continue
            | BuiltinCmd::Return
            | BuiltinCmd::Signal
            | BuiltinCmd::Unset => ice_unreachable_command(kind, name),
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

    /// `test_result = (exit_status == 0)` — sync the branch truth value from
    /// `$?` after a capture-condition (`if x=$(cmd)`), whose suppressed capture
    /// set the status but not the test result.
    fn emit_set_test_from_status(&self, builder: &mut FunctionBuilder, ctx: &mut InstCtx) {
        let test_result_var = ctx.test_result_var;
        let exit_status_var = ctx.exit_status_var;
        let cur_es = builder.use_var(exit_status_var);
        let zero = builder.ins().iconst(I64, 0);
        let one = builder.ins().iconst(I64, 1);
        let is_zero = builder.ins().icmp(IntCC::Equal, cur_es, zero);
        let result = builder.ins().select(is_zero, one, zero);
        builder.def_var(test_result_var, result);
    }

    fn emit_trap_signal(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) {
        // handle_signal emits exactly two integer literals for IGN / DFL.
        let kind = BuiltinCmd::TrapSignal;
        let [signal, handler] = kind.expect_ssa_args(args);
        let signum = builder.ins().iconst(I64, kind.expect_literal_int(signal));
        let handler = builder.ins().iconst(I64, kind.expect_literal_int(handler));
        let trap_ref = module.declare_func_in_func(ctx.externs.trap, builder.func);
        builder.ins().call(trap_ref, &[signum, handler]);
    }

    fn emit_signal_flag(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) {
        // handle_signal emits the signal number and a FLAG/EXIT bit.
        let (signal, exiting) = BuiltinCmd::expect_signal_flag(args);
        let signum = builder.ins().iconst(I64, signal);
        let exiting = builder.ins().iconst(I64, exiting);
        let flag_ref = module.declare_func_in_func(ctx.externs.signal_flag, builder.func);
        builder.ins().call(flag_ref, &[signum, exiting]);
    }

    /// `:` succeeds silently: test_result = 1, $? = 0. (true/false are
    /// external programs now.)
    ///
    /// DD-less exception to SB4046 (DG, 2026-10-11): `:` produces no output,
    /// yet `: >f` is the universal truncate-a-file idiom, so it honours its
    /// redirects instead of being rejected with the other no-output
    /// builtins. It used to *silently drop* them — this function took no
    /// `redirects` parameter at all, so `: >f` compiled clean and never
    /// opened the file. The guarded-body wrapper is the same one `echo` uses,
    /// which also gives `:` the DD#55 open-failure behaviour for free.
    ///
    /// Its arguments were dropped the same way, so `: ${s:?empty}` never
    /// aborted. They are now evaluated for their effects inside the body,
    /// after the redirect opens, as `echo` evaluates its own.
    fn emit_noop(
        &self,
        args: &[SsaValue],
        redirects: &[crate::ir::Redirect],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) {
        let in_condition = ctx.in_condition;
        super::inst_handlers::emit_redirect_guarded_body(
            redirects,
            in_condition,
            builder,
            module,
            ctx,
            |b, m, c| {
                super::commands::evaluate_args_for_effects(
                    args,
                    b,
                    m,
                    c.vars,
                    c.literals,
                    c.externs,
                    c.exit_status_var,
                );
                let one = b.ins().iconst(I64, 1);
                b.def_var(c.test_result_var, one);
                let zero = b.ins().iconst(I64, 0);
                b.def_var(c.exit_status_var, zero);
            },
        );
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
        // SB4045 admits 2 or 3 source args; try_handle_match_into removes the
        // 3-arg form. An extra SSA operand would lose its out-array silently.
        let [subject, pattern] = BuiltinCmd::Match.expect_ssa_args(args);
        let (s_ptr, s_len) = lower_value(
            subject,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            ctx.externs,
            ctx.exit_status_var,
        );
        let (p_ptr, p_len) = lower_value(
            pattern,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            ctx.externs,
            ctx.exit_status_var,
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
        super::inst_handlers::emit_redirect_guarded(
            redirects,
            ctx.in_condition,
            builder,
            module,
            ctx,
            |builder, module, ctx, saved| {
                emit_discard_saved(saved, builder, module, ctx.externs);
                if args.is_empty() {
                    emit_exec_redirects_only(redirects, builder, module, ctx);
                    false
                } else {
                    super::inst_handlers::emit_signal_exit_check(builder, module, ctx);
                    emit_exec_static_argv(args, builder, module, ctx);
                    true
                }
            },
        )
    }

    /// TRUST: attest is the ONLY mechanism to move tainted → clean.
    fn emit_attest(
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
        let exit_status_var = ctx.exit_status_var;
        // Runtime attest enforcement: evaluate test on source, abort if false.
        // On pass, copy source value to target (new clean SSA version).
        let result = lower_test_expr(
            test,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
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
        // attest failed: `<argv0>: attest: <name> = '<val>' failed the gate`,
        // exit(1). Name + value tell the user which gate failed and why.
        let name = ctx.program.source_names.original(&source.name);
        let name_id = literals[name];
        let name_gv = module.declare_data_in_func(name_id, builder.func);
        let name_ptr = builder.ins().global_value(I64, name_gv);
        let name_len = builder.ins().iconst(I64, name.len() as i64);
        // The source value, for `= '<val>'`. An int slot is rendered with
        // `sash_itoa` rather than dropped: the C backend already names the
        // value for an int (`emit_var_value` → `emit_itoa_temp`), so passing
        // zeros here made the two backends print different diagnostics for
        // the same program.
        //
        // One lookup serves both the fail block (the message) and the pass
        // block (the retagged copy). They ask the identical question, so they
        // must not be able to answer it differently — they used to, and the
        // pass block's answer was to skip the copy in silence. A miss is a
        // compiler fault; `types::var_slot` owns that claim.
        let src_key = format!("{}_{}", source.name, source.version);
        let src_slot = super::types::var_slot(vars, &src_key);
        let (val_ptr, val_len) = match src_slot {
            VarSlot::Str { ptr, len } => (ptr.load(builder), len.load(builder)),
            VarSlot::Int(v) => {
                let val = v.load(builder);
                super::types::emit_int_to_str(builder, module, externs, val)
            }
        };
        let attest_fail_ref = module.declare_func_in_func(externs.attest_fail, builder.func);
        builder
            .ins()
            .call(attest_fail_ref, &[name_ptr, name_len, val_ptr, val_len]);
        builder
            .ins()
            .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());

        builder.switch_to_block(pass_block);
        builder.seal_block(pass_block);

        // Attestation changes trust, not the binding's backing storage.
        let tgt_key = format!("{}_{}", target.name, target.version);
        vars.insert(tgt_key, src_slot);
        false
    }

    /// Retag an attest's subject (scalar, array or map) clean once its
    /// predicate has passed — see `SsaInst::AttestRetag`.
    ///
    /// The retag is a **compile-time** relabelling: it marks the subject
    /// clean for the taint pass and changes nothing at runtime. So the target
    /// version must *alias* the source slot — the same Cranelift variables,
    /// not copies of them.
    ///
    /// It used to `declare_var`/`def_var` a fresh pair. For a caught
    /// `if attest … in a` those definitions were emitted **only in the pass
    /// block**. `find_array_slot` resolves a collection by *name*, scanning
    /// `vars.keys()` for any `a_*`, so on the failing path of a caught
    /// `if attest … in a` it could pick the pass-block-only `a_2` and hand
    /// `sash_table_len` an undefined pointer — a SIGSEGV from safe source.
    ///
    /// Because that scan walks a `HashMap`, which key it found varied *per
    /// compile*: one source produced two different binaries, one of which
    /// faulted. Aliasing makes every version of the name the same entity, so
    /// the choice cannot matter and the output is reproducible again.
    ///
    /// The c backend has always treated this as a no-op ("versions collapse to
    /// one slot", `emit_c.rs`), which is why the differential oracle showed the
    /// fault only in the native backend.
    fn emit_attest_retag(&self, source: &VarRef, target: &VarRef, ctx: &mut InstCtx) -> bool {
        let slot = super::types::var_slot(ctx.vars, &source.display());
        ctx.vars.insert(target.display(), slot);
        false
    }

    /// DD#61: `attest x in a [[…]]` / `attest k v in m [[…]]` — validate every
    /// element of the array/map against the per-element `test`. Iterates the
    /// backing table, binding the element value (and key, for maps) to fresh
    /// codegen slots so the predicate can read them, and evaluates `test`.
    ///
    /// Statement form (`if_form == false`): a failing element writes the
    /// attest error and exits 1; reaching the end sets the test-result var to
    /// true. If form: the first failing element leaves false in the
    /// test-result var for the surrounding branch, and no later element's
    /// predicate runs. An empty collection is vacuously true.
    #[allow(clippy::too_many_arguments)]
    fn emit_attest_all(
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
        let exit_status_var = ctx.exit_status_var;
        let test_result_var = ctx.test_result_var;

        // Backing table pointer (tables live in `_0` slots).
        let table_ptr = {
            let sv = find_array_slot(ctx.vars, &source.name, "attest-all array");
            sv.str_ptr().load(builder)
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
        let (binder_keys, binder_ptrs, binder_lens) = declare_attest_binders(binders, ctx, builder);

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
        }
        let value_binder = usize::from(is_map);
        let (vp, vl) = emit_table_lookup(externs.table_value, table_ptr, ci, builder, module);
        builder.def_var(binder_ptrs[value_binder], vp);
        builder.def_var(binder_lens[value_binder], vl);
        let result = lower_test_expr(
            test,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            externs,
            exit_status_var,
        );

        let cont = builder.create_block();
        let fail = builder.create_block();
        builder.ins().brif(result, cont, &[], fail, &[]);
        builder.switch_to_block(fail);
        builder.seal_block(fail);
        if if_form {
            // DD#61: the first failing cell decides the ∀, as it does for the
            // statement form; a later cell's predicate must not run (it can
            // abort: `${y:?}`).
            let failed = builder.ins().iconst(I64, 0);
            builder.def_var(all_passed, failed);
            builder.ins().jump(after, &[]);
        } else {
            // "attest: NAME failed the gate" → stderr, exit(1)
            self.emit_attest_fail(
                ctx.program.source_names.original(&source.name),
                builder,
                module,
                ctx.literals,
                externs,
            );
        }
        // cont: counter++ ; loop
        builder.switch_to_block(cont);
        builder.seal_block(cont);
        let cnext = builder.ins().iadd_imm(ci, 1);
        builder.def_var(counter, cnext);
        builder.ins().jump(header, &[]);

        // header's predecessors (entry + back-edge) are now complete.
        builder.seal_block(header);

        builder.switch_to_block(after);
        builder.seal_block(after);
        let passed = builder.use_var(all_passed);
        builder.def_var(test_result_var, passed);

        for key in binder_keys {
            ctx.vars.remove(&key);
        }
        false
    }

    /// Emit the attest failure path: `<argv0>: attest: <name> failed the gate`
    /// to stderr, then exit(1). Shared by the statement-form `AttestAll`
    /// (no single scalar value — the value clause is omitted). Scalar
    /// `Attest` emits its own call inline (it also passes the value).
    fn emit_attest_fail(
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
        let attest_fail_ref = module.declare_func_in_func(externs.attest_fail, builder.func);
        builder
            .ins()
            .call(attest_fail_ref, &[name_ptr, name_len, zero, zero]);
        builder
            .ins()
            .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
    }

    /// DD#126: the `- wait` statement. Reaps a child for the effect and drops
    /// the pid; the status lands in `$?` exactly as it does for the value
    /// spellings, because both go through `emit_wait_value`. A *bare* `wait`
    /// never reaches here — SB4016 rejects it.
    fn emit_wait(
        &self,
        args: &[SsaValue],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let mut argv = vec![SsaValue::Literal("wait".to_string())];
        argv.extend(args.iter().cloned());
        crate::codegen::value_handlers::emit_wait_value(
            &argv,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            ctx.externs,
            ctx.exit_status_var,
        );
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
        let sv = find_array_slot(vars, array, "array");
        let table_ptr = sv.str_ptr().load(builder);

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
            externs,
            exit_status_var,
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
        redirects: &[crate::ir::Redirect],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        // Set up redirects (e.g. herestring <<< "text"). DD#55: a failed
        // open aborts rather than binding an empty line.
        let (target, fd) = (target.clone(), *fd);
        let in_condition = ctx.in_condition;
        super::inst_handlers::emit_redirect_guarded_body(
            redirects,
            in_condition,
            builder,
            module,
            ctx,
            |builder, module, ctx| emit_readline_body(&target, fd, builder, module, ctx),
        );
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
        let exit_status_var = ctx.exit_status_var;
        let test_result_var = ctx.test_result_var;
        // let builtin / loop bound: exit status 0 if value != 0, 1 if == 0.
        // The arg is any integer-valued SsaValue (a `(( ))` result, or a
        // for-loop bound comparison `__ai < ${arr arrlen}`).
        let [arg] = BuiltinCmd::LetExit.expect_ssa_args(args);
        let int_val = super::values::lower_value_as_int(
            arg,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        );
        let zero = builder.ins().iconst(I64, 0);
        let is_zero = builder.ins().icmp(IntCC::Equal, int_val, zero);
        let one = builder.ins().iconst(I64, 1);
        let es = builder.ins().select(is_zero, one, zero);
        builder.def_var(exit_status_var, es);
        // Also set test_result_var for branch terminators (if/while/for).
        let tr = builder.ins().select(is_zero, zero, one);
        builder.def_var(test_result_var, tr);
        false
    }

    /// `$!` after `cmd &`: the [`Terminator::Fork`] left the child's pid in
    /// `test_result`, and `$!` takes it either way. A negative pid (`-1`) is a
    /// fork that never started the job: `$!` reads `-1` rather than naming an
    /// earlier job (DD#126: a knowable `-1` over a stale answer), and the
    /// statement fails the way a pipeline's fork does (`guard_pipeline_setup`):
    /// status 1, and where checked the DD#55 diagnostic and abort. A started
    /// job leaves `$?` as it was.
    fn emit_store_bg_pid(
        &self,
        args: &[SsaValue],
        checked: bool,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let kind = BuiltinCmd::StoreBgPid;
        let [target] = kind.expect_ssa_args(args);
        let vr = kind.expect_var_arg(target);
        let pid = builder.use_var(ctx.test_result_var);
        let (str_ptr, str_len) = emit_int_to_str(builder, module, ctx.externs, pid);
        let sv = super::types::var_slot(ctx.vars, &vr.display());
        sv.str_ptr().store(builder, str_ptr);
        sv.str_len().store(builder, str_len);
        let failed = builder.ins().icmp_imm(IntCC::SignedLessThan, pid, 0);
        let failure = builder.create_block();
        let done = builder.create_block();
        builder.ins().brif(failed, failure, &[], done, &[]);

        builder.switch_to_block(failure);
        builder.seal_block(failure);
        let one = builder.ins().iconst(I64, 1);
        let zero = builder.ins().iconst(I64, 0);
        builder.def_var(ctx.exit_status_var, one);
        builder.def_var(ctx.test_result_var, zero);
        let (seg1_ptr, seg1_len) =
            super::inst_handlers::diag_static_seg("fork", ctx.literals, module, builder);
        let (seg2_ptr, seg2_len) = super::inst_handlers::diag_empty_seg(builder);
        super::inst_handlers::emit_status_abort_check(
            builder,
            module,
            ctx.externs,
            ctx.exit_status_var,
            ctx.end_cl_block,
            checked,
            super::inst_handlers::AbortMsg::Errno {
                seg1_ptr,
                seg1_len,
                seg2_ptr,
                seg2_len,
            },
        );
        builder.ins().jump(done, &[]);

        builder.switch_to_block(done);
        builder.seal_block(done);
        false
    }

    fn emit_table_delete(
        &self,
        args: &[SsaValue],
        kind: BuiltinCmd,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let literals = ctx.literals;
        let externs = ctx.externs;
        let exit_status_var = ctx.exit_status_var;
        // unset map[key] → sash_table_delete(table_ptr, key_ptr, key_len)
        let [table, key] = kind.expect_ssa_args(args);
        kind.expect_var_arg(table);
        let (tbl_ptr, _) = lower_value(
            table,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        );
        let (key_ptr, key_len) = if matches!(kind, BuiltinCmd::TableDeleteIdx) {
            let idx_int = super::values::lower_value_as_int(
                key,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (idx_slot, kp) = super::values::alloc_out_slot(builder);
            builder.ins().stack_store(idx_int, idx_slot, 0);
            let kl = builder.ins().iconst(I64, 8);
            (kp, kl)
        } else {
            lower_value(
                key,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            )
        };
        let del_ref = module.declare_func_in_func(externs.table_delete, builder.func);
        builder.ins().call(del_ref, &[tbl_ptr, key_ptr, key_len]);
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
        let saved_end_status_var = ctx.saved_end_status_var;
        // exit CODE — evaluate the argument as an integer. DD#89: ANY int
        // value / variable / return is a valid `exit` argument (not just a
        // literal). An out-of-range value is carried as a full i64 (readable in
        // the END block via `$?`) and truncated to 8 bits only at the final
        // `exit_group`. No argument → 0.
        let code_val = match args.first() {
            Some(arg) => super::values::lower_value_as_int(
                arg,
                builder,
                module,
                ctx.vars,
                ctx.literals,
                externs,
                exit_status_var,
            ),
            None => builder.ins().iconst(I64, 0),
        };
        // DD#56: from the main body, route `exit` to the END block carrying
        // the code. Inside END, with no END, or in a forked child (whose
        // `end_cl_block` is None: END runs only in the main process) exit
        // directly.
        let route_to_end = ctx.end_cl_block.filter(|_| !ctx.in_end_block);
        if let Some(end_cl) = route_to_end {
            builder.def_var(exit_status_var, code_val);
            builder.ins().jump(end_cl, &[]);
            return true;
        }
        // Direct exit. Inside END this also updates saved_end_status_var so
        // Return uses it.
        builder.def_var(saved_end_status_var, code_val);
        let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
        builder.ins().call(exit_ref, &[code_val]);
        builder
            .ins()
            .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
        true // block is terminated
    }

    /// DD#99 `map m = @{a:DISP}`: fill the map from the array, pairwise. The
    /// odd-count disposition is `mode`; `:?` returns 1 and we abort with the
    /// supplied message (DD#55), like every other runtime disposition failure.
    #[allow(clippy::too_many_arguments)]
    fn emit_map_from_array(
        &self,
        source: &str,
        target: &str,
        mode: u8,
        key: &SsaValue,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let (key_ptr, key_len) = super::values::lower_value(
            key,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            ctx.externs,
            ctx.exit_status_var,
        );
        let src = find_array_slot(ctx.vars, source, "array");
        let src_ptr = src.str_ptr().load(builder);
        let dst = find_array_slot(ctx.vars, target, "map");
        let dst_ptr = dst.str_ptr().load(builder);
        let mode_val = builder.ins().iconst(I64, i64::from(mode));
        let fref = module.declare_func_in_func(ctx.externs.map_from_array, builder.func);
        builder
            .ins()
            .call(fref, &[src_ptr, dst_ptr, mode_val, key_ptr, key_len]);
        false
    }

    /// DD#99 `read $SRC ARRAY`: split a scalar value on NUL into the array.
    /// Same call as the fd form, with the bytes coming from `lower_value`
    /// rather than `sash_read_all`. Terminator semantics (a trailing NUL does
    /// not add an empty final element), matching `read` on a stream.
    fn emit_split_value(
        &self,
        source: &SsaValue,
        array: &str,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let (ptr, len) = super::values::lower_value(
            source,
            builder,
            module,
            ctx.vars,
            ctx.literals,
            ctx.externs,
            ctx.exit_status_var,
        );
        let sv = find_array_slot(ctx.vars, array, "array");
        let table_ptr = sv.str_ptr().load(builder);
        let split_ref = module.declare_func_in_func(ctx.externs.split_into_array, builder.func);
        let nul = builder.ins().iconst(I64, 0);
        let terminator = builder.ins().iconst(I64, 1);
        builder
            .ins()
            .call(split_ref, &[ptr, len, nul, terminator, table_ptr]);
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_read_array(
        &self,
        array: &str,
        fd: &i64,
        redirects: &[crate::ir::Redirect],
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        // DD#55: a failed `read a < nosuch` open must abort rather than
        // yield an empty array, exactly as `readline v < nosuch` does.
        let (array, fd) = (array.to_string(), *fd);
        let in_condition = ctx.in_condition;
        super::inst_handlers::emit_redirect_guarded_body(
            redirects,
            in_condition,
            builder,
            module,
            ctx,
            |builder, module, ctx| emit_read_array_body(&array, fd, builder, module, ctx),
        );
        false
    }

    fn emit_capture_end(
        &self,
        site: u32,
        buffered: bool,
        target: &VarRef,
        builder: &mut FunctionBuilder,
        module: &mut ObjectModule,
        ctx: &mut InstCtx,
    ) -> bool {
        let vars = &mut *ctx.vars;
        let externs = ctx.externs;
        // Both paths yield (ptr, len) with trailing newlines already stripped,
        // so only the call differs — everything downstream is shared.
        let call = if buffered {
            let cap_ref = module.declare_func_in_func(externs.capture_end_buf, builder.func);
            builder.ins().call(cap_ref, &[])
        } else {
            let (saved_var, handle_var) = ctx.capture_slots[site as usize];
            let saved = builder.use_var(saved_var);
            let handle = builder.use_var(handle_var);
            let cap_ref = module.declare_func_in_func(externs.capture_end, builder.func);
            builder.ins().call(cap_ref, &[saved, handle])
        };
        let ptr = builder.inst_results(call)[0];
        let len = builder.inst_results(call)[1];
        let sv = vars[&target.display()];
        // DD#58 Option C: see the note in `emit_c::emit_capture_end_c`. A
        // capture is the stdout half of `$(…)`, so its target is always a
        // string; a `-> int` function's return takes the `__intret_` path.
        // `str_ptr`/`str_len` are the shared guard for exactly this class of
        // caller — their INVARIANT note already lists capture targets.
        sv.str_ptr().store(builder, ptr);
        sv.str_len().store(builder, len);
        false
    }
}

/// Finish successful standalone fd manipulation: the guard already applied
/// the redirects. Rebind diagnostics only when exec actually changed the fds.
fn emit_exec_redirects_only(
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    if !redirects.is_empty() {
        // DD#120: this is the ONE redirect form that rebinds the invoker's own
        // stderr, so the diagnostic channel follows it. A per-command redirect
        // (`cmd 2>…`) deliberately does not — it is a channel for the command's
        // output, not for the program's diagnostics about it.
        let rebind = module.declare_func_in_func(ctx.externs.scope_bind, builder.func);
        builder.ins().call(rebind, &[]);
    }
    let zero = builder.ins().iconst(I64, 0);
    builder.def_var(ctx.exit_status_var, zero);
    let one = builder.ins().iconst(I64, 1);
    builder.def_var(ctx.test_result_var, one);
}

/// `exec CMD ARGS…` — `args[0]` is the command, exactly the shape
/// [`build_argv_values`] takes.
///
/// This goes through the SHARED argv builder on purpose. It used to have its
/// own stack-slot copy, which had drifted into a strictly worse version of
/// `build_argv_fixed`: it lowered every argument as a scalar, so `@a` and
/// `${x:~}` reached `lower_value` and panicked, and it never called
/// `emit_argv_guard`, so `exec cmd "$s"` with an interior NUL in `s`
/// truncated silently at execve and exited 0 — the precise outcome DD#99
/// stage 7 exists to prevent. Three faults, one cause: a second
/// implementation of a thing that already existed.
fn emit_exec_static_argv(
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let argv_addr = super::commands::build_argv_values(
        args,
        builder,
        module,
        ctx.vars,
        ctx.literals,
        ctx.externs,
        ctx.exit_status_var,
    );
    finalize_exec(argv_addr, builder, module, ctx.externs);
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
/// DD#61: declare per-binder `(ptr,len)` slots for a attest-all predicate and
/// register them in `ctx.vars` under the binders' globally-unique SSA keys
/// (so the pre-lowered predicate operands resolve to exactly these slots).
/// Returns `(keys, ptrs, lens)` for the caller to bind and later remove.
#[allow(clippy::type_complexity)]
fn declare_attest_binders(
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
        ctx.vars.insert(
            key.clone(),
            VarSlot::Str {
                ptr: bp.into(),
                len: bl.into(),
            },
        );
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
        ctx.exit_status_var,
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
        msg,
    );
}

/// The `read ARR` body proper: slurp the fd and split into the array. Split
/// out of `emit_read_array` so it can be handed to the DD#55 redirect guard
/// as a closure.
fn emit_read_array_body(
    array: &str,
    fd: i64,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let externs = ctx.externs;
    let sv = find_array_slot(&*ctx.vars, array, "array");
    let table_ptr = sv.str_ptr().load(builder);

    let fd_val = super::inst_handlers::read_fd(fd, builder, ctx);

    let read_all_ref = module.declare_func_in_func(externs.read_all, builder.func);
    let call = builder.ins().call(read_all_ref, &[fd_val]);
    let data_ptr = builder.inst_results(call)[0];
    let data_len = builder.inst_results(call)[1];
    let nonempty = builder
        .ins()
        .icmp_imm(IntCC::SignedGreaterThan, data_len, 0);
    let result = builder.ins().uextend(I64, nonempty);
    builder.def_var(ctx.test_result_var, result);

    // DD#99: NUL is the only separator, with terminator semantics (a
    // trailing empty field is dropped).
    let split_ref = module.declare_func_in_func(externs.split_into_array, builder.func);
    let delim_val = builder.ins().iconst(I64, 0);
    let terminator = builder.ins().iconst(I64, 1);
    builder.ins().call(
        split_ref,
        &[data_ptr, data_len, delim_val, terminator, table_ptr],
    );
}

/// The `readline VAR` body proper. Split out of `emit_readline` so it can be
/// handed to the DD#55 redirect guard as a closure.
fn emit_readline_body(
    target: &VarRef,
    fd: i64,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) {
    let externs = ctx.externs;
    let fd_val = super::inst_handlers::read_fd(fd, builder, ctx);
    let read_line_ref = module.declare_func_in_func(externs.read_line, builder.func);
    let call = builder.ins().call(read_line_ref, &[fd_val]);
    let (ptr, len) = (builder.inst_results(call)[0], builder.inst_results(call)[1]);
    let (empty, zero) = super::values::lower_literal_value("", builder, module, ctx.literals);
    let got_data = builder.ins().icmp(IntCC::NotEqual, ptr, zero);
    // Keep EOF in the verdict; a stored string must not use the null integer-cell tag.
    let string_ptr = builder.ins().select(got_data, ptr, empty);
    let sv = &ctx.vars[&target.display()];
    sv.str_ptr().store(builder, string_ptr);
    sv.str_len().store(builder, len);
    let result = builder.ins().uextend(I64, got_data);
    builder.def_var(ctx.test_result_var, result);
}

#[cfg(test)]
mod invariant_tests {
    use super::*;
    use crate::codegen::entry::{compile_ssa, Embedded};

    #[test]
    fn unlowered_builtins_cannot_fall_through_to_external_commands() {
        let groups: &[(&[&str], &str)] = &[
            (&["write"], "rewritten before codegen"),
            (
                &[
                    "len", "strlen", "keys", "join", "sub", "gsub", "matchall", "toupper",
                    "tolower", "basename", "dirname", "argv",
                ],
                "expression intrinsic resolved in transform",
            ),
            (
                &[
                    "[[",
                    "attest",
                    "read",
                    "readline",
                    "mapfile",
                    "readarray",
                    "break",
                    "continue",
                    "return",
                    "signal",
                    "unset",
                ],
                "lowered to its own instruction",
            ),
        ];
        for (names, expected) in groups {
            for name in *names {
                let message = native_ice(vec![SsaInst::command(*name, vec![], vec![], false)]);
                assert!(message.contains(expected), "{name}: {message}");
            }
        }
    }

    fn native_ice(insts: Vec<SsaInst>) -> String {
        let mut ssa = SsaProgram::new();
        let entry = ssa.new_block("entry");
        ssa.block_mut(entry).insts = insts;
        let payload = std::panic::catch_unwind(|| {
            compile_ssa(
                &ssa,
                Embedded {
                    source: "",
                    ir: "",
                    manifest: "",
                },
                false,
            )
        })
        .expect_err("malformed SSA must not compile");
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
            .expect("the invariant reports its claim")
    }

    fn internal_ice(kind: BuiltinCmd, args: Vec<SsaValue>) -> String {
        native_ice(vec![SsaInst::internal(kind, args, true)])
    }

    #[test]
    fn an_unlowered_test_command_must_not_become_true() {
        let message = native_ice(vec![SsaInst::command("[[", vec![], vec![], true)]);
        assert!(message.contains("Test"), "{message}");
        assert!(
            message.contains("lowered to its own instruction"),
            "{message}"
        );
    }

    #[test]
    fn an_empty_redirect_scope_must_not_silently_succeed() {
        let message = native_ice(vec![SsaInst::RedirectPush {
            site: 0,
            redirects: vec![],
            checked: true,
        }]);
        assert!(
            message.contains("RedirectPush requires at least one redirect"),
            "{message}"
        );
    }

    #[test]
    fn non_scalar_redirect_targets_are_loud_invariant_failures() {
        for (kind, expected) in [
            (crate::ir::ExprKind::Array(vec![]), "Array"),
            (crate::ir::ExprKind::AssocArray(vec![]), "AssocArray"),
        ] {
            let message = native_ice(vec![SsaInst::command(
                "echo",
                vec![],
                vec![crate::ir::Redirect {
                    fd: None,
                    op: crate::ir::RedirectOp::Write,
                    target: crate::ir::Expr {
                        kind,
                        span: crate::ir::Span::default(),
                    },
                }],
                true,
            )]);
            assert!(
                message.contains(&format!("unexpected non-scalar target kind {expected}")),
                "{message}"
            );
        }
    }

    #[test]
    fn internal_command_arity_is_exact() {
        for (kind, count) in [
            (BuiltinCmd::TrapSignal, 2),
            (BuiltinCmd::SignalFlag, 2),
            (BuiltinCmd::SignalCheckpoint, 0),
            (BuiltinCmd::SetTestFromPipe, 1),
            (BuiltinCmd::LetExit, 1),
            (BuiltinCmd::StoreBgPid, 1),
            (BuiltinCmd::TableDelete, 2),
            (BuiltinCmd::TableDeleteIdx, 2),
        ] {
            for len in [0, count + 1].into_iter().filter(|&len| len != count) {
                let message = internal_ice(kind, vec![SsaValue::Literal("1".into()); len]);
                assert!(
                    message.contains(&format!("{kind:?} needs exactly {count} SSA arguments")),
                    "{message}"
                );
            }
        }
    }

    #[test]
    fn match_cannot_lose_a_missing_pattern_or_an_extra_out_array() {
        for len in [0, 1, 3] {
            let message = native_ice(vec![SsaInst::command(
                "match",
                vec![SsaValue::Literal("x".into()); len],
                vec![],
                true,
            )]);
            assert!(
                message.contains("Match needs exactly 2 SSA arguments"),
                "{message}"
            );
        }
    }

    #[test]
    fn signal_operands_must_remain_integer_literals() {
        for bad in [
            SsaValue::Var(VarRef::new("dynamic", 0)),
            SsaValue::Literal("not-an-integer".into()),
            SsaValue::Literal("9223372036854775808".into()),
        ] {
            for (kind, args) in [
                (
                    BuiltinCmd::SignalFlag,
                    vec![bad.clone(), SsaValue::Literal("0".into())],
                ),
                (
                    BuiltinCmd::SignalFlag,
                    vec![SsaValue::Literal("15".into()), bad.clone()],
                ),
                (
                    BuiltinCmd::TrapSignal,
                    vec![bad.clone(), SsaValue::Literal("1".into())],
                ),
                (
                    BuiltinCmd::TrapSignal,
                    vec![SsaValue::Literal("15".into()), bad.clone()],
                ),
            ] {
                let message = internal_ice(kind, args);
                assert!(message.contains(&format!("{kind:?}")), "{message}");
                assert!(message.contains("integer literal"), "{message}");
            }
        }
    }

    #[test]
    fn background_pid_storage_needs_a_variable_target() {
        let message = internal_ice(BuiltinCmd::StoreBgPid, vec![SsaValue::Literal("1".into())]);
        assert!(
            message.contains("StoreBgPid needs a variable operand"),
            "{message}"
        );
    }

    #[test]
    fn signal_registration_needs_a_flag_or_exit_bit() {
        let message = internal_ice(
            BuiltinCmd::SignalFlag,
            vec![
                SsaValue::Literal("15".into()),
                SsaValue::Literal("2".into()),
            ],
        );
        assert!(message.contains("EXIT bit must be 0 or 1"), "{message}");
    }

    #[test]
    fn close_needs_a_literal_os_descriptor_even_when_unchecked() {
        for count in [0, 2] {
            let message = native_ice(vec![SsaInst::command(
                "close",
                vec![SsaValue::Literal("1".into()); count],
                vec![],
                false,
            )]);
            assert!(
                message.contains("Close needs exactly 1 SSA arguments"),
                "{message}"
            );
        }
        for (arg, claim) in [
            (
                SsaValue::Var(VarRef::new("dynamic", 0)),
                "literal file descriptor",
            ),
            (SsaValue::Literal("bad".into()), "non-integer literal"),
            (SsaValue::Literal("-1".into()), "outside 0..2147483647"),
            (
                SsaValue::Literal("2147483648".into()),
                "outside 0..2147483647",
            ),
        ] {
            for checked in [false, true] {
                let message = native_ice(vec![SsaInst::command(
                    "close",
                    vec![arg.clone()],
                    vec![],
                    checked,
                )]);
                assert!(message.contains(claim), "{message}");
            }
        }
    }

    #[test]
    fn a_table_delete_needs_a_variable_table() {
        for kind in [BuiltinCmd::TableDelete, BuiltinCmd::TableDeleteIdx] {
            let message = internal_ice(
                kind,
                vec![
                    SsaValue::Literal("table".into()),
                    SsaValue::Literal("0".into()),
                ],
            );
            assert!(
                message.contains(&format!("{kind:?} needs a variable operand")),
                "{message}"
            );
        }
    }

    #[test]
    fn retag_cannot_silently_skip_an_unmapped_source() {
        let message = native_ice(vec![SsaInst::AttestRetag {
            source: VarRef::new("missing", 0),
            target: VarRef::new("missing", 1),
        }]);
        assert!(
            message.contains("no codegen slot for `missing_0`"),
            "{message}"
        );
        assert!(message.contains("map_inst_versioned"), "{message}");
    }

    #[test]
    fn retag_cannot_substitute_a_different_source_version() {
        let message = native_ice(vec![
            SsaInst::Assign {
                target: VarRef::new("items", 0),
                value: SsaValue::IndexedArray(vec![]),
            },
            SsaInst::AttestRetag {
                source: VarRef::new("items", 7),
                target: VarRef::new("items", 8),
            },
        ]);
        assert!(
            message.contains("no codegen slot for `items_7`"),
            "{message}"
        );
    }
}
