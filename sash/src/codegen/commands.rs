#![allow(clippy::doc_markdown, clippy::doc_lazy_continuation)]
#![allow(clippy::too_many_arguments)]
//! Builtin command code generation: echo, exec.
//!
//! TRUST: lower_exec is the path to fork/exec. Command names are SSA
//! literals resolved at compile time. No shell interpretation.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::MemFlags;
use cranelift_codegen::ir::{instructions::BlockArg, InstBuilder, StackSlotData, StackSlotKind};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::types::{ExternFuncs, VarSlot};
use super::values::lower_value;
use crate::ssa::*;

/// Emit a single `sash_write(fd, ptr, len)` and fold a write failure
/// (`result < 0`, e.g. ENOSPC/EPIPE) into the running `failed` flag.
/// Returns the updated `failed` (0 = all writes OK so far, 1 = some
/// write failed). DD#55 (A): echo's write verdict must be observable.
fn echo_write_tracked(
    builder: &mut FunctionBuilder,
    write_ref: cranelift_codegen::ir::FuncRef,
    fd: cranelift_codegen::ir::Value,
    ptr: cranelift_codegen::ir::Value,
    len: cranelift_codegen::ir::Value,
    failed: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let call = builder.ins().call(write_ref, &[fd, ptr, len]);
    let result = builder.inst_results(call)[0];
    let zero = builder.ins().iconst(I64, 0);
    let this_failed = builder.ins().icmp(IntCC::SignedLessThan, result, zero);
    let this_failed = builder.ins().uextend(I64, this_failed);
    builder.ins().bor(failed, this_failed)
}

/// Lower `echo`. Returns the write-failure status (0 = OK, 1 = some
/// `write(2)` failed) so the caller can set `$?`/test_result and apply
/// the DD#55 (A) abort.
/// Parse the optional leading `-n` flag. Returns (args_to_print,
/// suppress_newline).
fn echo_parse_flags(args: &[SsaValue]) -> (&[SsaValue], bool) {
    if let Some(SsaValue::Literal(flag)) = args.first() {
        if flag == "-n" {
            return (&args[1..], true);
        }
    }
    (args, false)
}

#[derive(Clone, Copy)]
struct EchoState {
    failed: cranelift_codegen::ir::Value,
    has_arg: cranelift_codegen::ir::Value,
}

fn echo_write_separator(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    write_ref: cranelift_codegen::ir::FuncRef,
    fd: cranelift_codegen::ir::Value,
    space_id: cranelift_module::DataId,
    state: EchoState,
) -> cranelift_codegen::ir::Value {
    let write_blk = builder.create_block();
    let cont_blk = builder.create_block();
    builder.append_block_param(cont_blk, I64);
    let has_arg = builder.ins().icmp_imm(IntCC::NotEqual, state.has_arg, 0);
    builder.ins().brif(
        has_arg,
        write_blk,
        &[],
        cont_blk,
        &[BlockArg::Value(state.failed)],
    );
    builder.switch_to_block(write_blk);
    builder.seal_block(write_blk);
    let sp_gv = module.declare_data_in_func(space_id, builder.func);
    let sp_ptr = builder.ins().global_value(I64, sp_gv);
    let sp_len = builder.ins().iconst(I64, 1);
    let failed = echo_write_tracked(builder, write_ref, fd, sp_ptr, sp_len, state.failed);
    builder.ins().jump(cont_blk, &[BlockArg::Value(failed)]);
    builder.switch_to_block(cont_blk);
    builder.seal_block(cont_blk);
    builder.block_params(cont_blk)[0]
}

/// Real empty arguments still count; only an empty elision leaves `has_arg`
/// unchanged. Concat parts remain raw writes, including interior NUL bytes.
#[allow(clippy::too_many_arguments)]
fn echo_write_one_arg(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    write_ref: cranelift_codegen::ir::FuncRef,
    fd: cranelift_codegen::ir::Value,
    arg: &SsaValue,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    space_id: cranelift_module::DataId,
    exit_status_var: Variable,
    state: EchoState,
) -> EchoState {
    if let SsaValue::ElideEmpty { source, .. } = arg {
        return echo_write_elide_arg(
            builder,
            module,
            write_ref,
            fd,
            source,
            vars,
            literals,
            externs,
            space_id,
            exit_status_var,
            state,
        );
    }
    let mut failed = echo_write_separator(builder, module, write_ref, fd, space_id, state);
    let parts: &[SsaValue] = match arg {
        SsaValue::Concat(parts) => parts,
        other => std::slice::from_ref(other),
    };
    for part in parts {
        let (ptr, len) = lower_value(
            part,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        );
        failed = echo_write_tracked(builder, write_ref, fd, ptr, len, failed);
    }
    EchoState {
        failed,
        has_arg: builder.ins().iconst(I64, 1),
    }
}

