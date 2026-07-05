//! SSA value lowering: SsaValue → Cranelift IR (ptr, len) pairs.
//!
//! SECURITY: No implicit data introduced. Literals become data sections,
//! variables become register loads, concatenations call sash_concat.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{InstBuilder, MemFlags, StackSlot, StackSlotData, StackSlotKind};
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

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_value(
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
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
        SsaValue::FuncCall(name) => {
            lower_func_call_value(name, builder, module, literals, func_constants)
        }
        SsaValue::IntFuncCall { name, .. } => {
            panic!(
                "ICE: IntFuncCall should have been lifted during transform: {}",
                name
            );
        }
        SsaValue::ArraySplat { array } => {
            panic!(
                "ICE: ArraySplat (@{}) must be expanded by the argv builder, \
                 not lowered as a scalar value",
                array
            );
        }
        SsaValue::Concat(parts) => lower_concat_value(
            parts,
            builder,
            module,
            vars,
            literals,
            func_constants,
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
            func_constants,
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
            func_constants,
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
            func_constants,
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
                func_constants,
                externs,
                exit_status_var,
            );
            let op = if *upper { "^^" } else { ",," };
            crate::codegen::value_handlers::emit_case_conversion(
                op, s_ptr, s_len, builder, module, externs,
            )
        }
        SsaValue::DefaultValue { var, default } => {
            let (s_ptr, s_len) = lower_value(
                &SsaValue::Var(var.clone()),
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                exit_status_var,
            );
            let (d_ptr, d_len) = lower_value(
                default,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                exit_status_var,
            );
            crate::codegen::value_handlers::emit_default_value(
                s_ptr, s_len, d_ptr, d_len, builder, module, externs,
            )
        }
        SsaValue::ArithExpr { var, op, operand } => {
            let int_val = lower_value_as_int(
                &SsaValue::Var(var.clone()),
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            let rhs = builder.ins().iconst(I64, *operand);
            let result =
                emit_arith_op(*op, int_val, rhs, builder, module, externs, exit_status_var);
            emit_int_to_str(builder, module, externs, result)
        }
        SsaValue::ArithBinOp { left, op, right } => {
            let l = lower_value_as_int(
                left,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            let r = lower_value_as_int(
                right,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            let result = emit_arith_op(*op, l, r, builder, module, externs, None);
            emit_int_to_str(builder, module, externs, result)
        }
        SsaValue::CmdSubst {
            argv: argv_values,
            redirects,
        } => handle_cmdsub(
            argv_values,
            redirects,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
            exit_status_var,
        ),
        SsaValue::AssocArray(pairs) => lower_assoc_array_value(
            pairs,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
            exit_status_var,
        ),
        SsaValue::Subscript {
            array,
            index,
            is_integer_key,
        } => lower_subscript_value(
            array,
            index,
            *is_integer_key,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
            exit_status_var,
        ),
        SsaValue::ArgvIndex(idx_ref) => {
            lower_argv_index_value(idx_ref, builder, module, vars, externs)
        }
        SsaValue::ArgvSnapshot { start } => {
            let start_val = builder.ins().iconst(I64, *start);
            let snap_ref = module.declare_func_in_func(externs.argv_snapshot, builder.func);
            let call = builder.ins().call(snap_ref, &[start_val]);
            let table_ptr = builder.inst_results(call)[0];
            let zero = builder.ins().iconst(I64, 0);
            (table_ptr, zero)
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
            func_constants,
            externs,
            exit_status_var,
        ),
        SsaValue::IndexedArray(elems) => lower_indexed_array_value(
            elems,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
            exit_status_var,
        ),
    }
}

/// Stack slot for "out_len" arguments to extern calls returning (ptr, len-by-ref).
fn alloc_out_slot(builder: &mut FunctionBuilder) -> (StackSlot, cranelift_codegen::ir::Value) {
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
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let (s_ptr, s_len) = lower_value(
        subject,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
        exit_status_var,
    );
    let (p_ptr, p_len) = lower_value(
        pattern,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
        exit_status_var,
    );
    let (r_ptr, r_len) = lower_value(
        replacement,
        builder,
        module,
        vars,
        literals,
        func_constants,
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
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let (s_ptr, s_len) = lower_value(
        subject,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
        exit_status_var,
    );
    let (p_ptr, p_len) = lower_value(
        pattern,
        builder,
        module,
        vars,
        literals,
        func_constants,
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
    // Strip numeric SSA version suffix to get base name.
    let base = if let Some(idx) = key.rfind('_') {
        let suffix = &key[idx + 1..];
        if suffix.chars().all(|c| c.is_ascii_digit()) {
            &key[..idx]
        } else {
            key.as_str()
        }
    } else {
        key.as_str()
    };
    if base == "?" {
        if let Some(esv) = exit_status_var {
            let status = builder.use_var(esv);
            return emit_int_to_str(builder, module, externs, status);
        }
    }
    if base == "$" {
        let getpid_ref = module.declare_func_in_func(externs.getpid, builder.func);
        let call = builder.ins().call(getpid_ref, &[]);
        let pid = builder.inst_results(call)[0];
        return emit_int_to_str(builder, module, externs, pid);
    }
    let sv = match vars.get(&key) {
        Some(v) => v,
        None => {
            let zero = builder.ins().iconst(I64, 0);
            return (zero, zero);
        }
    };
    if sv.is_int() {
        let int_val = builder.use_var(*sv.as_int());
        emit_int_to_str(builder, module, externs, int_val)
    } else {
        let ptr = builder.use_var(sv.str_ptr());
        let len = builder.use_var(sv.str_len());
        (ptr, len)
    }
}

fn lower_func_call_value(
    name: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    if let Some(constant) = func_constants.get(name) {
        let data_id = literals.get(constant.as_str()).unwrap_or_else(|| {
            panic!("ICE: constant-folded string not interned: {:?}", constant);
        });
        let gv = module.declare_data_in_func(*data_id, builder.func);
        let ptr = builder.ins().global_value(I64, gv);
        let len = builder.ins().iconst(I64, constant.len() as i64);
        (ptr, len)
    } else {
        panic!(
            "ICE: non-constant elidable function call not yet supported: {}",
            name
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_concat_value(
    parts: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
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
        func_constants,
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
            func_constants,
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
    func_constants: &HashMap<String, String>,
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
        let key_data = literals.get(key.as_str()).unwrap_or_else(|| {
            panic!("ICE: assoc array key not interned: {}", key);
        });
        let key_gv = module.declare_data_in_func(*key_data, builder.func);
        let key_ptr = builder.ins().global_value(I64, key_gv);
        let key_len = builder.ins().iconst(I64, key.len() as i64);
        let (val_ptr, val_len) = lower_value(
            val_ssa,
            builder,
            module,
            vars,
            literals,
            func_constants,
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
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
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
            func_constants,
            externs,
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
            func_constants,
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

fn lower_argv_index_value(
    idx_ref: &VarRef,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let sv = vars
        .get(&idx_ref.display())
        .unwrap_or_else(|| panic!("ICE: undefined index var: {}", idx_ref.display()));
    let idx_ptr = builder.use_var(sv.str_ptr());
    let idx_len = builder.use_var(sv.str_len());
    let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
    let call = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
    let idx_int = builder.inst_results(call)[0];
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
    let idx_int = lookup_atoi_var(index, builder, module, vars, externs);
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
    let idx_int = lookup_atoi_var(index, builder, module, vars, externs);
    let (out_slot, out_addr) = alloc_out_slot(builder);
    let tk_ref = module.declare_func_in_func(externs.table_key, builder.func);
    let call = builder.ins().call(tk_ref, &[table_ptr, idx_int, out_addr]);
    let key_ptr = builder.inst_results(call)[0];
    let key_len = builder.ins().stack_load(I64, out_slot, 0);
    (key_ptr, key_len)
}

/// Load a string var by VarRef and run it through sash_atoi.
fn lookup_atoi_var(
    index: &VarRef,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    let idx_sv = vars
        .get(&index.display())
        .unwrap_or_else(|| panic!("ICE: undefined index var: {}", index.display()));
    let idx_ptr = builder.use_var(idx_sv.str_ptr());
    let idx_len = builder.use_var(idx_sv.str_len());
    let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
    let call = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
    builder.inst_results(call)[0]
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
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let cond_int = lower_value_as_int(
        cond,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
    );
    let zero = builder.ins().iconst(I64, 0);
    let is_true = builder.ins().icmp(IntCC::NotEqual, cond_int, zero);
    let (t_ptr, t_len) = lower_value(
        true_val,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
        exit_status_var,
    );
    let (f_ptr, f_len) = lower_value(
        false_val,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
        exit_status_var,
    );
    let r_ptr = builder.ins().select(is_true, t_ptr, f_ptr);
    let r_len = builder.ins().select(is_true, t_len, f_len);
    (r_ptr, r_len)
}

#[allow(clippy::too_many_arguments)]
fn lower_indexed_array_value(
    elems: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let capacity = core::cmp::max(elems.len() * 2, 8) as i64;
    let cap_val = builder.ins().iconst(I64, capacity);
    let new_ref = module.declare_func_in_func(externs.table_new, builder.func);
    let call = builder.ins().call(new_ref, &[cap_val]);
    let table_ptr = builder.inst_results(call)[0];

    let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
    for (i, val_ssa) in elems.iter().enumerate() {
        let (idx_slot, key_ptr) = alloc_out_slot(builder);
        let idx_val = builder.ins().iconst(I64, i as i64);
        builder.ins().stack_store(idx_val, idx_slot, 0);
        let key_len = builder.ins().iconst(I64, 8);
        let (val_ptr, val_len) = lower_value(
            val_ssa,
            builder,
            module,
            vars,
            literals,
            func_constants,
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

/// Emit Cranelift instructions for an ArithOp on two i64 values.
pub(super) fn lower_value_as_int(
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    match value {
        SsaValue::Literal(s) | SsaValue::Expr(s) => {
            let n = s.parse::<i64>().unwrap_or(0);
            builder.ins().iconst(I64, n)
        }
        SsaValue::Var(vref) => {
            let key = vref.display();
            let sv = match vars.get(&key) {
                Some(v) => v,
                None => {
                    // Undefined variable → 0 in int context
                    return builder.ins().iconst(I64, 0);
                }
            };
            if sv.is_int() {
                builder.use_var(*sv.as_int())
            } else {
                // String var in int context: atoi
                let ptr = builder.use_var(sv.str_ptr());
                let len = builder.use_var(sv.str_len());
                let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                let call = builder.ins().call(atoi_ref, &[ptr, len]);
                builder.inst_results(call)[0]
            }
        }
        SsaValue::ArithExpr { var, op, operand } => {
            let var_val = lower_value_as_int(
                &SsaValue::Var(var.clone()),
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            let rhs = builder.ins().iconst(I64, *operand);
            emit_arith_op(*op, var_val, rhs, builder, module, externs, None)
        }
        SsaValue::ArithBinOp { left, op, right } => {
            let l = lower_value_as_int(
                left,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            let r = lower_value_as_int(
                right,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            emit_arith_op(*op, l, r, builder, module, externs, None)
        }
        SsaValue::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            let cond_int = lower_value_as_int(
                cond,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            let zero = builder.ins().iconst(I64, 0);
            let is_true = builder.ins().icmp(IntCC::NotEqual, cond_int, zero);
            let t = lower_value_as_int(
                true_val,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            let f = lower_value_as_int(
                false_val,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
            );
            builder.ins().select(is_true, t, f)
        }
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
        _ => {
            // Fallback: lower as string, atoi
            let (ptr, len) = lower_value(
                value,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                None,
            );
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[ptr, len]);
            builder.inst_results(call)[0]
        }
    }
}

/// If `name` is a positional ($1, $2, ...) or special var ($#, $@, $0),
/// insert it into `refs` for later runtime emission.
fn maybe_insert_positional(name: &str, refs: &mut std::collections::HashSet<String>) {
    if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
        refs.insert(name.to_string());
    }
}

/// Scan an SsaValue for variable references that might be positional or env vars
pub(super) fn scan_value_for_vars(val: &SsaValue, refs: &mut std::collections::HashSet<String>) {
    match val {
        SsaValue::Var(vref) if vref.version == 0 => maybe_insert_positional(&vref.name, refs),
        SsaValue::ParamExpansion { var, .. } if var.version == 0 => {
            maybe_insert_positional(&var.name, refs)
        }
        SsaValue::ArithExpr { var, .. } if var.version == 0 => {
            maybe_insert_positional(&var.name, refs)
        }
        SsaValue::DefaultValue { var, default } => {
            if var.version == 0 {
                maybe_insert_positional(&var.name, refs);
            }
            scan_value_for_vars(default, refs);
        }
        other => scan_composite_children(other, refs),
    }
}

/// Recurse into the child values of composite SsaValues. Split out of
/// `scan_value_for_vars` to keep that dispatcher under the complexity gate;
/// behavior is identical (ArithBinOp/Ternary intentionally not recursed, as
/// before).
fn scan_composite_children(val: &SsaValue, refs: &mut std::collections::HashSet<String>) {
    match val {
        SsaValue::Concat(parts) | SsaValue::IndexedArray(parts) => {
            for p in parts {
                scan_value_for_vars(p, refs);
            }
        }
        SsaValue::CmdSubst { argv, .. } => {
            for a in argv {
                scan_value_for_vars(a, refs);
            }
        }
        SsaValue::AssocArray(pairs) => {
            for (_, v) in pairs {
                scan_value_for_vars(v, refs);
            }
        }
        SsaValue::Subscript { index, .. } => {
            scan_value_for_vars(index, refs);
        }
        SsaValue::RegexReplace {
            subject,
            pattern,
            replacement,
            ..
        } => {
            scan_value_for_vars(subject, refs);
            scan_value_for_vars(pattern, refs);
            scan_value_for_vars(replacement, refs);
        }
        SsaValue::RegexMatch { subject, pattern } => {
            scan_value_for_vars(subject, refs);
            scan_value_for_vars(pattern, refs);
        }
        SsaValue::CaseConvert { subject, .. } => {
            scan_value_for_vars(subject, refs);
        }
        _ => {}
    }
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

pub(super) fn scan_test_for_vars(test: &SsaTest, refs: &mut std::collections::HashSet<String>) {
    match test {
        SsaTest::Binary { left, right, .. } => {
            scan_value_for_vars(left, refs);
            if let SsaTestRhs::Value(v) = right {
                scan_value_for_vars(v, refs);
            }
        }
        SsaTest::Unary { operand, .. } => {
            scan_value_for_vars(operand, refs);
        }
        SsaTest::Not(inner) => {
            scan_test_for_vars(inner, refs);
        }
        SsaTest::And(a, b) | SsaTest::Or(a, b) => {
            scan_test_for_vars(a, refs);
            scan_test_for_vars(b, refs);
        }
    }
}
