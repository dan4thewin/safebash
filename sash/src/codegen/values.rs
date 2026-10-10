//! SSA value lowering: SsaValue → Cranelift IR (ptr, len) pairs.
//!
//! SECURITY: No implicit data introduced. Literals become data sections,
//! variables become register loads, concatenations call sash_concat.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{
    BlockArg, InstBuilder, MemFlags, StackSlot, StackSlotData, StackSlotKind,
};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::arithmetic::emit_arith_op;
use super::types::{emit_int_to_str, find_array_slot, ExternFuncs, VarSlot};
use super::value_handlers::{handle_cmdsub, handle_param_expansion};
use crate::ssa::*;

/// Call `sash_atoi(ptr, len) -> i64`. Used wherever a string slice needs
/// to be coerced to an integer at codegen time.
pub(super) fn atoi_call(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    ptr: cranelift_codegen::ir::Value,
    len: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
    let call = builder.ins().call(atoi_ref, &[ptr, len]);
    builder.inst_results(call)[0]
}

/// The `DataId` for the interned string `s`. The literal pre-pass
/// (`setup::intern_literals`) interns every string the SSA references, so a
/// miss here means a codegen path has drifted from that pre-pass. It was once
/// a `panic!`, then a declare-on-the-fly fallback; the fallback produced
/// *correct* (merely un-deduplicated) data, which is exactly what made it
/// useless — it papered over the drift instead of reporting it. Instrumenting
/// the miss and running the whole spec suite recorded **zero** misses across
/// 5747 tests, so the branch is a should-never-run ICE (DG, 2026-09-19: "any
/// code that should be unreachable should halt with an ICE if it ever ran").
pub(super) fn intern_or_declare(
    literals: &HashMap<String, cranelift_module::DataId>,
    s: &str,
) -> cranelift_module::DataId {
    *literals.get(s).unwrap_or_else(|| {
        panic!("ICE: codegen referenced a string the literal pre-pass did not intern: {s:?}")
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_value(
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    #[allow(unused_variables)] exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    match value {
        SsaValue::Literal(s) | SsaValue::Expr(s) => {
            lower_literal_value(s, builder, module, literals)
        }
        SsaValue::Var(vref) => {
            lower_var_value(vref, builder, module, vars, externs, exit_status_var)
        }
        SsaValue::FuncCall { .. }
        | SsaValue::IntFuncCall { .. }
        | SsaValue::CmdSubstBody { .. }
        | SsaValue::ArraySplat { .. }
        | SsaValue::ElideEmpty { .. } => crate::ssa::unlowerable_scalar_value("native", value),
        SsaValue::SlurpFile(path) => lower_slurp_file_value(
            path,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        // DD#91: `${x:!}` is a pure taint annotation — lower the source and
        // emit exactly what a bare expansion would.
        SsaValue::Trusted { source } => lower_value(
            source,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        SsaValue::Concat(parts) => lower_concat_value(
            parts,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        SsaValue::ParamExpansion {
            var,
            operator,
            operand,
        } => handle_param_expansion(
            var,
            operator,
            operand,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        SsaValue::RegexReplace {
            subject,
            pattern,
            replacement,
            global,
        } => lower_regex_replace_value(
            subject,
            pattern,
            replacement,
            *global,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        SsaValue::RegexMatch { subject, pattern } => lower_regex_match_value(
            subject,
            pattern,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        SsaValue::CaseConvert { subject, upper } => {
            let (s_ptr, s_len) = lower_value(
                subject,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let op = *upper;
            crate::codegen::value_handlers::emit_case_conversion(
                op, s_ptr, s_len, builder, module, externs,
            )
        }
        // DD#102: `$(strlen X)` — `lower_value` already yields the operand's
        // length alongside its pointer, so the byte count needs no runtime
        // call of its own.
        SsaValue::StrLen { subject } => {
            let (_s_ptr, s_len) = lower_value(
                subject,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            super::types::emit_int_to_str(builder, module, externs, s_len)
        }
        SsaValue::DefaultValue { source, default } => {
            let (s_ptr, s_len) = lower_value(
                source,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (d_ptr, d_len) = lower_value(
                default,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            crate::codegen::value_handlers::emit_default_value(
                s_ptr, s_len, d_ptr, d_len, builder, module, externs,
            )
        }
        SsaValue::Substring { source, spec } => {
            let (s_ptr, s_len) = lower_value(
                source,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            crate::codegen::value_handlers::emit_substring(
                spec, s_ptr, s_len, builder, module, externs,
            )
        }
        SsaValue::RequireValue { source, message } => {
            let (s_ptr, s_len) = lower_value(
                source,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (m_ptr, m_len) = lower_value(
                message,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            // Abort (`<prog>: MESSAGE`, exit 1) if unset/empty; else pass through.
            let req_ref = module.declare_func_in_func(externs.require_value, builder.func);
            builder.ins().call(req_ref, &[s_len, m_ptr, m_len]);
            (s_ptr, s_len)
        }
        SsaValue::ArithExpr { .. } => {
            // One arithmetic lowering, not two: ask the int path for the value
            // and stringify it. This arm used to recompute what
            // `lower_value_as_int` already does, which is why that arm was dead
            // in the native backend while the c backend hit its equivalent 57×.
            let result = lower_value_as_int(
                value,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            emit_int_to_str(builder, module, externs, result)
        }
        SsaValue::ArithBinOp {
            op: ArithOp::And | ArithOp::Or,
            ..
        } => {
            // Same delegation as the `ArithExpr` arm above: one arithmetic
            // lowering, not two. Routing through the int path is also what
            // keeps `&&`/`||` short-circuiting in a string context.
            let result = lower_value_as_int(
                value,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            emit_int_to_str(builder, module, externs, result)
        }
        SsaValue::ArithBinOp { left, op, right } => {
            let l = lower_value_as_int(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let r = lower_value_as_int(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let result = emit_arith_op(*op, l, r, builder, module, externs);
            emit_int_to_str(builder, module, externs, result)
        }
        SsaValue::CmdSubst {
            argv: argv_values,
            suppressed,
        } => handle_cmdsub(
            argv_values,
            *suppressed,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        SsaValue::AssocArray(pairs) => lower_assoc_array_value(
            pairs,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        SsaValue::Subscript {
            array,
            index,
            is_integer_key,
            is_integer_value,
        } => lower_subscript_value(
            array,
            index,
            *is_integer_key,
            *is_integer_value,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        SsaValue::ArgvIndex(idx_ref) => {
            lower_argv_index_value(idx_ref, builder, module, vars, externs)
        }
        SsaValue::TableValue { array, index } => {
            lower_table_value_value(array, index, builder, module, vars, externs)
        }
        SsaValue::TableKey { array, index } => {
            lower_table_key_value(array, index, builder, module, vars, externs)
        }
        SsaValue::Ternary {
            cond,
            true_val,
            false_val,
        } => lower_ternary_value(
            cond,
            true_val,
            false_val,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        SsaValue::IndexedArray(elems) => lower_indexed_array_value(
            elems,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        // DD#76: an int cell stores as `(val_ptr = null, val_len = the i64)`.
        SsaValue::IntCell(inner) => {
            let n = lower_value_as_int(
                inner,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let null_ptr = builder.ins().iconst(I64, 0);
            (null_ptr, n)
        }
    }
}

/// Stack slot for "out_len" arguments to extern calls returning (ptr, len-by-ref).
pub(super) fn alloc_out_slot(
    builder: &mut FunctionBuilder,
) -> (StackSlot, cranelift_codegen::ir::Value) {
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let addr = builder.ins().stack_addr(I64, slot, 0);
    (slot, addr)
}

/// Look up the table pointer for an array: prefer `name_0`, else find any
/// `name_*` and use its slot's str_ptr.
fn lookup_array_table(
    array: &str,
    vars: &HashMap<String, VarSlot>,
    builder: &mut FunctionBuilder,
) -> cranelift_codegen::ir::Value {
    let sv = find_array_slot(vars, array, "array");
    builder.use_var(sv.str_ptr())
}

pub(super) fn lower_literal_value(
    s: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    literals: &HashMap<String, cranelift_module::DataId>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let data_id = literals[s];
    let gv = module.declare_data_in_func(data_id, builder.func);
    let ptr = builder.ins().global_value(I64, gv);
    let len = builder.ins().iconst(I64, s.len() as i64);
    (ptr, len)
}

/// DD#63: lower `$(sub S P R)` / `$(gsub S P R)`. Each operand is an
/// ordinary value (recursively lowered to a (ptr,len) pair), so quoting
/// and interpolation are normal argument evaluation. Calls
/// `sash_regex_replace(s, slen, p, plen, r, rlen, global) -> (ptr,len)`.
#[allow(clippy::too_many_arguments)]
fn lower_regex_replace_value(
    subject: &SsaValue,
    pattern: &SsaValue,
    replacement: &SsaValue,
    global: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let (s_ptr, s_len) = lower_value(
        subject,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let (p_ptr, p_len) = lower_value(
        pattern,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let (r_ptr, r_len) = lower_value(
        replacement,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let global_val = builder.ins().iconst(I64, global as i64);
    let replace_ref = module.declare_func_in_func(externs.regex_replace, builder.func);
    let call = builder.ins().call(
        replace_ref,
        &[s_ptr, s_len, p_ptr, p_len, r_ptr, r_len, global_val],
    );
    (builder.inst_results(call)[0], builder.inst_results(call)[1])
}

/// DD#63: lower `$(match S P)` (whole-match capture) to a call to
/// `sash_regex_match_str(s, slen, p, plen) -> (ptr, len)`.
#[allow(clippy::too_many_arguments)]
fn lower_regex_match_value(
    subject: &SsaValue,
    pattern: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let (s_ptr, s_len) = lower_value(
        subject,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let (p_ptr, p_len) = lower_value(
        pattern,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let match_ref = module.declare_func_in_func(externs.regex_match_str, builder.func);
    let call = builder.ins().call(match_ref, &[s_ptr, s_len, p_ptr, p_len]);
    (builder.inst_results(call)[0], builder.inst_results(call)[1])
}

fn lower_var_value(
    vref: &VarRef,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let key = vref.display();
    let base = strip_ssa_version(&key);
    if base == "?" {
        if let Some(esv) = exit_status_var {
            let status = builder.use_var(esv);
            return emit_int_to_str(builder, module, externs, status);
        }
    }
    lower_slot_as_str_value(&key, builder, module, vars, externs)
}

/// Read a variable's slot in string context: an int slot is formatted to a
/// decimal string, a string slot yields its (ptr, len) directly. An undefined
/// variable reads as the empty string (null ptr, zero len).
fn lower_slot_as_str_value(
    key: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    match vars.get(key) {
        None => {
            let zero = builder.ins().iconst(I64, 0);
            (zero, zero)
        }
        Some(&VarSlot::Int(v)) => {
            let int_val = builder.use_var(v);
            emit_int_to_str(builder, module, externs, int_val)
        }
        Some(&VarSlot::Str { ptr, len }) => {
            let ptr = builder.use_var(ptr);
            let len = builder.use_var(len);
            (ptr, len)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_concat_value(
    parts: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    assert!(!parts.is_empty(), "empty Concat");
    let (mut acc_ptr, mut acc_len) = lower_value(
        &parts[0],
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    for part in &parts[1..] {
        let (p_ptr, p_len) = lower_value(
            part,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        );
        let (out_slot, out_addr) = alloc_out_slot(builder);
        let concat_ref = module.declare_func_in_func(externs.concat, builder.func);
        let call = builder
            .ins()
            .call(concat_ref, &[acc_ptr, acc_len, p_ptr, p_len, out_addr]);
        acc_ptr = builder.inst_results(call)[0];
        acc_len = builder.ins().stack_load(I64, out_slot, 0);
    }
    (acc_ptr, acc_len)
}

#[allow(clippy::too_many_arguments)]
fn lower_assoc_array_value(
    pairs: &[(String, SsaValue)],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let capacity = core::cmp::max(pairs.len() * 2, 8) as i64;
    let cap_val = builder.ins().iconst(I64, capacity);
    let new_ref = module.declare_func_in_func(externs.table_new, builder.func);
    let call = builder.ins().call(new_ref, &[cap_val]);
    let table_ptr = builder.inst_results(call)[0];

    let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
    for (key, val_ssa) in pairs.iter() {
        let key_data = intern_or_declare(literals, key.as_str());
        let key_gv = module.declare_data_in_func(key_data, builder.func);
        let key_ptr = builder.ins().global_value(I64, key_gv);
        let key_len = builder.ins().iconst(I64, key.len() as i64);
        let (val_ptr, val_len) = lower_value(
            val_ssa,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        );
        builder
            .ins()
            .call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
    }
    let zero = builder.ins().iconst(I64, 0);
    (table_ptr, zero)
}

#[allow(clippy::too_many_arguments)]
fn lower_subscript_value(
    array: &str,
    index: &SsaValue,
    is_integer_key: bool,
    is_integer_value: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let (val_ptr, val_len) = lower_subscript_raw(
        array,
        index,
        is_integer_key,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    // DD#76: an int-cell read in string context renders the i64 (`val_len`) as
    // a decimal string (`val_ptr` is null for an int cell). A string cell
    // returns its stored `(ptr, len)` verbatim.
    if is_integer_value {
        emit_int_to_str(builder, module, externs, val_len)
    } else {
        (val_ptr, val_len)
    }
}

/// The raw `sash_table_get` result — `(val_ptr, val_len)` as stored. For a
/// string cell that's `(ptr, byte_len)`; for an int cell (DD#76) it's
/// `(null, the i64)`; a missing key is `(null, 0)`.
#[allow(clippy::too_many_arguments)]
fn lower_subscript_raw(
    array: &str,
    index: &SsaValue,
    is_integer_key: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let table_ptr = lookup_array_table(array, vars, builder);
    let (key_ptr, key_len) = if is_integer_key {
        let idx_int = lower_value_as_int(
            index,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        );
        let (idx_slot, kp) = alloc_out_slot(builder);
        builder.ins().stack_store(idx_int, idx_slot, 0);
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
            exit_status_var,
        )
    };
    let (out_slot, out_addr) = alloc_out_slot(builder);
    let get_ref = module.declare_func_in_func(externs.table_get, builder.func);
    let call = builder
        .ins()
        .call(get_ref, &[table_ptr, key_ptr, key_len, out_addr]);
    let val_ptr = builder.inst_results(call)[0];
    let val_len = builder.ins().stack_load(I64, out_slot, 0);
    (val_ptr, val_len)
}

/// A subscript read in int context: an int cell yields its raw `val_len`; a
/// string cell atoi's its stored bytes (DD#76).
#[allow(clippy::too_many_arguments)]
fn lower_subscript_as_int(
    array: &str,
    index: &SsaValue,
    is_integer_key: bool,
    is_integer_value: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    let (val_ptr, val_len) = lower_subscript_raw(
        array,
        index,
        is_integer_key,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    if is_integer_value {
        return val_len;
    }
    let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
    let call = builder.ins().call(atoi_ref, &[val_ptr, val_len]);
    builder.inst_results(call)[0]
}

fn lower_argv_index_value(
    idx_ref: &VarRef,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let idx_int = index_var_to_int(idx_ref, builder, module, vars, externs);
    // DD#60: argv slice is 0-based — argv[idx] directly (idx 0 = program name).
    let eight = builder.ins().iconst(I64, 8);
    let offset = builder.ins().imul(idx_int, eight);
    let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
    let argv_ptr = builder.ins().global_value(I64, argv_gv);
    let argv_val = builder.ins().load(I64, MemFlags::trusted(), argv_ptr, 0);
    let arg_addr = builder.ins().iadd(argv_val, offset);
    let arg_ptr = builder.ins().load(I64, MemFlags::trusted(), arg_addr, 0);
    let arg_len = emit_strlen(builder, module, externs.strlen, arg_ptr);
    (arg_ptr, arg_len)
}

fn lower_table_value_value(
    array: &str,
    index: &VarRef,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let table_ptr = lookup_array_table(array, vars, builder);
    let idx_int = index_var_to_int(index, builder, module, vars, externs);
    let (out_slot, out_addr) = alloc_out_slot(builder);
    let tv_ref = module.declare_func_in_func(externs.table_value, builder.func);
    let call = builder.ins().call(tv_ref, &[table_ptr, idx_int, out_addr]);
    let val_ptr = builder.inst_results(call)[0];
    let val_len = builder.ins().stack_load(I64, out_slot, 0);
    (val_ptr, val_len)
}

fn lower_table_key_value(
    array: &str,
    index: &VarRef,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let table_ptr = lookup_array_table(array, vars, builder);
    let idx_int = index_var_to_int(index, builder, module, vars, externs);
    let (out_slot, out_addr) = alloc_out_slot(builder);
    let tk_ref = module.declare_func_in_func(externs.table_key, builder.func);
    let call = builder.ins().call(tk_ref, &[table_ptr, idx_int, out_addr]);
    let key_ptr = builder.inst_results(call)[0];
    let key_len = builder.ins().stack_load(I64, out_slot, 0);
    (key_ptr, key_len)
}

/// Load a var used as an integer index. Total over both slot kinds: an int slot
/// is read directly; a string slot is `atoi`'d. This removes the `str_ptr()`
/// -on-int panic and lets a numeric loop counter be an Int slot (the previous
/// `str_ptr()+atoi` assumed every index var was a string — a convention between
/// the loop-lowering and this read, not a type-level guarantee).
/// `$(<file)` — one `sash_slurp_file` call. No fork, no pipe, no `/usr/bin/cat`.
///
/// The path is an ordinary value, so `$(<$f)` works with no extra machinery.
/// A missing file aborts inside the runtime (it cannot fabricate contents),
/// which is what the old `cat` desugar did too — only the diagnostic changes.
#[allow(clippy::too_many_arguments)]
fn lower_slurp_file_value(
    path: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let (path_ptr, path_len) = lower_value(
        path,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let out_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
    let func_ref = module.declare_func_in_func(externs.slurp_file, builder.func);
    let call = builder
        .ins()
        .call(func_ref, &[path_ptr, path_len, out_addr]);
    let ptr = builder.inst_results(call)[0];
    let len = builder.ins().stack_load(I64, out_slot, 0);
    (ptr, len)
}

fn index_var_to_int(
    index: &VarRef,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    let idx_sv = vars
        .get(&index.display())
        .unwrap_or_else(|| panic!("ICE: undefined index var: {}", index.display()));
    match *idx_sv {
        VarSlot::Int(v) => builder.use_var(v),
        VarSlot::Str { ptr, len } => {
            let idx_ptr = builder.use_var(ptr);
            let idx_len = builder.use_var(len);
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
            builder.inst_results(call)[0]
        }
    }
}

/// DD#25/bash parity: lazy `&&` / `||` inside `(( … ))`. bash evaluates the
/// right operand only when the left does not already decide the result, so a
/// guard idiom like `(( d != 0 && 10 / d > 1 ))` must not run the division
/// when `d` is 0. Lowering both operands up front and combining them (the old
/// `emit_logical_and_or`) ran the right operand's div-by-zero check and
/// overflow trap unconditionally — the same eager-evaluation class as the
/// ternary bug fixed in journal 193. Result is 1 or 0, as in bash.
#[allow(clippy::too_many_arguments)]
fn lower_logical_short_circuit(
    op: ArithOp,
    left: &SsaValue,
    right: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    let l = lower_value_as_int(
        left,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let zero = builder.ins().iconst(I64, 0);
    let l_true = builder.ins().icmp(IntCC::NotEqual, l, zero);

    let rhs_block = builder.create_block();
    let merge_block = builder.create_block();
    let result = builder.append_block_param(merge_block, I64);

    // `&&` short-circuits to 0 on a false left; `||` to 1 on a true left.
    let is_and = matches!(op, ArithOp::And);
    let short = builder.ins().iconst(I64, if is_and { 0 } else { 1 });
    if is_and {
        builder.ins().brif(
            l_true,
            rhs_block,
            &[],
            merge_block,
            &[BlockArg::Value(short)],
        );
    } else {
        builder.ins().brif(
            l_true,
            merge_block,
            &[BlockArg::Value(short)],
            rhs_block,
            &[],
        );
    }

    builder.switch_to_block(rhs_block);
    builder.seal_block(rhs_block);
    let r = lower_value_as_int(
        right,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let r_zero = builder.ins().iconst(I64, 0);
    let r_true = builder.ins().icmp(IntCC::NotEqual, r, r_zero);
    let r_i64 = builder.ins().uextend(I64, r_true);
    builder.ins().jump(merge_block, &[BlockArg::Value(r_i64)]);

    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
    result
}

/// Lazy int-valued ternary: brif on the condition into then/else blocks, each
/// lowering only its own branch, merging the chosen i64 via a block param. Like
/// `lower_ternary_value` but for a single int result (side effects stay lazy).
#[allow(clippy::too_many_arguments)]
fn lower_ternary_int(
    cond: &SsaValue,
    true_val: &SsaValue,
    false_val: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    let cond_int = lower_value_as_int(
        cond,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let zero = builder.ins().iconst(I64, 0);
    let is_true = builder.ins().icmp(IntCC::NotEqual, cond_int, zero);
    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let merge_block = builder.create_block();
    let result = builder.append_block_param(merge_block, I64);
    builder
        .ins()
        .brif(is_true, then_block, &[], else_block, &[]);

    builder.switch_to_block(then_block);
    builder.seal_block(then_block);
    let t = lower_value_as_int(
        true_val,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    builder.ins().jump(merge_block, &[BlockArg::Value(t)]);

    builder.switch_to_block(else_block);
    builder.seal_block(else_block);
    let f = lower_value_as_int(
        false_val,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    builder.ins().jump(merge_block, &[BlockArg::Value(f)]);

    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
    result
}

#[allow(clippy::too_many_arguments)]
fn lower_ternary_value(
    cond: &SsaValue,
    true_val: &SsaValue,
    false_val: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let cond_int = lower_value_as_int(
        cond,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    let zero = builder.ins().iconst(I64, 0);
    let is_true = builder.ins().icmp(IntCC::NotEqual, cond_int, zero);
    // Lazy: lower each branch inside its own block so a side-effecting branch
    // (div/mod-by-zero, overflow trap) runs only when selected — matches bash's
    // short-circuit `?:`. A flat `select` over both branches evaluated both.
    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let merge_block = builder.create_block();
    let r_ptr = builder.append_block_param(merge_block, I64);
    let r_len = builder.append_block_param(merge_block, I64);
    builder
        .ins()
        .brif(is_true, then_block, &[], else_block, &[]);

    builder.switch_to_block(then_block);
    builder.seal_block(then_block);
    let (t_ptr, t_len) = lower_value(
        true_val,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    builder.ins().jump(
        merge_block,
        &[BlockArg::Value(t_ptr), BlockArg::Value(t_len)],
    );

    builder.switch_to_block(else_block);
    builder.seal_block(else_block);
    let (f_ptr, f_len) = lower_value(
        false_val,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    builder.ins().jump(
        merge_block,
        &[BlockArg::Value(f_ptr), BlockArg::Value(f_len)],
    );

    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
    (r_ptr, r_len)
}

#[allow(clippy::too_many_arguments)]
fn lower_indexed_array_value(
    elems: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let capacity = core::cmp::max(elems.len() * 2, 8) as i64;
    let cap_val = builder.ins().iconst(I64, capacity);
    let new_ref = module.declare_func_in_func(externs.table_new, builder.func);
    let call = builder.ins().call(new_ref, &[cap_val]);
    let table_ptr = builder.inst_results(call)[0];

    // DD#84: a running write index (a Cranelift Variable) rather than the
    // element position, so a `${…:~}` element that is empty is skipped and the
    // remaining elements stay contiguous (`(x ${y:~} z)` with empty y → [x, z]).
    let widx = builder.declare_var(I64);
    let zero0 = builder.ins().iconst(I64, 0);
    builder.def_var(widx, zero0);
    for val_ssa in elems {
        match val_ssa {
            SsaValue::ElideEmpty { source: inner, .. } => {
                let (val_ptr, val_len) = lower_value(
                    inner,
                    builder,
                    module,
                    vars,
                    literals,
                    externs,
                    exit_status_var,
                );
                emit_array_set_if_nonempty(
                    builder, module, externs, table_ptr, widx, val_ptr, val_len,
                );
            }
            // A splat contributes its ELEMENTS, not itself. Without this arm
            // it fell to `_` below and was lowered as one value that
            // stringifies as the joined array — so `f @a` passed a function a
            // single "x y z", and `(@a q)` built a 2-element array. Silently
            // lossy: `("a b" "c")` and `(a b c)` both collapsed to "a b c".
            SsaValue::ArraySplat { array, disposition } => {
                let tbl = super::commands::array_table_ptr(array, vars, builder);
                let eff = match disposition {
                    None => tbl,
                    Some(d) => super::commands::emit_splat_disposition(
                        builder,
                        module,
                        vars,
                        literals,
                        externs,
                        exit_status_var,
                        tbl,
                        d,
                    ),
                };
                emit_splat_append(builder, module, externs, table_ptr, widx, eff);
            }
            _ => {
                let (val_ptr, val_len) = lower_value(
                    val_ssa,
                    builder,
                    module,
                    vars,
                    literals,
                    externs,
                    exit_status_var,
                );
                emit_array_set_running(builder, module, externs, table_ptr, widx, val_ptr, val_len);
            }
        }
    }
    let zero = builder.ins().iconst(I64, 0);
    (table_ptr, zero)
}

/// Append every element of `src` to `dst` at the running write index — the
/// array-building counterpart of the argv builder's `emit_splat_fill`.
fn emit_splat_append(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    dst: cranelift_codegen::ir::Value,
    widx: Variable,
    src: cranelift_codegen::ir::Value,
) {
    let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
    let call = builder.ins().call(len_ref, &[src]);
    let len = builder.inst_results(call)[0];

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
    let (out_slot, out_addr) = alloc_out_slot(builder);
    let tv_ref = module.declare_func_in_func(externs.table_value, builder.func);
    let call = builder.ins().call(tv_ref, &[src, iv2, out_addr]);
    let elem_ptr = builder.inst_results(call)[0];
    let elem_len = builder.ins().stack_load(I64, out_slot, 0);
    emit_array_set_running(builder, module, externs, dst, widx, elem_ptr, elem_len);
    let next = builder.ins().iadd_imm(iv2, 1);
    builder.def_var(i, next);
    builder.ins().jump(hdr, &[]);
    builder.seal_block(hdr);

    builder.switch_to_block(done);
    builder.seal_block(done);
}

/// `table[widx++] = (val_ptr, val_len)` — set at the running index and advance.
fn emit_array_set_running(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    table_ptr: cranelift_codegen::ir::Value,
    widx: Variable,
    val_ptr: cranelift_codegen::ir::Value,
    val_len: cranelift_codegen::ir::Value,
) {
    let (idx_slot, key_ptr) = alloc_out_slot(builder);
    let cur = builder.use_var(widx);
    builder.ins().stack_store(cur, idx_slot, 0);
    let key_len = builder.ins().iconst(I64, 8);
    let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
    builder
        .ins()
        .call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
    let next = builder.ins().iadd_imm(cur, 1);
    builder.def_var(widx, next);
}

/// DD#84: `table[widx++] = val` only when `val_len != 0` — an empty (identity)
/// `${…:~}` element is elided, keeping indices contiguous.
fn emit_array_set_if_nonempty(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    table_ptr: cranelift_codegen::ir::Value,
    widx: Variable,
    val_ptr: cranelift_codegen::ir::Value,
    val_len: cranelift_codegen::ir::Value,
) {
    let set_blk = builder.create_block();
    let cont_blk = builder.create_block();
    let is_empty = builder.ins().icmp_imm(IntCC::Equal, val_len, 0);
    builder.ins().brif(is_empty, cont_blk, &[], set_blk, &[]);

    builder.switch_to_block(set_blk);
    builder.seal_block(set_blk);
    emit_array_set_running(builder, module, externs, table_ptr, widx, val_ptr, val_len);
    builder.ins().jump(cont_blk, &[]);

    builder.switch_to_block(cont_blk);
    builder.seal_block(cont_blk);
}

/// DD#77 arith `:-`/`:?` dispositions in int context — `${s:-N}` via
/// `sash_atoi_or` (is_int(s) ? s : N), `${s:?msg}` via `sash_atoi_or_abort`
/// (is_int(s) ? s : abort). `None` for any other value (handled by
/// `lower_value_as_int`'s main match). Split out for the complexity gate.
#[allow(clippy::too_many_arguments)]
fn lower_disposition_as_int(
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> Option<cranelift_codegen::ir::Value> {
    match value {
        // DD#76: an int-cell subscript read / int cell in int context.
        SsaValue::Subscript {
            array,
            index,
            is_integer_key,
            is_integer_value,
        } => Some(lower_subscript_as_int(
            array,
            index,
            *is_integer_key,
            *is_integer_value,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        )),
        SsaValue::IntCell(_) => crate::ssa::int_cell_in_int_context("native codegen"),
        SsaValue::DefaultValue { source, default } => {
            let (s_ptr, s_len) = lower_value(
                source,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let d_int = lower_value_as_int(
                default,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let atoi_or = module.declare_func_in_func(externs.atoi_or, builder.func);
            let call = builder.ins().call(atoi_or, &[s_ptr, s_len, d_int]);
            Some(builder.inst_results(call)[0])
        }
        SsaValue::RequireValue { source, message } => {
            let (s_ptr, s_len) = lower_value(
                source,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (m_ptr, m_len) = lower_value(
                message,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let atoi_or_abort = module.declare_func_in_func(externs.atoi_or_abort, builder.func);
            let call = builder
                .ins()
                .call(atoi_or_abort, &[s_ptr, s_len, m_ptr, m_len]);
            Some(builder.inst_results(call)[0])
        }
        _ => None,
    }
}

/// DD#58/Option C: a sexpr nullary int intrinsic reuses the word-form
/// `CmdSubst` node (`(getppid)` → `CmdSubst{["getppid"]}`), and the word form
/// `$(getppid)` builds the same node directly. Either way int context computes
/// the i64 DIRECTLY, with no string round-trip.
///
/// **There is no fallback, because nothing can reach one.** There was one
/// until 2026-10-10 — `lower_value` + `sash_atoi` — and `epochseconds` was its
/// last user, until DG's ruling that every int intrinsic be int-valued in the
/// runtime made `sash_epochseconds()` return an i64. What remains is
/// unreachable by construction, on the producer side:
///
/// - `type_check` rejects every capture in int context that is not an
///   `ArithDenotation::Int` intrinsic (SB4030, measured for `$(getcwd)`,
///   `$(getenv …)` and an external command, in both the `int x =` and the
///   array-subscript positions). It is behind no flag — `--no-taint` still
///   reports it.
/// - `status` and `getcpid`, the two stateful `Int` names, are rewritten to
///   the `?`/`!` slots by `nullary_stateful_slot` before codegen runs.
/// - the remaining seven each have an `int_intrinsic_i64` arm, and `wait` is
///   taken above.
///
/// The ICE earns its line by asking a *different* question from the gates
/// upstream, on different data. `type_check` consults `resolve`'s denotation
/// table and never looks at the emission table here, so it cannot notice an
/// `Int` name added to `resolve` and never given an arm.
/// `resolve_nullary_sexpr`'s guard catches exactly that — but only for the
/// `(name)` sexpr. The word form `$(name)` builds its `CmdSubst` in
/// `handle_command_substitution` without passing through there, so this is the
/// hop that guard cannot cover. Silently, it would be `getcpid`'s historical
/// 0: `lower_value` would try to exec a name that is not a program, and
/// `sash_atoi` an empty capture.
///
/// Not unit-tested — reaching it needs a live `FunctionBuilder` and
/// `ObjectModule`, and `codegen/` has no harness for either (`codegen/tests.rs`
/// is the `[[ ]]` lowering, not a test module). The first hop of the same
/// regression is unit-tested, at `transform::expr_handlers::tests::
/// a_lowering_less_intrinsic_in_a_nullary_sexpr_is_a_loud_ice`.
#[allow(clippy::too_many_arguments)]
fn lower_cmdsubst_as_int(
    argv: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    if let Some(SsaValue::Literal(name)) = argv.first() {
        // DD#126: `wait` takes an operand, so it cannot go through the
        // nullary table below.
        if name == "wait" {
            return crate::codegen::value_handlers::emit_wait_value(
                argv,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
        }
        if let Some(v) = crate::codegen::value_handlers::int_intrinsic_i64(
            name,
            builder,
            module,
            externs,
            exit_status_var,
        ) {
            return v;
        }
    }
    unreachable!(
        "`$( … )` in int context must name `wait` or an intrinsic with an \
         `int_intrinsic_i64` arm. SB4030 rejects every other capture there, \
         and `nullary_stateful_slot` has already taken `status`/`getcpid`, so \
         a head with no arm is an `ArithDenotation::Int` name added to \
         `resolve` and never given one — `getcpid`'s silent 0, in the word \
         form that `resolve_nullary_sexpr`'s guard does not see. Head: {:?}",
        argv.first()
    )
}

/// Emit Cranelift instructions for lowering an `SsaValue` to an i64 in
/// arithmetic / comparison context.
#[allow(clippy::too_many_arguments)]
/// The integer value of a literal in an int context.
///
/// An **empty** literal is a bare `int n` declaration's zero-init — that is
/// what makes `int n; echo $n` print `0` (specs `86.5`, `ud.6`, `94.1`-`94.7`),
/// so it is a value, not a failure.
///
/// A **non-empty** literal that does not parse is a different thing entirely:
/// it used to become 0 too, which is how `exit 1x` reported success. SB4030
/// rejects those in `type_check`, so reaching the panic means a check was
/// missed — and a silent 0 is exactly what would hide that (DD#116,
/// property 1: the fallback degrades silently wrong).
pub(super) fn int_literal_or_zero_init(s: &str) -> i64 {
    if s.is_empty() {
        return 0;
    }
    s.parse::<i64>().unwrap_or_else(|_| {
        panic!("non-integer literal `{s}` reached int lowering; type_check (SB4030) should have rejected it")
    })
}

pub(super) fn lower_value_as_int(
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    // DD#77 arith `:-`/`:?` dispositions are handled first (out-of-line) to keep
    // this dispatcher under the complexity gate.
    if let Some(v) = lower_disposition_as_int(
        value,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    ) {
        return v;
    }
    match value {
        SsaValue::Literal(s) | SsaValue::Expr(s) => {
            let n = int_literal_or_zero_init(s);
            builder.ins().iconst(I64, n)
        }
        SsaValue::Var(vref) => {
            lower_var_as_int(vref, builder, module, vars, externs, exit_status_var)
        }
        SsaValue::ArithExpr { var, op, operand } => {
            let var_val = lower_value_as_int(
                &SsaValue::Var(var.clone()),
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let rhs = builder.ins().iconst(I64, *operand);
            emit_arith_op(*op, var_val, rhs, builder, module, externs)
        }
        SsaValue::ArithBinOp { left, op, right } => {
            // `&&` / `||` short-circuit (bash): the right operand is not
            // evaluated when the left already decides the result, so its
            // guards (div/mod-by-zero, overflow trap) must not run either.
            // Same class as the eager-ternary bug fixed in journal 193.
            if matches!(op, ArithOp::And | ArithOp::Or) {
                return lower_logical_short_circuit(
                    *op,
                    left,
                    right,
                    builder,
                    module,
                    vars,
                    literals,
                    externs,
                    exit_status_var,
                );
            }
            let l = lower_value_as_int(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let r = lower_value_as_int(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            emit_arith_op(*op, l, r, builder, module, externs)
        }
        SsaValue::Ternary {
            cond,
            true_val,
            false_val,
        } => lower_ternary_int(
            cond,
            true_val,
            false_val,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        // `${#arr[@]}` (array length) is already an integer — call
        // sash_table_len directly instead of round-tripping through a
        // decimal string + atoi (the generic fallback below). Matters for
        // synthesized for-loop bounds (`__ai < ${arr arrlen}`), re-evaluated
        // every iteration.
        SsaValue::ParamExpansion { var, operator, .. } if operator == "arrlen" => {
            let sv = find_array_slot(vars, &var.name, "arrlen");
            let table_ptr = builder.use_var(sv.str_ptr());
            let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
            let call = builder.ins().call(len_ref, &[table_ptr]);
            builder.inst_results(call)[0]
        }
        // DD#58/Option C: a sexpr nullary int intrinsic reuses the word-form
        // `CmdSubst` node (`(getppid)` → `CmdSubst{["getppid"]}`). In int
        // context, compute the i64 DIRECTLY (no string round-trip). Every
        // capture that reaches int context is one of those — SB4030 rejects
        // the rest — so the callee ICEs rather than falling back to atoi.
        SsaValue::CmdSubst { argv, .. } => lower_cmdsubst_as_int(
            argv,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        // DD#76: an int-cell subscript read in int context is the i64 directly
        // (`val_len`), skipping the string round-trip and the null-`val_ptr`
        // atoi. A string cell atoi's its stored bytes.
        // (Subscript / IntCell handled out-of-line in lower_disposition_as_int.)
        _ => {
            // Fallback: lower as string, atoi
            let (ptr, len) = lower_value(
                value,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[ptr, len]);
            builder.inst_results(call)[0]
        }
    }
}

/// A `SsaValue::Var` read in int context: a computed special ($$/$?) computes
/// its value; a materialized slot reads its int (or atoi's its string); an
/// unknown key is 0.
fn lower_var_as_int(
    vref: &VarRef,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    let key = vref.display();
    // Computed specials ($$ → getpid, $? → the exit-status var) have no
    // materialized slot — mirror the string path (lower_var_value) so a special
    // read as an int OPERAND computes its value instead of the undefined-var 0.
    // ($# is a real slot, so it falls through below.)
    if let Some(v) = special_var_as_int(&key, builder, exit_status_var) {
        return v;
    }
    let Some(sv) = vars.get(&key) else {
        // Undefined variable → 0 in int context
        return builder.ins().iconst(I64, 0);
    };
    match *sv {
        VarSlot::Int(v) => builder.use_var(v),
        VarSlot::Str { ptr, len } => {
            // String var in int context: atoi
            let ptr = builder.use_var(ptr);
            let len = builder.use_var(len);
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[ptr, len]);
            builder.inst_results(call)[0]
        }
    }
}

/// The one computed special var with no materialized slot: `$?` → the
/// exit-status var. Mirrors the string path (`lower_var_value`) so it is read
/// as an int operand by computing its value rather than the undefined-var 0.
/// Returns `None` for an ordinary variable (`$#` included — it is a real slot
/// handled by the normal path). DD#24/DD#31: this is a clean arithmetic
/// operand. (`$!` / getcpid is a materialized slot, not here. `getpid` is a
/// stateless *intrinsic* and never reaches this path — `int_intrinsic_i64`
/// lowers it by name.)
fn special_var_as_int(
    key: &str,
    builder: &mut FunctionBuilder,
    exit_status_var: Option<Variable>,
) -> Option<cranelift_codegen::ir::Value> {
    match strip_ssa_version(key) {
        "?" => exit_status_var.map(|esv| builder.use_var(esv)),
        _ => None,
    }
}

/// Strip a trailing `_<digits>` SSA version suffix from a variable key, yielding
/// the base name (`$_3` → `$`, `count_7` → `count`). Matches the base-extraction
/// in the string path (`lower_var_value`).
fn strip_ssa_version(key: &str) -> &str {
    if let Some(idx) = key.rfind('_') {
        if key[idx + 1..].chars().all(|c| c.is_ascii_digit()) {
            return &key[..idx];
        }
    }
    key
}

/// Emit Cranelift IR to compute strlen by calling sash-rt's strlen
pub(super) fn emit_strlen(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    strlen_func: cranelift_module::FuncId,
    ptr: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let strlen_ref = module.declare_func_in_func(strlen_func, builder.func);
    let call = builder.ins().call(strlen_ref, &[ptr]);
    builder.inst_results(call)[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `setup::intern_literals` interns every string the SSA references, so no
    /// program can reach the miss — instrumenting it and running the whole
    /// spec suite recorded zero hits across 5747 tests. Calling the lookup
    /// directly with an empty map is the only way to execute the guard.
    /// Same shape as `unresolved_redirect_var_is_loud_ice` in
    /// `codegen::redirects`.
    #[test]
    fn missing_literal_is_loud_ice() {
        let literals: HashMap<String, cranelift_module::DataId> = HashMap::new();
        let caught = std::panic::catch_unwind(|| intern_or_declare(&literals, "never-interned"));
        assert!(
            caught.is_err(),
            "a string the literal pre-pass did not intern must raise a loud ICE"
        );
    }
}