/// DD#84/#91: an empty `:~` / `:!~` contributes neither a value nor a separator.
/// Both edges retain previous write failures and runtime argument presence.
#[allow(clippy::too_many_arguments)]
fn echo_write_elide_arg(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    write_ref: cranelift_codegen::ir::FuncRef,
    fd: cranelift_codegen::ir::Value,
    source: &SsaValue,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    space_id: cranelift_module::DataId,
    exit_status_var: Variable,
    state: EchoState,
) -> EchoState {
    let (ptr, len) = lower_value(
        source,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let write_blk = builder.create_block();
    let cont_blk = builder.create_block();
    builder.append_block_param(cont_blk, I64);
    builder.append_block_param(cont_blk, I64);
    let is_empty = builder.ins().icmp_imm(IntCC::Equal, len, 0);
    builder.ins().brif(
        is_empty,
        cont_blk,
        &[
            BlockArg::Value(state.failed),
            BlockArg::Value(state.has_arg),
        ],
        write_blk,
        &[],
    );

    builder.switch_to_block(write_blk);
    builder.seal_block(write_blk);
    let failed = echo_write_separator(builder, module, write_ref, fd, space_id, state);
    let failed = echo_write_tracked(builder, write_ref, fd, ptr, len, failed);
    let has_arg = builder.ins().iconst(I64, 1);
    builder.ins().jump(
        cont_blk,
        &[BlockArg::Value(failed), BlockArg::Value(has_arg)],
    );

    builder.switch_to_block(cont_blk);
    builder.seal_block(cont_blk);
    EchoState {
        failed: builder.block_params(cont_blk)[0],
        has_arg: builder.block_params(cont_blk)[1],
    }
}

/// Lower `echo`. Returns the write-failure status (0 = OK, 1 = some
/// `write(2)` failed) so the caller can set `$?`/test_result and apply
/// the DD#55 (A) abort.
pub(super) fn lower_echo(
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    space_id: cranelift_module::DataId,
    nl_id: cranelift_module::DataId,
    exit_status_var: Variable,
) -> cranelift_codegen::ir::Value {
    let fd = builder.ins().iconst(I64, 1);
    let write_ref = module.declare_func_in_func(externs.write, builder.func);
    let zero = builder.ins().iconst(I64, 0);
    let mut state = EchoState {
        failed: zero,
        has_arg: zero,
    };
    let (print_args, suppress_newline) = echo_parse_flags(args);

    for arg in print_args {
        state = echo_write_one_arg(
            builder,
            module,
            write_ref,
            fd,
            arg,
            vars,
            literals,
            externs,
            space_id,
            exit_status_var,
            state,
        );
    }

    if !suppress_newline {
        let nl_gv = module.declare_data_in_func(nl_id, builder.func);
        let nl_ptr = builder.ins().global_value(I64, nl_gv);
        let nl_len = builder.ins().iconst(I64, 1);
        state.failed = echo_write_tracked(builder, write_ref, fd, nl_ptr, nl_len, state.failed);
    }
    state.failed
}

/// Emit fork/exec for an external command.
///
/// TRUST: The command name comes from SSA, which was:
/// 1. Validated by command_resolution (SB5011) — must be absolute path or resolved
/// 2. Resolved at compile time by resolve_pipeline_command (DD#23)
/// No runtime PATH lookup occurs. The binary literally contains "/usr/bin/grep".
///
/// TRUST: Arguments are SSA values — literals, variable reads, or
/// concatenations thereof. No shell interpretation (no word splitting,
/// no globbing, no metacharacter expansion).
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_exec(
    name: &str,
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    test_result_var: Variable,
    exit_status_var: Variable,
) {
    let argv_addr = build_exec_argv(
        name,
        args,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );

    let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
    let envp_addr = builder.ins().global_value(I64, envp_gv);
    let envp = builder.ins().load(I64, MemFlags::new(), envp_addr, 0);

    let run_ref = module.declare_func_in_func(externs.run, builder.func);
    let call = builder.ins().call(run_ref, &[argv_addr, envp]);
    let exit_status = builder.inst_results(call)[0];
    // Set test_result_var: 1 if exit status == 0 (success), 0 otherwise
    let zero = builder.ins().iconst(I64, 0);
    let ok = builder.ins().icmp(IntCC::Equal, exit_status, zero);
    let result = builder.ins().uextend(I64, ok);
    builder.def_var(test_result_var, result);
    builder.def_var(exit_status_var, exit_status);
}

/// Build a NUL-terminated argv `[name, args…, NULL]` and return its base
/// address (a `*const *const u8` as an `I64` Value), ready to hand to
/// `sash_run` / `sash_exec`.
///
/// Fast path — no [`SsaValue::ArraySplat`] among `args`: a fixed-size
/// stack slot sized at compile time (the historical behaviour, byte for
/// byte).
///
/// Splat path (DD#20/DD#22, B1) — at least one `@a`: argc is a *runtime*
/// value `1 + #scalars + Σ sash_table_len(splat)`. We `sash_alloc`
/// `(argc+1)*8` bytes and fill them with Cranelift loops (scalars stored
/// directly; each splat loops `sash_table_value`), then NUL-terminate.
/// An empty array contributes zero entries.
pub(super) fn build_exec_argv(
    name: &str,
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Variable,
) -> cranelift_codegen::ir::Value {
    let mut values = Vec::with_capacity(args.len() + 1);
    values.push(SsaValue::Literal(name.to_string()));
    values.extend(args.iter().cloned());
    build_argv_values(
        &values,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    )
}

/// Build argv from a complete value list (`values[0]` is the command).
/// Used by [`build_exec_argv`] and by command substitution. Picks the
/// fixed (stack) or runtime (heap + splat loops) path automatically.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_argv_values(
    values: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Variable,
) -> cranelift_codegen::ir::Value {
    if values
        .iter()
        .any(|a| matches!(a, SsaValue::ArraySplat { .. } | SsaValue::ElideEmpty { .. }))
    {
        build_argv_runtime(
            values,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        )
    } else {
        build_argv_fixed(
            values,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        )
    }
}

/// `:` evaluates its arguments for their effects (a `:?` abort, a DD#69
/// splat disposition, an arithmetic fault) and discards the values. It used
/// to evaluate none of them, so `: ${s:?empty}` carried on (DD#77). The order
/// is [`build_argv_runtime`]'s, splat dispositions first and then every
/// other value left to right, but no argv is built. Nothing is exec'd, so
/// the DD#99 NUL guard (an exec-boundary check) does not apply, and the
/// bump heap is not charged for a table nobody reads.
#[allow(clippy::too_many_arguments)]
pub(super) fn evaluate_args_for_effects(
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Variable,
) {
    resolve_splat_tables(
        args,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    for arg in args {
        let value = match arg {
            SsaValue::ArraySplat { .. } => continue,
            SsaValue::ElideEmpty { source, .. } => source.as_ref(),
            other => other,
        };
        lower_value(
            value,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        );
    }
}

/// DD#99 stage 7: an argv entry's length is known here but is lost at
/// `execve`, where the entry becomes a C string. Check for an interior NUL
/// while we still can — silent truncation is the one outcome to avoid.
///
/// Splat ELEMENTS are not guarded here: they come from a table the runtime
/// walks, so `emit_splat_fill` would need the check inside its loop.
fn emit_argv_guard(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    ptr: cranelift_codegen::ir::Value,
    len: cranelift_codegen::ir::Value,
) {
    let guard = module.declare_func_in_func(externs.argv_guard, builder.func);
    builder.ins().call(guard, &[ptr, len]);
}

/// Compile-time-sized argv on a stack slot. No splats present.
#[allow(clippy::too_many_arguments)]
fn build_argv_fixed(
    values: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Variable,
) -> cranelift_codegen::ir::Value {
    let argc = values.len();
    let argv_size = ((argc + 1) * 8) as u32;
    let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        argv_size,
        3,
    ));

    for (i, v) in values.iter().enumerate() {
        let offset = (i * 8) as i32;
        let (ptr, len) = lower_value(v, builder, module, vars, literals, externs, exit_status_var);
        // DD#99 stage 7: the length is known here but is lost at execve, where
        // the entry becomes a C string. Check for an interior NUL while we
        // still can — truncating silently is the one outcome to avoid.
        emit_argv_guard(builder, module, externs, ptr, len);
        builder.ins().stack_store(ptr, argv_slot, offset);
    }

    let null = builder.ins().iconst(I64, 0);
    builder
        .ins()
        .stack_store(null, argv_slot, (argc * 8) as i32);
    builder.ins().stack_addr(I64, argv_slot, 0)
}

/// Runtime-sized argv on the heap. At least one `@a` splat present.
#[allow(clippy::too_many_arguments)]
fn build_argv_runtime(
    values: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Variable,
) -> cranelift_codegen::ir::Value {
    // 0) Pre-resolve each `@a` splat to the table to expand (DD#69
    //    disposition applied once, so argc and the fill agree).
    let splat_tables = resolve_splat_tables(
        values,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );

    // 1) argc = #scalars + Σ sash_table_len(effective splat). (cmd is a scalar.)
    let scalar_count = values
        .iter()
        .filter(|a| !matches!(a, SsaValue::ArraySplat { .. }))
        .count();
    let mut argc = builder.ins().iconst(I64, scalar_count as i64);
    for &tbl in &splat_tables {
        let len = emit_table_len_call(tbl, builder, module, externs);
        argc = builder.ins().iadd(argc, len);
    }

    // 2) heap argv of (argc+1)*8 bytes.
    let argc_plus1 = builder.ins().iadd_imm(argc, 1);
    let bytes = builder.ins().imul_imm(argc_plus1, 8);
    let alloc_ref = module.declare_func_in_func(externs.alloc, builder.func);
    let call = builder.ins().call(alloc_ref, &[bytes]);
    let base = builder.inst_results(call)[0];

    // 3) running write index, starting at 0.
    let widx = builder.declare_var(I64);
    let zero = builder.ins().iconst(I64, 0);
    builder.def_var(widx, zero);

    // 4) fill: scalars directly, splats per element (from the pre-resolved
    //    effective tables, in splat order).
    let mut si = 0;
    for v in values {
        match v {
            SsaValue::ArraySplat { .. } => {
                emit_splat_fill(builder, module, externs, base, widx, splat_tables[si]);
                si += 1;
            }
            // DD#84: `${…:~}` (elide) — store the value only when non-empty, so
            // an identity value contributes zero args. argv is over-allocated
            // (each elide counted as 1 above), so a skipped store just leaves a
            // spare slot; the NUL terminates at the real count (widx).
            SsaValue::ElideEmpty { source: inner, .. } => {
                let (ptr, len) = lower_value(
                    inner,
                    builder,
                    module,
                    vars,
                    literals,
                    externs,
                    exit_status_var,
                );
                emit_argv_guard(builder, module, externs, ptr, len);
                store_argv_entry_if_nonempty(builder, base, widx, ptr, len);
            }
            scalar => {
                let (ptr, len) = lower_value(
                    scalar,
                    builder,
                    module,
                    vars,
                    literals,
                    externs,
                    exit_status_var,
                );
                emit_argv_guard(builder, module, externs, ptr, len);
                store_argv_entry(builder, base, widx, ptr);
            }
        }
    }

    // 5) NUL-terminate at argv[argc] (widx now equals argc).
    let cur = builder.use_var(widx);
    let off = builder.ins().imul_imm(cur, 8);
    let addr = builder.ins().iadd(base, off);
    let null = builder.ins().iconst(I64, 0);
    builder.ins().store(MemFlags::new(), null, addr, 0);
    base
}

/// DD#69: pre-resolve each `@a` command-arg splat to the effective table to
/// expand (applying the empty-disposition once, so argc and the fill agree
/// and a `:?` aborts exactly once). One entry per `ArraySplat`, in order.
#[allow(clippy::too_many_arguments)]
fn resolve_splat_tables(
    values: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Variable,
) -> Vec<cranelift_codegen::ir::Value> {
    let mut splat_tables = Vec::new();
    for v in values {
        if let SsaValue::ArraySplat { array, disposition } = v {
            let eff = resolve_splat_table(
                array,
                disposition.as_ref(),
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            splat_tables.push(eff);
        }
    }
    splat_tables
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_splat_table(
    array: &str,
    disposition: Option<&crate::ssa::ArraySplatDisposition>,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Variable,
) -> cranelift_codegen::ir::Value {
    let table = array_table_ptr(array, vars, builder);
    match disposition {
        None => table,
        Some(d) => emit_splat_disposition(
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
            table,
            d,
        ),
    }
}

/// Load an array variable's table pointer by name.
pub(super) fn array_table_ptr(
    array: &str,
    vars: &HashMap<String, VarSlot>,
    builder: &mut FunctionBuilder,
) -> cranelift_codegen::ir::Value {
    let sv = super::types::find_array_slot(vars, array, "array");
    sv.str_ptr().load(builder)
}

/// DD#69: apply an external-splat empty-disposition, returning the table to
/// expand — non-empty array → the array; empty → `sash_splat_disposition`
/// (WORD single arg for `:-`, abort for `:?`, empty for bare `@{a:-}`).
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_splat_disposition(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Variable,
    tbl: cranelift_codegen::ir::Value,
    d: &crate::ssa::ArraySplatDisposition,
) -> cranelift_codegen::ir::Value {
    let (word_ptr, word_len) = if d.has_word {
        lower_value(
            &d.word,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        )
    } else {
        let zero = builder.ins().iconst(I64, 0);
        (zero, zero)
    };
    let has_word = builder.ins().iconst(I64, d.has_word as i64);
    let abort = builder.ins().iconst(I64, d.abort as i64);
    let disp_ref = module.declare_func_in_func(externs.splat_disposition, builder.func);
    let call = builder
        .ins()
        .call(disp_ref, &[tbl, word_ptr, word_len, has_word, abort]);
    builder.inst_results(call)[0]
}

/// `sash_table_len(tbl)` → i64.
fn emit_table_len_call(
    tbl: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
    let call = builder.ins().call(len_ref, &[tbl]);
    builder.inst_results(call)[0]
}

/// Store `ptr` at `argv[widx]` (base + widx*8) and increment `widx`.
fn store_argv_entry(
    builder: &mut FunctionBuilder,
    base: cranelift_codegen::ir::Value,
    widx: Variable,
    ptr: cranelift_codegen::ir::Value,
) {
    let cur = builder.use_var(widx);
    let off = builder.ins().imul_imm(cur, 8);
    let addr = builder.ins().iadd(base, off);
    builder.ins().store(MemFlags::new(), ptr, addr, 0);
    let next = builder.ins().iadd_imm(cur, 1);
    builder.def_var(widx, next);
}

/// DD#84: store `ptr` at `argv[widx++]` only when `len != 0` — an empty
/// (identity) value is elided (contributes zero args). `widx` is a Cranelift
/// `Variable`, so the conditional increment merges at the continuation block.
fn store_argv_entry_if_nonempty(
    builder: &mut FunctionBuilder,
    base: cranelift_codegen::ir::Value,
    widx: Variable,
    ptr: cranelift_codegen::ir::Value,
    len: cranelift_codegen::ir::Value,
) {
    let store_blk = builder.create_block();
    let cont_blk = builder.create_block();
    let is_empty = builder.ins().icmp_imm(IntCC::Equal, len, 0);
    builder.ins().brif(is_empty, cont_blk, &[], store_blk, &[]);

    builder.switch_to_block(store_blk);
    builder.seal_block(store_blk);
    store_argv_entry(builder, base, widx, ptr);
    builder.ins().jump(cont_blk, &[]);

    builder.switch_to_block(cont_blk);
    builder.seal_block(cont_blk);
}

/// Append every element of `tbl` to argv: for i in 0..len { argv[widx++]
/// = sash_table_value(tbl, i) }. Element pointers are NUL-terminated
/// (string invariant ptr[len]==0), so they go straight into argv.
fn emit_splat_fill(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    base: cranelift_codegen::ir::Value,
    widx: Variable,
    tbl: cranelift_codegen::ir::Value,
) {
    let len = emit_table_len_call(tbl, builder, module, externs);
    let i = builder.declare_var(I64);
    let zero = builder.ins().iconst(I64, 0);
    builder.def_var(i, zero);

    let hdr = builder.create_block();
    let body = builder.create_block();
    let done = builder.create_block();
    builder.ins().jump(hdr, &[]);

    builder.switch_to_block(hdr);
    let iv = builder.use_var(i);
    let end = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, iv, len);
    builder.ins().brif(end, done, &[], body, &[]);

    builder.switch_to_block(body);
    builder.seal_block(body);
    let iv2 = builder.use_var(i);
    let out_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
    let tv_ref = module.declare_func_in_func(externs.table_value, builder.func);
    let call = builder.ins().call(tv_ref, &[tbl, iv2, out_addr]);
    let elem_ptr = builder.inst_results(call)[0];
    // DD#99 stage 7: guard splat ELEMENTS too. `table_value` writes the
    // element length into `out_slot`, so the check is available right here —
    // without it the guard would cover scalars only, which is worse than no
    // guard: it would imply argv NULs abort while `@a` still truncated.
    let elem_len = builder.ins().stack_load(I64, out_slot, 0);
    emit_argv_guard(builder, module, externs, elem_ptr, elem_len);
    store_argv_entry(builder, base, widx, elem_ptr);
    let next = builder.ins().iadd_imm(iv2, 1);
    builder.def_var(i, next);
    builder.ins().jump(hdr, &[]);
    builder.seal_block(hdr);

    builder.switch_to_block(done);
    builder.seal_block(done);
}
