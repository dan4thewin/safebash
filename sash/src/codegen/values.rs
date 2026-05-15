//! SSA value lowering: SsaValue → Cranelift IR (ptr, len) pairs.
//!
//! SECURITY: No implicit data introduced. Literals become data sections,
//! variables become register loads, concatenations call sash_concat.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::instructions::BlockArg;
use cranelift_codegen::ir::types::{I64, I8};
use cranelift_codegen::ir::{InstBuilder, MemFlags, StackSlotData, StackSlotKind, TrapCode};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::arithmetic::emit_arith_op;
use super::redirects::emit_redirects;
use super::types::{ExternFuncs, VarSlot};
use crate::ir::TestExpr;
use crate::ssa::*;

pub(super) fn lower_expr_value(
    expr: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    #[allow(unused_variables)] exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    match &expr.kind {
        crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => {
            if let Some(&data_id) = literals.get(s.as_str()) {
                let gv = module.declare_data_in_func(data_id, builder.func);
                let ptr = builder.ins().global_value(I64, gv);
                let len = builder.ins().iconst(I64, s.len() as i64);
                (ptr, len)
            } else {
                let zero = builder.ins().iconst(I64, 0);
                (zero, zero)
            }
        }
        crate::ir::ExprKind::SimpleExpansion(name) => {
            // $? — exit status
            if name == "?" {
                if let Some(esv) = exit_status_var {
                    let status = builder.use_var(esv);
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call = builder.ins().call(itoa_ref, &[status, out_addr]);
                    let res_ptr = builder.inst_results(call)[0];
                    let res_len = builder.ins().stack_load(I64, out_slot, 0);
                    return (res_ptr, res_len);
                }
            }
            // $$ — process ID
            if name == "$" {
                let getpid_ref = module.declare_func_in_func(externs.getpid, builder.func);
                let call = builder.ins().call(getpid_ref, &[]);
                let pid = builder.inst_results(call)[0];
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call2 = builder.ins().call(itoa_ref, &[pid, out_addr]);
                let res_ptr = builder.inst_results(call2)[0];
                let res_len = builder.ins().stack_load(I64, out_slot, 0);
                return (res_ptr, res_len);
            }
            // Search vars map for the highest version of this variable
            // This finds phi-merged versions (x_3) as well as regular ones (x_1)
            let mut best_key = None;
            let mut best_ver: Option<u32> = None;
            for key in vars.keys() {
                if let Some(rest) = key.strip_prefix(name.as_str()) {
                    if let Some(ver_str) = rest.strip_prefix('_') {
                        if let Ok(v) = ver_str.parse::<u32>() {
                            if best_ver.is_none() || v > best_ver.unwrap() {
                                best_ver = Some(v);
                                best_key = Some(key.clone());
                            }
                        }
                    }
                }
            }
            if let Some(key) = best_key {
                let sv = &vars[&key];
                if sv.is_int() {
                    let int_val = builder.use_var(*sv.as_int());
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call = builder.ins().call(itoa_ref, &[int_val, out_addr]);
                    let res_ptr = builder.inst_results(call)[0];
                    let res_len = builder.ins().stack_load(I64, out_slot, 0);
                    (res_ptr, res_len)
                } else {
                    let ptr = builder.use_var(sv.str_ptr());
                    let len = builder.use_var(sv.str_len());
                    (ptr, len)
                }
            } else {
                let zero = builder.ins().iconst(I64, 0);
                (zero, zero)
            }
        }
        crate::ir::ExprKind::DoubleQuoted(parts) | crate::ir::ExprKind::Concat(parts) => {
            if parts.len() == 1 {
                lower_expr_value(
                    &parts[0],
                    builder,
                    module,
                    vars,
                    literals,
                    externs,
                    exit_status_var,
                )
            } else if parts.len() >= 2 {
                // Multi-part: concat all parts using sash_concat(a_ptr, a_len, b_ptr, b_len, out_len_ptr) -> ptr
                let (mut acc_ptr, mut acc_len) = lower_expr_value(
                    &parts[0],
                    builder,
                    module,
                    vars,
                    literals,
                    externs,
                    exit_status_var,
                );
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let concat_ref = module.declare_func_in_func(externs.concat, builder.func);
                for part in &parts[1..] {
                    let (p2, l2) = lower_expr_value(
                        part,
                        builder,
                        module,
                        vars,
                        literals,
                        externs,
                        exit_status_var,
                    );
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let call = builder
                        .ins()
                        .call(concat_ref, &[acc_ptr, acc_len, p2, l2, out_addr]);
                    acc_ptr = builder.inst_results(call)[0];
                    acc_len = builder.ins().stack_load(I64, out_slot, 0);
                }
                (acc_ptr, acc_len)
            } else {
                let zero = builder.ins().iconst(I64, 0);
                (zero, zero)
            }
        }
        crate::ir::ExprKind::ParameterExpansion(pe) => {
            if pe.operator.is_none() {
                // ${var} with no operator — same as $var (SimpleExpansion)
                let simple = crate::ir::Expr {
                    kind: crate::ir::ExprKind::SimpleExpansion(pe.name.clone()),
                    span: expr.span,
                };
                lower_expr_value(
                    &simple,
                    builder,
                    module,
                    vars,
                    literals,
                    externs,
                    exit_status_var,
                )
            } else {
                // Has operator — delegate to lower_value path via SSA
                let zero = builder.ins().iconst(I64, 0);
                (zero, zero)
            }
        }
        crate::ir::ExprKind::Subscript { name, index } => {
            // Array/map subscript: look up in table
            let array_key = format!("{}_0", name);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys()
                    .find(|k| k.starts_with(&format!("{}_", name)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array in test: {}", name))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            // Determine key type: if index is a non-numeric literal, use string key
            let use_string_key = match &index.kind {
                crate::ir::ExprKind::Literal(s) => !s.chars().all(|c| c.is_ascii_digit()),
                crate::ir::ExprKind::SimpleExpansion(_)
                | crate::ir::ExprKind::ParameterExpansion(_) => false,
                _ => false,
            };

            let (kp, kl) = if use_string_key {
                // String key: use literal bytes
                let (p, l) =
                    lower_expr_value(index, builder, module, vars, literals, externs, None);
                (p, l)
            } else {
                // Integer key
                let idx_val =
                    lower_expr_value_as_int(index, builder, module, vars, literals, externs);
                let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                builder.ins().stack_store(idx_val, idx_slot, 0);
                let p = builder.ins().stack_addr(I64, idx_slot, 0);
                let l = builder.ins().iconst(I64, 8);
                (p, l)
            };
            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let get_ref = module.declare_func_in_func(externs.table_get, builder.func);
            let call = builder.ins().call(get_ref, &[table_ptr, kp, kl, out_addr]);
            let val_ptr = builder.inst_results(call)[0];
            let val_len = builder.ins().stack_load(I64, out_slot, 0);
            (val_ptr, val_len)
        }
        _ => {
            let zero = builder.ins().iconst(I64, 0);
            (zero, zero)
        }
    }
}

/// Lower an IR Expr to an i64 value for numeric contexts.
/// Int vars are loaded directly; string vars go through atoi.

pub(super) fn lower_expr_value_as_int(
    expr: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    match &expr.kind {
        crate::ir::ExprKind::Literal(s) => builder.ins().iconst(I64, s.parse::<i64>().unwrap_or(0)),
        crate::ir::ExprKind::SimpleExpansion(name) => {
            // Find highest version of this variable
            let mut best_key = None;
            let mut best_ver: Option<u32> = None;
            for key in vars.keys() {
                if let Some(rest) = key.strip_prefix(name.as_str()) {
                    if let Some(ver_str) = rest.strip_prefix('_') {
                        if let Ok(v) = ver_str.parse::<u32>() {
                            if best_ver.is_none() || v > best_ver.unwrap() {
                                best_ver = Some(v);
                                best_key = Some(key.clone());
                            }
                        }
                    }
                }
            }
            if let Some(key) = best_key {
                let sv = &vars[&key];
                if sv.is_int() {
                    builder.use_var(*sv.as_int())
                } else {
                    let ptr = builder.use_var(sv.str_ptr());
                    let len = builder.use_var(sv.str_len());
                    let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                    let call = builder.ins().call(atoi_ref, &[ptr, len]);
                    builder.inst_results(call)[0]
                }
            } else {
                builder.ins().iconst(I64, 0)
            }
        }
        _ => {
            // Fallback: lower as string, atoi
            // Special case: ParameterExpansion (e.g., arrlen) — route through SSA value lowering
            if let crate::ir::ExprKind::ParameterExpansion(pe) = &expr.kind {
                let var = {
                    // Find highest version of this variable
                    let mut best_key = None;
                    let mut best_ver: Option<u32> = None;
                    for key in vars.keys() {
                        if let Some(rest) = key.strip_prefix(pe.name.as_str()) {
                            if let Some(ver_str) = rest.strip_prefix('_') {
                                if let Ok(v) = ver_str.parse::<u32>() {
                                    if best_ver.is_none() || v > best_ver.unwrap() {
                                        best_ver = Some(v);
                                        best_key = Some(key.clone());
                                    }
                                }
                            }
                        }
                    }
                    best_key
                };
                if let Some(ref op) = pe.operator {
                    if op == "arrlen" {
                        if let Some(key) = var {
                            let sv = &vars[&key];
                            let table_ptr = builder.use_var(sv.str_ptr());
                            let len_ref =
                                module.declare_func_in_func(externs.table_len, builder.func);
                            let call = builder.ins().call(len_ref, &[table_ptr]);
                            return builder.inst_results(call)[0];
                        }
                    }
                }
            }
            let (ptr, len) = lower_expr_value(expr, builder, module, vars, literals, externs, None);
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[ptr, len]);
            builder.inst_results(call)[0]
        }
    }
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
            let data_id = literals[s];
            let gv = module.declare_data_in_func(data_id, builder.func);
            let ptr = builder.ins().global_value(I64, gv);
            let len = builder.ins().iconst(I64, s.len() as i64);
            (ptr, len)
        }
        SsaValue::Var(vref) => {
            let key = vref.display();
            // Special: $? — read exit_status_var and itoa
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
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call = builder.ins().call(itoa_ref, &[status, out_addr]);
                    let res_ptr = builder.inst_results(call)[0];
                    let res_len = builder.ins().stack_load(I64, out_slot, 0);
                    return (res_ptr, res_len);
                }
            }
            if base == "$" {
                let getpid_ref = module.declare_func_in_func(externs.getpid, builder.func);
                let call = builder.ins().call(getpid_ref, &[]);
                let pid = builder.inst_results(call)[0];
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call2 = builder.ins().call(itoa_ref, &[pid, out_addr]);
                let res_ptr = builder.inst_results(call2)[0];
                let res_len = builder.ins().stack_load(I64, out_slot, 0);
                return (res_ptr, res_len);
            }
            let sv = match vars.get(&key) {
                Some(v) => v,
                None => {
                    // Undefined variable → empty string
                    let zero = builder.ins().iconst(I64, 0);
                    return (zero, zero);
                }
            };
            if sv.is_int() {
                // Int var used in string context: itoa
                let int_val = builder.use_var(*sv.as_int());
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call = builder.ins().call(itoa_ref, &[int_val, out_addr]);
                let res_ptr = builder.inst_results(call)[0];
                let res_len = builder.ins().stack_load(I64, out_slot, 0);
                (res_ptr, res_len)
            } else {
                let ptr = builder.use_var(sv.str_ptr());
                let len = builder.use_var(sv.str_len());
                (ptr, len)
            }
        }
        SsaValue::FuncCall(name) => {
            if let Some(constant) = func_constants.get(name.as_str()) {
                // Constant-folded: the function output is a known string
                let data_id = literals.get(constant.as_str()).unwrap_or_else(|| {
                    panic!("ICE: constant-folded string not interned: {:?}", constant);
                });
                let gv = module.declare_data_in_func(*data_id, builder.func);
                let ptr = builder.ins().global_value(I64, gv);
                let len = builder.ins().iconst(I64, constant.len() as i64);
                (ptr, len)
            } else {
                // Non-constant function — not yet supported
                panic!(
                    "ICE: non-constant elidable function call not yet supported: {}",
                    name
                );
            }
        }
        SsaValue::IntFuncCall { name, .. } => {
            panic!(
                "ICE: IntFuncCall should have been lifted during transform: {}",
                name
            );
        }
        SsaValue::Concat(parts) => {
            // Runtime string concatenation: fold left with sash_concat
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
                // Stack slot for out_len
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let concat_ref = module.declare_func_in_func(externs.concat, builder.func);
                let call = builder
                    .ins()
                    .call(concat_ref, &[acc_ptr, acc_len, p_ptr, p_len, out_addr]);
                acc_ptr = builder.inst_results(call)[0];
                acc_len = builder.ins().stack_load(I64, out_slot, 0);
            }
            (acc_ptr, acc_len)
        }
        SsaValue::ParamExpansion {
            var,
            operator,
            operand,
        } => {
            // glob: arr=(*.sh) → sash_glob(".", pattern)
            if operator == "glob" {
                let dir_str = ".";
                let dir_id = literals
                    .get(dir_str)
                    .unwrap_or_else(|| panic!("ICE: missing literal: {}", dir_str));
                let dir_gv = module.declare_data_in_func(*dir_id, builder.func);
                let dir_ptr = builder.ins().global_value(I64, dir_gv);
                let dir_len = builder.ins().iconst(I64, dir_str.len() as i64);
                let pat = operand.as_str();
                let pat_id = literals
                    .get(pat)
                    .unwrap_or_else(|| panic!("ICE: missing literal: {}", pat));
                let pat_gv = module.declare_data_in_func(*pat_id, builder.func);
                let pat_ptr = builder.ins().global_value(I64, pat_gv);
                let pat_len = builder.ins().iconst(I64, pat.len() as i64);
                let glob_ref = module.declare_func_in_func(externs.glob, builder.func);
                let call = builder
                    .ins()
                    .call(glob_ref, &[dir_ptr, dir_len, pat_ptr, pat_len]);
                let table_ptr = builder.inst_results(call)[0];
                let zero = builder.ins().iconst(I64, 0);
                return (table_ptr, zero);
            }
            if operator == "glob_regex" {
                let dir_str = ".";
                let dir_id = literals
                    .get(dir_str)
                    .unwrap_or_else(|| panic!("ICE: missing literal: {}", dir_str));
                let dir_gv = module.declare_data_in_func(*dir_id, builder.func);
                let dir_ptr = builder.ins().global_value(I64, dir_gv);
                let dir_len = builder.ins().iconst(I64, dir_str.len() as i64);
                let pat = operand.as_str();
                let pat_id = literals
                    .get(pat)
                    .unwrap_or_else(|| panic!("ICE: missing literal: {}", pat));
                let pat_gv = module.declare_data_in_func(*pat_id, builder.func);
                let pat_ptr = builder.ins().global_value(I64, pat_gv);
                let pat_len = builder.ins().iconst(I64, pat.len() as i64);
                let glob_regex_ref = module.declare_func_in_func(externs.glob_regex, builder.func);
                let call = builder
                    .ins()
                    .call(glob_regex_ref, &[dir_ptr, dir_len, pat_ptr, pat_len]);
                let table_ptr = builder.inst_results(call)[0];
                let zero = builder.ins().iconst(I64, 0);
                return (table_ptr, zero);
            }

            // Get the variable's current value
            let key = var.display();
            let (s_ptr, s_len) = if let Some(sv) = vars.get(&key) {
                (builder.use_var(sv.str_ptr()), builder.use_var(sv.str_len()))
            } else if operator == ":-" || operator == "-" || operator == ":+" || operator == "+" {
                // Unset variable with default/alternate operator → treat as empty
                let null = builder.ins().iconst(I64, 0);
                let zero = builder.ins().iconst(I64, 0);
                (null, zero)
            } else {
                // Undefined variable — treat as empty string (bash behavior)
                let null = builder.ins().iconst(I64, 0);
                let zero = builder.ins().iconst(I64, 0);
                (null, zero)
            };

            // "strlen" doesn't use an operand
            if operator == "strlen" {
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call = builder.ins().call(itoa_ref, &[s_len, out_addr]);
                let result_ptr = builder.inst_results(call)[0];
                let result_len = builder.ins().stack_load(I64, out_slot, 0);
                return (result_ptr, result_len);
            }

            // ${#arr[@]} — array length
            if operator == "arrlen" {
                // s_ptr IS the table pointer (stored in str_ptr slot for arrays)
                let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
                let call = builder.ins().call(len_ref, &[s_ptr]);
                let arr_len = builder.inst_results(call)[0];
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call2 = builder.ins().call(itoa_ref, &[arr_len, out_addr]);
                let result_ptr = builder.inst_results(call2)[0];
                let result_len = builder.ins().stack_load(I64, out_slot, 0);
                return (result_ptr, result_len);
            }

            // ${#arr[N]} — string length of element N
            if operator == "strlen_elem" {
                // Get element by integer index from the table
                let tv_ref = module.declare_func_in_func(externs.table_value, builder.func);
                let idx_val = if !operand.is_empty() {
                    builder
                        .ins()
                        .iconst(I64, operand.parse::<i64>().unwrap_or(0))
                } else {
                    builder.ins().iconst(I64, 0)
                };
                let out_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_len_addr = builder.ins().stack_addr(I64, out_len_slot, 0);
                let call = builder.ins().call(tv_ref, &[s_ptr, idx_val, out_len_addr]);
                let _elem_ptr = builder.inst_results(call)[0];
                let elem_len = builder.ins().stack_load(I64, out_len_slot, 0);
                // itoa the length
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call2 = builder.ins().call(itoa_ref, &[elem_len, out_addr]);
                let result_ptr = builder.inst_results(call2)[0];
                let result_len = builder.ins().stack_load(I64, out_slot, 0);
                return (result_ptr, result_len);
            }
            if operator == "regex_replace" {
                // operand format: "pattern\0replacement\0global"
                let parts: Vec<&str> = operand.split('\0').collect();
                let pattern = parts.first().unwrap_or(&"");
                let replacement = parts.get(1).unwrap_or(&"");
                let global_s = parts.get(2).unwrap_or(&"0");
                let global_i: i64 = if *global_s == "1" { 1 } else { 0 };

                let pat_id = literals
                    .get(*pattern)
                    .unwrap_or_else(|| panic!("ICE: missing literal: {}", pattern));
                let pat_gv = module.declare_data_in_func(*pat_id, builder.func);
                let pat_ptr = builder.ins().global_value(I64, pat_gv);
                let pat_len = builder.ins().iconst(I64, pattern.len() as i64);

                let rep_id = literals
                    .get(*replacement)
                    .unwrap_or_else(|| panic!("ICE: missing literal: {}", replacement));
                let rep_gv = module.declare_data_in_func(*rep_id, builder.func);
                let rep_ptr = builder.ins().global_value(I64, rep_gv);
                let rep_len = builder.ins().iconst(I64, replacement.len() as i64);

                let global_val = builder.ins().iconst(I64, global_i);
                let replace_ref = module.declare_func_in_func(externs.regex_replace, builder.func);
                let call = builder.ins().call(
                    replace_ref,
                    &[s_ptr, s_len, pat_ptr, pat_len, rep_ptr, rep_len, global_val],
                );
                let result_ptr = builder.inst_results(call)[0];
                let result_len = builder.inst_results(call)[1];
                return (result_ptr, result_len);
            }
            if operator == "arrkeys"
                || operator == "arrvals"
                || operator == "arrjoin"
                || operator.starts_with("arrvals_")
            {
                // Build separator-joined list of keys or values from a table.
                let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
                let call = builder.ins().call(len_ref, &[s_ptr]);
                let arr_len = builder.inst_results(call)[0];

                // Buffer for concatenated output
                let buf_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    4096,
                    3,
                ));
                let buf_ptr = builder.ins().stack_addr(I64, buf_slot, 0);
                // Track current write position
                let pos_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let zero = builder.ins().iconst(I64, 0);
                builder.ins().stack_store(zero, pos_slot, 0);

                let item_fn = if operator == "arrkeys" {
                    module.declare_func_in_func(externs.table_key, builder.func)
                } else {
                    module.declare_func_in_func(externs.table_value, builder.func)
                };

                // Separator: arrjoin uses operand, others use space
                let sep_str = if operator == "arrjoin" {
                    operand.as_str()
                } else {
                    " "
                };
                let sep_len_val = sep_str.len() as i64;
                let sep_data_id = literals.get(sep_str).unwrap_or_else(|| {
                    panic!("ICE: join separator not interned: {:?}", sep_str);
                });
                let sep_gv = module.declare_data_in_func(*sep_data_id, builder.func);
                let sep_ptr = builder.ins().global_value(I64, sep_gv);
                let sep_len = builder.ins().iconst(I64, sep_len_val);

                // Loop index variable
                let idx_var = builder.declare_var(I64);
                let zero2 = builder.ins().iconst(I64, 0);
                builder.def_var(idx_var, zero2);

                let header = builder.create_block();
                let body = builder.create_block();
                let exit = builder.create_block();
                builder.ins().jump(header, &[]);

                // Header: check i < len
                builder.switch_to_block(header);
                let i = builder.use_var(idx_var);
                let done = builder
                    .ins()
                    .icmp(IntCC::SignedGreaterThanOrEqual, i, arr_len);
                builder.ins().brif(done, exit, &[], body, &[]);

                // Body: get item, append space separator + item bytes
                builder.switch_to_block(body);
                builder.seal_block(body);

                let cur_i = builder.use_var(idx_var);

                // Insert space before all items except the first
                let need_sep = builder.ins().icmp_imm(IntCC::SignedGreaterThan, cur_i, 0);
                let sep_block = builder.create_block();
                let item_block = builder.create_block();
                builder
                    .ins()
                    .brif(need_sep, sep_block, &[], item_block, &[]);

                builder.switch_to_block(sep_block);
                builder.seal_block(sep_block);
                let pos = builder.ins().stack_load(I64, pos_slot, 0);
                let dst = builder.ins().iadd(buf_ptr, pos);
                // Copy separator bytes
                let sep_copy_idx = builder.declare_var(I64);
                let sep_zero = builder.ins().iconst(I64, 0);
                builder.def_var(sep_copy_idx, sep_zero);
                let sep_hdr = builder.create_block();
                let sep_body = builder.create_block();
                let sep_done = builder.create_block();
                builder.ins().jump(sep_hdr, &[]);

                builder.switch_to_block(sep_hdr);
                let sj = builder.use_var(sep_copy_idx);
                let sep_end = builder
                    .ins()
                    .icmp(IntCC::SignedGreaterThanOrEqual, sj, sep_len);
                builder.ins().brif(sep_end, sep_done, &[], sep_body, &[]);

                builder.switch_to_block(sep_body);
                builder.seal_block(sep_body);
                let sj2 = builder.use_var(sep_copy_idx);
                let src = builder.ins().iadd(sep_ptr, sj2);
                let b = builder.ins().load(
                    cranelift_codegen::ir::types::I8,
                    cranelift_codegen::ir::MemFlags::new(),
                    src,
                    0,
                );
                let d = builder.ins().iadd(dst, sj2);
                builder
                    .ins()
                    .store(cranelift_codegen::ir::MemFlags::new(), b, d, 0);
                let next_sj = builder.ins().iadd_imm(sj2, 1);
                builder.def_var(sep_copy_idx, next_sj);
                builder.ins().jump(sep_hdr, &[]);
                builder.seal_block(sep_hdr);

                builder.switch_to_block(sep_done);
                builder.seal_block(sep_done);
                let pos_after_sep = builder.ins().iadd(pos, sep_len);
                builder.ins().stack_store(pos_after_sep, pos_slot, 0);
                builder.ins().jump(item_block, &[]);

                // Get item ptr+len from table
                builder.switch_to_block(item_block);
                builder.seal_block(item_block);
                let cur_i2 = builder.use_var(idx_var);
                let item_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let item_len_addr = builder.ins().stack_addr(I64, item_len_slot, 0);
                let call = builder.ins().call(item_fn, &[s_ptr, cur_i2, item_len_addr]);
                let item_ptr = builder.inst_results(call)[0];
                let item_len = builder.ins().stack_load(I64, item_len_slot, 0);

                // DD#33: per-element operation (if arrvals_OP)
                let (item_ptr, item_len) = if let Some(sub_op) = operator.strip_prefix("arrvals_") {
                    let op_data_id = literals.get(operand.as_str()).unwrap_or_else(|| {
                        panic!("ICE: per-element operand not interned: {:?}", operand);
                    });
                    let op_gv = module.declare_data_in_func(*op_data_id, builder.func);
                    let op_ptr = builder.ins().global_value(I64, op_gv);
                    let op_len = builder.ins().iconst(I64, operand.len() as i64);
                    match sub_op {
                        "/" | "//" => {
                            // ${arr[@]/pat/rep} or ${arr[@]//pat/rep}
                            // operand is "regex\0replacement" (null-separated)
                            let (pat_s, rep_s) = operand.split_once('\0').unwrap_or((operand, ""));
                            let pat_did = literals.get(pat_s).unwrap_or_else(|| {
                                panic!("ICE: pattern not interned: {:?}", pat_s);
                            });
                            let rep_did = literals.get(rep_s).unwrap_or_else(|| {
                                panic!("ICE: replacement not interned: {:?}", rep_s);
                            });
                            let p_gv = module.declare_data_in_func(*pat_did, builder.func);
                            let p_ptr = builder.ins().global_value(I64, p_gv);
                            let p_len = builder.ins().iconst(I64, pat_s.len() as i64);
                            let r_gv = module.declare_data_in_func(*rep_did, builder.func);
                            let r_ptr = builder.ins().global_value(I64, r_gv);
                            let r_len = builder.ins().iconst(I64, rep_s.len() as i64);
                            let global = if sub_op == "//" {
                                builder.ins().iconst(I64, 1)
                            } else {
                                builder.ins().iconst(I64, 0)
                            };
                            let regex_ref =
                                module.declare_func_in_func(externs.regex_replace, builder.func);
                            let call2 = builder.ins().call(
                                regex_ref,
                                &[item_ptr, item_len, p_ptr, p_len, r_ptr, r_len, global],
                            );
                            let res_ptr = builder.inst_results(call2)[0];
                            let res_len = builder.inst_results(call2)[1];
                            (res_ptr, res_len)
                        }
                        "#" | "##" => {
                            // ${arr[@]#pat} or ${arr[@]##pat} — strip prefix
                            let longest = if sub_op == "##" {
                                builder.ins().iconst(I64, 1)
                            } else {
                                builder.ins().iconst(I64, 0)
                            };
                            let strip_ref =
                                module.declare_func_in_func(externs.strip_prefix, builder.func);
                            let call2 = builder
                                .ins()
                                .call(strip_ref, &[item_ptr, item_len, op_ptr, op_len, longest]);
                            let offset = builder.inst_results(call2)[0];
                            let res_ptr = builder.ins().iadd(item_ptr, offset);
                            let res_len = builder.ins().isub(item_len, offset);
                            (res_ptr, res_len)
                        }
                        "%" | "%%" => {
                            // ${arr[@]%pat} or ${arr[@]%%pat} — strip suffix
                            let longest = if sub_op == "%%" {
                                builder.ins().iconst(I64, 1)
                            } else {
                                builder.ins().iconst(I64, 0)
                            };
                            let strip_ref =
                                module.declare_func_in_func(externs.strip_suffix, builder.func);
                            let call2 = builder
                                .ins()
                                .call(strip_ref, &[item_ptr, item_len, op_ptr, op_len, longest]);
                            let res_len = builder.inst_results(call2)[0];
                            (item_ptr, res_len)
                        }
                        _ => (item_ptr, item_len),
                    }
                } else {
                    (item_ptr, item_len)
                };

                // Byte-copy loop
                let copy_idx = builder.declare_var(I64);
                let czero = builder.ins().iconst(I64, 0);
                builder.def_var(copy_idx, czero);

                let copy_hdr = builder.create_block();
                let copy_body = builder.create_block();
                let copy_done = builder.create_block();
                builder.ins().jump(copy_hdr, &[]);

                builder.switch_to_block(copy_hdr);
                let cj = builder.use_var(copy_idx);
                let copy_end = builder
                    .ins()
                    .icmp(IntCC::SignedGreaterThanOrEqual, cj, item_len);
                builder.ins().brif(copy_end, copy_done, &[], copy_body, &[]);

                builder.switch_to_block(copy_body);
                builder.seal_block(copy_body);
                let cj2 = builder.use_var(copy_idx);
                let cur_pos = builder.ins().stack_load(I64, pos_slot, 0);
                let src_addr = builder.ins().iadd(item_ptr, cj2);
                let byte = builder.ins().load(
                    cranelift_codegen::ir::types::I8,
                    cranelift_codegen::ir::MemFlags::new(),
                    src_addr,
                    0,
                );
                let dst_addr = builder.ins().iadd(buf_ptr, cur_pos);
                builder
                    .ins()
                    .store(cranelift_codegen::ir::MemFlags::new(), byte, dst_addr, 0);
                let new_pos = builder.ins().iadd_imm(cur_pos, 1);
                builder.ins().stack_store(new_pos, pos_slot, 0);
                let next_cj = builder.ins().iadd_imm(cj2, 1);
                builder.def_var(copy_idx, next_cj);
                builder.ins().jump(copy_hdr, &[]);
                builder.seal_block(copy_hdr);

                builder.switch_to_block(copy_done);
                builder.seal_block(copy_done);

                // Increment loop index
                let cur_i3 = builder.use_var(idx_var);
                let next_i = builder.ins().iadd_imm(cur_i3, 1);
                builder.def_var(idx_var, next_i);
                builder.ins().jump(header, &[]);
                builder.seal_block(header);

                builder.switch_to_block(exit);
                builder.seal_block(exit);
                let final_len = builder.ins().stack_load(I64, pos_slot, 0);
                return (buf_ptr, final_len);
            }

            // Intern the operand pattern
            let pat_data_id = literals.get(operand.as_str()).unwrap_or_else(|| {
                panic!("ICE: param expansion operand not interned: {:?}", operand);
            });
            let pat_gv = module.declare_data_in_func(*pat_data_id, builder.func);
            let pat_ptr = builder.ins().global_value(I64, pat_gv);
            let pat_len = builder.ins().iconst(I64, operand.len() as i64);

            match operator.as_str() {
                "%%" | "%" => {
                    // strip_suffix returns new_len; ptr unchanged
                    let longest = builder
                        .ins()
                        .iconst(I64, if operator == "%%" { 1 } else { 0 });
                    let func_ref = module.declare_func_in_func(externs.strip_suffix, builder.func);
                    let call = builder
                        .ins()
                        .call(func_ref, &[s_ptr, s_len, pat_ptr, pat_len, longest]);
                    let new_len = builder.inst_results(call)[0];
                    (s_ptr, new_len)
                }
                "##" | "#" => {
                    // strip_prefix returns offset; ptr advances, len shrinks
                    let longest = builder
                        .ins()
                        .iconst(I64, if operator == "##" { 1 } else { 0 });
                    let func_ref = module.declare_func_in_func(externs.strip_prefix, builder.func);
                    let call = builder
                        .ins()
                        .call(func_ref, &[s_ptr, s_len, pat_ptr, pat_len, longest]);
                    let offset = builder.inst_results(call)[0];
                    let new_ptr = builder.ins().iadd(s_ptr, offset);
                    let new_len = builder.ins().isub(s_len, offset);
                    (new_ptr, new_len)
                }
                ":-" | "-" => {
                    // default_value(s_len) returns 0 if empty, 1 if set
                    let func_ref = module.declare_func_in_func(externs.default_value, builder.func);
                    let call = builder.ins().call(func_ref, &[s_len]);
                    let is_set = builder.inst_results(call)[0];
                    // if is_set: use (s_ptr, s_len), else use (pat_ptr, pat_len)
                    let result_ptr = builder.ins().select(is_set, s_ptr, pat_ptr);
                    let result_len = builder.ins().select(is_set, s_len, pat_len);
                    (result_ptr, result_len)
                }
                ":+" | "+" => {
                    // alternate_value: if variable is set/non-empty, use operand; else empty
                    let func_ref = module.declare_func_in_func(externs.default_value, builder.func);
                    let call = builder.ins().call(func_ref, &[s_len]);
                    let is_set = builder.inst_results(call)[0];
                    // Inverted from :- — if set, use operand; if empty, use empty
                    let null = builder.ins().iconst(I64, 0);
                    let zero = builder.ins().iconst(I64, 0);
                    let result_ptr = builder.ins().select(is_set, pat_ptr, null);
                    let result_len = builder.ins().select(is_set, pat_len, zero);
                    (result_ptr, result_len)
                }
                "substring" => {
                    // ${var:offset:length} — operand is "offset" or "offset:length"
                    let parts: Vec<&str> = operand.split(':').collect();
                    let offset_val = parts[0].parse::<i64>().unwrap_or(0);
                    let offset = builder.ins().iconst(I64, offset_val);
                    let (length, has_length) = if parts.len() > 1 {
                        let l = parts[1].parse::<i64>().unwrap_or(0);
                        (builder.ins().iconst(I64, l), builder.ins().iconst(I64, 1))
                    } else {
                        (builder.ins().iconst(I64, 0), builder.ins().iconst(I64, 0))
                    };
                    let func_ref = module.declare_func_in_func(externs.substring, builder.func);
                    let call = builder
                        .ins()
                        .call(func_ref, &[s_ptr, s_len, offset, length, has_length]);
                    let results = builder.inst_results(call);
                    (results[0], results[1])
                }
                "^" | "^^" | "," | ",," => {
                    // Case conversion: allocate buffer, call runtime, return (ptr, len)
                    let buf_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        4096,
                        3,
                    ));
                    let buf_ptr = builder.ins().stack_addr(I64, buf_slot, 0);
                    let func_id = match operator.as_str() {
                        "^" => externs.toupper_first,
                        "^^" => externs.toupper_all,
                        "," => externs.tolower_first,
                        ",," => externs.tolower_all,
                        _ => unreachable!(),
                    };
                    let func_ref = module.declare_func_in_func(func_id, builder.func);
                    let call = builder.ins().call(func_ref, &[s_ptr, s_len, buf_ptr]);
                    let result_len = builder.inst_results(call)[0];
                    (buf_ptr, result_len)
                }
                "/" | "//" | "/#" | "/%" => {
                    // ${var/pat/rep} → coerce to regex_replace (DD#37)
                    // operand is "pattern\0replacement"
                    let parts: Vec<&str> = operand.splitn(2, '\0').collect();
                    let pattern = parts.first().unwrap_or(&"");
                    let replacement = parts.get(1).unwrap_or(&"");

                    let pat_id = literals
                        .get(*pattern)
                        .unwrap_or_else(|| panic!("ICE: missing literal: {}", pattern));
                    let pat_gv = module.declare_data_in_func(*pat_id, builder.func);
                    let pat_ptr = builder.ins().global_value(I64, pat_gv);
                    let pat_len = builder.ins().iconst(I64, pattern.len() as i64);

                    let rep_id = literals
                        .get(*replacement)
                        .unwrap_or_else(|| panic!("ICE: missing literal: {}", replacement));
                    let rep_gv = module.declare_data_in_func(*rep_id, builder.func);
                    let rep_ptr = builder.ins().global_value(I64, rep_gv);
                    let rep_len = builder.ins().iconst(I64, replacement.len() as i64);

                    let global_flag = if operator == "//" { 1i64 } else { 0i64 };
                    let global_val = builder.ins().iconst(I64, global_flag);

                    let func_ref = module.declare_func_in_func(externs.regex_replace, builder.func);
                    let call = builder.ins().call(
                        func_ref,
                        &[s_ptr, s_len, pat_ptr, pat_len, rep_ptr, rep_len, global_val],
                    );
                    let result_ptr = builder.inst_results(call)[0];
                    let result_len = builder.inst_results(call)[1];
                    (result_ptr, result_len)
                }
                _ => {
                    panic!(
                        "ICE: unsupported parameter expansion operator: {}",
                        operator
                    );
                }
            }
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
            // Convert back to string via itoa
            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
            let call2 = builder.ins().call(itoa_ref, &[result, out_addr]);
            let res_ptr = builder.inst_results(call2)[0];
            let res_len = builder.ins().stack_load(I64, out_slot, 0);
            (res_ptr, res_len)
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
            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
            let call2 = builder.ins().call(itoa_ref, &[result, out_addr]);
            let res_ptr = builder.inst_results(call2)[0];
            let res_len = builder.ins().stack_load(I64, out_slot, 0);
            (res_ptr, res_len)
        }
        SsaValue::CmdSubst {
            argv: argv_values,
            redirects,
        } => {
            // Builtin command substitutions — no fork/exec needed
            if let Some(SsaValue::Literal(name)) = argv_values.first() {
                match name.as_str() {
                    "epochseconds" => {
                        let func_ref =
                            module.declare_func_in_func(externs.epochseconds, builder.func);
                        let call = builder.ins().call(func_ref, &[]);
                        let ptr = builder.inst_results(call)[0];
                        let len = builder.inst_results(call)[1];
                        return (ptr, len);
                    }
                    "getppid" => {
                        // $(getppid) → parent PID as string
                        let func_ref = module.declare_func_in_func(externs.getppid, builder.func);
                        let call = builder.ins().call(func_ref, &[]);
                        let pid = builder.inst_results(call)[0];
                        let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                        let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                        let call2 = builder.ins().call(itoa_ref, &[pid, out_addr]);
                        let ptr = builder.inst_results(call2)[0];
                        let len = builder.ins().stack_load(I64, out_slot, 0);
                        return (ptr, len);
                    }
                    "getcwd" => {
                        // $(getcwd) → current working directory
                        let func_ref = module.declare_func_in_func(externs.getcwd, builder.func);
                        let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                        let call = builder.ins().call(func_ref, &[out_addr]);
                        let ptr = builder.inst_results(call)[0];
                        let len = builder.ins().stack_load(I64, out_slot, 0);
                        return (ptr, len);
                    }
                    "getenv" if argv_values.len() >= 2 => {
                        // $(getenv NAME) — look up environment variable
                        let (name_ptr, name_len) = lower_value(
                            &argv_values[1],
                            builder,
                            module,
                            vars,
                            literals,
                            func_constants,
                            externs,
                            exit_status_var,
                        );
                        let out_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        let out_len_addr = builder.ins().stack_addr(I64, out_len_slot, 0);
                        let getenv_ref = module.declare_func_in_func(externs.getenv, builder.func);
                        let call = builder
                            .ins()
                            .call(getenv_ref, &[name_ptr, name_len, out_len_addr]);
                        let val_ptr = builder.inst_results(call)[0];
                        let val_len = builder.ins().stack_load(I64, out_len_slot, 0);
                        return (val_ptr, val_len);
                    }
                    "argc" => {
                        // $(argc) — return number of positional params as string
                        let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
                        let argc_base = builder.ins().global_value(I64, argc_gv);
                        let argc_val = builder.ins().load(I64, MemFlags::new(), argc_base, 0);
                        // argc includes $0, so subtract 1 for $# equivalence
                        let one = builder.ins().iconst(I64, 1);
                        let count = builder.ins().isub(argc_val, one);
                        let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                        let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                        let call = builder.ins().call(itoa_ref, &[count, out_addr]);
                        let ptr = builder.inst_results(call)[0];
                        let len = builder.ins().stack_load(I64, out_slot, 0);
                        return (ptr, len);
                    }
                    "signum" => {
                        // $(signum) — return last signal number as string
                        let signum_gv = module.declare_data_in_func(externs.signum, builder.func);
                        let signum_base = builder.ins().global_value(I64, signum_gv);
                        let signum_val = builder.ins().load(I64, MemFlags::new(), signum_base, 0);
                        let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                        let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                        let call = builder.ins().call(itoa_ref, &[signum_val, out_addr]);
                        let ptr = builder.inst_results(call)[0];
                        let len = builder.ins().stack_load(I64, out_slot, 0);
                        return (ptr, len);
                    }
                    "argv" if argv_values.len() >= 2 => {
                        // $(argv N) — get positional parameter by 1-based index
                        // Aborts if index is out of range (like integer overflow)
                        let (idx_ptr, idx_len) = lower_value(
                            &argv_values[1],
                            builder,
                            module,
                            vars,
                            literals,
                            func_constants,
                            externs,
                            exit_status_var,
                        );
                        let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                        let call_atoi = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
                        let idx = builder.inst_results(call_atoi)[0];

                        let out_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        let out_len_addr = builder.ins().stack_addr(I64, out_len_slot, 0);
                        let argv_get_ref =
                            module.declare_func_in_func(externs.argv_get, builder.func);
                        let call = builder.ins().call(argv_get_ref, &[idx, out_len_addr]);
                        let str_ptr = builder.inst_results(call)[0];
                        let str_len = builder.ins().stack_load(I64, out_len_slot, 0);
                        return (str_ptr, str_len);
                    }
                    "dtoi" if argv_values.len() >= 3 => {
                        // $(dtoi N "str") — decimal to int
                        let (digits_ptr, digits_len) = lower_value(
                            &argv_values[1],
                            builder,
                            module,
                            vars,
                            literals,
                            func_constants,
                            externs,
                            exit_status_var,
                        );
                        let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                        let call_atoi = builder.ins().call(atoi_ref, &[digits_ptr, digits_len]);
                        let digits = builder.inst_results(call_atoi)[0];

                        let (str_ptr, str_len) = lower_value(
                            &argv_values[2],
                            builder,
                            module,
                            vars,
                            literals,
                            func_constants,
                            externs,
                            exit_status_var,
                        );
                        let dtoi_ref = module.declare_func_in_func(externs.dtoi, builder.func);
                        let call = builder.ins().call(dtoi_ref, &[digits, str_ptr, str_len]);
                        let int_val = builder.inst_results(call)[0];
                        // Convert int result to string via itoa
                        let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                        let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                        let call2 = builder.ins().call(itoa_ref, &[int_val, out_addr]);
                        let ptr = builder.inst_results(call2)[0];
                        let len = builder.ins().stack_load(I64, out_slot, 0);
                        return (ptr, len);
                    }
                    "itod" if argv_values.len() >= 3 => {
                        // $(itod N $var) — int to decimal string
                        let (digits_ptr, digits_len) = lower_value(
                            &argv_values[1],
                            builder,
                            module,
                            vars,
                            literals,
                            func_constants,
                            externs,
                            exit_status_var,
                        );
                        let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                        let call_atoi = builder.ins().call(atoi_ref, &[digits_ptr, digits_len]);
                        let digits = builder.inst_results(call_atoi)[0];

                        let (val_ptr, val_len) = lower_value(
                            &argv_values[2],
                            builder,
                            module,
                            vars,
                            literals,
                            func_constants,
                            externs,
                            exit_status_var,
                        );
                        // Convert string arg to int
                        let call_val = builder.ins().call(atoi_ref, &[val_ptr, val_len]);
                        let value = builder.inst_results(call_val)[0];

                        let out_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        let out_len_addr = builder.ins().stack_addr(I64, out_len_slot, 0);
                        let itod_ref = module.declare_func_in_func(externs.itod, builder.func);
                        let call = builder.ins().call(itod_ref, &[digits, value, out_len_addr]);
                        let ptr = builder.inst_results(call)[0];
                        let len = builder.ins().stack_load(I64, out_len_slot, 0);
                        return (ptr, len);
                    }
                    _ => {}
                }
            }
            // Runtime command substitution using composable primitives:
            //   pipe → fork → (child: close read, dup2 write→stdout, close write, [redirects], exec)
            //                  (parent: close write, read loop, close read, wait)

            // 1. Build argv on stack (null-terminated pointer array)
            let argc = argv_values.len();
            let argv_size = ((argc + 1) * 8) as u32;
            let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                argv_size,
                3,
            ));
            for (i, arg_val) in argv_values.iter().enumerate() {
                let (ptr, _) = lower_value(
                    arg_val,
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                    exit_status_var,
                );
                builder.ins().stack_store(ptr, argv_slot, (i * 8) as i32);
            }
            let null = builder.ins().iconst(I64, 0);
            builder
                .ins()
                .stack_store(null, argv_slot, (argc * 8) as i32);
            let argv_addr = builder.ins().stack_addr(I64, argv_slot, 0);

            // envp
            let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
            let envp_addr = builder.ins().global_value(I64, envp_gv);
            let envp_val = builder.ins().load(I64, MemFlags::new(), envp_addr, 0);

            // 2. Create pipe: [read_fd, write_fd]
            let pipe_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                16,
                3,
            ));
            let pipe_addr = builder.ins().stack_addr(I64, pipe_slot, 0);
            let pipe_ref = module.declare_func_in_func(externs.pipe, builder.func);
            builder.ins().call(pipe_ref, &[pipe_addr]);
            let read_fd = builder.ins().stack_load(I64, pipe_slot, 0);
            let write_fd = builder.ins().stack_load(I64, pipe_slot, 8);

            // 3. Fork
            let fork_ref = module.declare_func_in_func(externs.fork, builder.func);
            let fork_call = builder.ins().call(fork_ref, &[]);
            let pid = builder.inst_results(fork_call)[0];

            let zero = builder.ins().iconst(I64, 0);
            let is_child = builder.ins().icmp(IntCC::Equal, pid, zero);

            let child_block = builder.create_block();
            let parent_block = builder.create_block();

            builder
                .ins()
                .brif(is_child, child_block, &[], parent_block, &[]);

            // 4. Child block: redirect stdout to pipe, exec
            builder.switch_to_block(child_block);
            builder.seal_block(child_block);

            let close_ref = module.declare_func_in_func(externs.close, builder.func);
            builder.ins().call(close_ref, &[read_fd]);

            let one = builder.ins().iconst(I64, 1); // stdout fd
            let dup2_ref = module.declare_func_in_func(externs.dup2, builder.func);
            builder.ins().call(dup2_ref, &[write_fd, one]);

            builder.ins().call(close_ref, &[write_fd]);

            // Apply redirects in child (e.g. 2>/dev/null)
            if !redirects.is_empty() {
                emit_redirects(
                    redirects,
                    builder,
                    module,
                    externs,
                    literals,
                    vars,
                    exit_status_var,
                );
            }

            let exec_ref = module.declare_func_in_func(externs.exec, builder.func);
            builder.ins().call(exec_ref, &[argv_addr, envp_val]);
            builder.ins().trap(TrapCode::user(1).unwrap()); // exec never returns

            // 5. Parent block: close write end, read loop, wait
            builder.switch_to_block(parent_block);
            builder.seal_block(parent_block);

            builder.ins().call(close_ref, &[write_fd]);

            // Allocate a 64KB capture buffer on the heap
            let buf_size = builder.ins().iconst(I64, 65536);
            let alloc_ref = module.declare_func_in_func(externs.alloc, builder.func);
            let alloc_call = builder.ins().call(alloc_ref, &[buf_size]);
            let buf_ptr = builder.inst_results(alloc_call)[0];

            // Read loop
            let total_var = builder.declare_var(I64);
            let zero_total = builder.ins().iconst(I64, 0);
            builder.def_var(total_var, zero_total);

            let read_hdr = builder.create_block();
            let read_body = builder.create_block();
            let read_done = builder.create_block();

            builder.ins().jump(read_hdr, &[]);

            // Read header: call sash_read
            builder.switch_to_block(read_hdr);
            let total_val = builder.use_var(total_var);
            let remaining = builder.ins().isub(buf_size, total_val);
            let cur_ptr = builder.ins().iadd(buf_ptr, total_val);
            let read_ref = module.declare_func_in_func(externs.read, builder.func);
            let read_call = builder.ins().call(read_ref, &[read_fd, cur_ptr, remaining]);
            let n_read = builder.inst_results(read_call)[0];

            let zero_check = builder.ins().iconst(I64, 0);
            let got_data = builder
                .ins()
                .icmp(IntCC::SignedGreaterThan, n_read, zero_check);
            builder.ins().brif(got_data, read_body, &[], read_done, &[]);

            // Read body: accumulate
            builder.switch_to_block(read_body);
            builder.seal_block(read_body);
            let new_total = builder.ins().iadd(total_val, n_read);
            builder.def_var(total_var, new_total);
            builder.ins().jump(read_hdr, &[]);

            // Seal read_hdr after both predecessors (parent_block jump + read_body jump)
            builder.seal_block(read_hdr);

            // Read done: close read fd, wait, strip trailing newline
            builder.switch_to_block(read_done);
            builder.seal_block(read_done);

            builder.ins().call(close_ref, &[read_fd]);

            let wait_ref = module.declare_func_in_func(externs.wait, builder.func);
            let wait_call = builder.ins().call(wait_ref, &[pid]);
            let cmd_status = builder.inst_results(wait_call)[0];

            // DD#55: store exit status and check — abort if nonzero
            if let Some(esv) = exit_status_var {
                builder.def_var(esv, cmd_status);

                let zero_s = builder.ins().iconst(I64, 0);
                let failed = builder.ins().icmp(IntCC::NotEqual, cmd_status, zero_s);
                let abort_block = builder.create_block();
                let cont_block = builder.create_block();
                builder
                    .ins()
                    .brif(failed, abort_block, &[], cont_block, &[]);

                builder.switch_to_block(abort_block);
                builder.seal_block(abort_block);
                // TODO: DD#56 — route to END block for cmdsub abort
                let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                builder.ins().call(exit_ref, &[cmd_status]);
                builder.ins().trap(TrapCode::user(1).unwrap());

                builder.switch_to_block(cont_block);
                builder.seal_block(cont_block);
            }

            // Strip trailing newline (shell convention for $(...))
            let final_total = builder.use_var(total_var);
            let one_val = builder.ins().iconst(I64, 1);
            let last_pos = builder.ins().isub(final_total, one_val);
            let last_byte_ptr = builder.ins().iadd(buf_ptr, last_pos);
            let last_byte = builder.ins().load(
                cranelift_codegen::ir::types::I8,
                MemFlags::new(),
                last_byte_ptr,
                0,
            );
            let last_byte_i64 = builder.ins().uextend(I64, last_byte);
            let newline = builder.ins().iconst(I64, 10);
            let is_newline = builder.ins().icmp(IntCC::Equal, last_byte_i64, newline);
            let stripped_len = builder.ins().isub(final_total, one_val);
            let result_len = builder.ins().select(is_newline, stripped_len, final_total);

            // Handle empty output (total=0): don't try to strip
            let is_empty = builder.ins().icmp(IntCC::Equal, final_total, zero_total);
            let result_len = builder.ins().select(is_empty, zero_total, result_len);

            (buf_ptr, result_len)
        }
        SsaValue::AssocArray(pairs) => {
            // Create a hash table via sash_table_new, populate with sash_table_set.
            // Store table_ptr in the variable's VarSlot (ptr slot = table, len slot = unused).
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
            (table_ptr, zero) // ptr = table handle, len = unused
        }
        SsaValue::Subscript {
            array,
            index,
            is_integer_key,
        } => {
            // Look up key in the hash table via sash_table_get
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys()
                    .find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            let (key_ptr, key_len) = if *is_integer_key {
                // Integer key: convert index to i64, store on stack, use raw 8 bytes as key
                let idx_int = lower_value_as_int(
                    index,
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                );
                let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                builder.ins().stack_store(idx_int, idx_slot, 0);
                let kp = builder.ins().stack_addr(I64, idx_slot, 0);
                let kl = builder.ins().iconst(I64, 8);
                (kp, kl)
            } else {
                // String key: use string bytes directly
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

            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let get_ref = module.declare_func_in_func(externs.table_get, builder.func);
            let call = builder
                .ins()
                .call(get_ref, &[table_ptr, key_ptr, key_len, out_addr]);
            let val_ptr = builder.inst_results(call)[0];
            let val_len = builder.ins().stack_load(I64, out_slot, 0);
            (val_ptr, val_len)
        }
        SsaValue::ArgvIndex(idx_ref) => {
            // Load argv[atoi(idx) + 1]: dynamic positional param access
            let sv = vars
                .get(&idx_ref.display())
                .unwrap_or_else(|| panic!("ICE: undefined index var: {}", idx_ref.display()));
            let idx_ptr = builder.use_var(sv.str_ptr());
            let idx_len = builder.use_var(sv.str_len());
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
            let idx_int = builder.inst_results(call)[0];
            // offset = (idx + 1) * 8 — positional params are 1-indexed
            let one = builder.ins().iconst(I64, 1);
            let idx_plus_1 = builder.ins().iadd(idx_int, one);
            let eight = builder.ins().iconst(I64, 8);
            let offset = builder.ins().imul(idx_plus_1, eight);
            // Load from sash_argv
            let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
            let argv_ptr = builder.ins().global_value(I64, argv_gv);
            let argv_val = builder.ins().load(I64, MemFlags::trusted(), argv_ptr, 0);
            let arg_addr = builder.ins().iadd(argv_val, offset);
            let arg_ptr = builder.ins().load(I64, MemFlags::trusted(), arg_addr, 0);
            let arg_len = emit_strlen(builder, module, externs.strlen, arg_ptr);
            (arg_ptr, arg_len)
        }
        SsaValue::TableValue { array, index } => {
            // Look up the table pointer for the array
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys()
                    .find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            // Convert index var to i64
            let idx_sv = vars
                .get(&index.display())
                .unwrap_or_else(|| panic!("ICE: undefined index var: {}", index.display()));
            let idx_ptr = builder.use_var(idx_sv.str_ptr());
            let idx_len = builder.use_var(idx_sv.str_len());
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
            let idx_int = builder.inst_results(call)[0];

            // sash_table_value(table, index, out_len_ptr) -> val_ptr
            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let tv_ref = module.declare_func_in_func(externs.table_value, builder.func);
            let call2 = builder.ins().call(tv_ref, &[table_ptr, idx_int, out_addr]);
            let val_ptr = builder.inst_results(call2)[0];
            let val_len = builder.ins().stack_load(I64, out_slot, 0);
            (val_ptr, val_len)
        }
        SsaValue::TableKey { array, index } => {
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys()
                    .find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            let idx_sv = vars
                .get(&index.display())
                .unwrap_or_else(|| panic!("ICE: undefined index var: {}", index.display()));
            let idx_ptr = builder.use_var(idx_sv.str_ptr());
            let idx_len = builder.use_var(idx_sv.str_len());
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
            let idx_int = builder.inst_results(call)[0];

            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let tk_ref = module.declare_func_in_func(externs.table_key, builder.func);
            let call2 = builder.ins().call(tk_ref, &[table_ptr, idx_int, out_addr]);
            let key_ptr = builder.inst_results(call2)[0];
            let key_len = builder.ins().stack_load(I64, out_slot, 0);
            (key_ptr, key_len)
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
        SsaValue::IndexedArray(elems) => {
            // Create a hash table with integer keys 0, 1, 2, ...
            let capacity = core::cmp::max(elems.len() * 2, 8) as i64;
            let cap_val = builder.ins().iconst(I64, capacity);
            let new_ref = module.declare_func_in_func(externs.table_new, builder.func);
            let call = builder.ins().call(new_ref, &[cap_val]);
            let table_ptr = builder.inst_results(call)[0];

            let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
            for (i, val_ssa) in elems.iter().enumerate() {
                // Key is the raw i64 bytes of the index
                let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let idx_val = builder.ins().iconst(I64, i as i64);
                builder.ins().stack_store(idx_val, idx_slot, 0);
                let key_ptr = builder.ins().stack_addr(I64, idx_slot, 0);
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
    }
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

/// Scan an SsaValue for variable references that might be positional or env vars

pub(super) fn scan_value_for_vars(val: &SsaValue, refs: &mut std::collections::HashSet<String>) {
    match val {
        SsaValue::Var(vref) if vref.version == 0 => {
            let name = &vref.name;
            if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
                refs.insert(name.clone());
            }
        }
        SsaValue::Concat(parts) => {
            for p in parts {
                scan_value_for_vars(p, refs);
            }
        }
        SsaValue::ParamExpansion { var, .. } if var.version == 0 => {
            let name = &var.name;
            if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
                refs.insert(name.clone());
            }
        }
        SsaValue::CmdSubst { argv, .. } => {
            for a in argv {
                scan_value_for_vars(a, refs);
            }
        }
        SsaValue::ArithExpr { var, .. } if var.version == 0 => {
            let name = &var.name;
            if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
                refs.insert(name.clone());
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
        SsaValue::IndexedArray(elems) => {
            for v in elems {
                scan_value_for_vars(v, refs);
            }
        }
        SsaValue::ArgvIndex(_) => {}
        SsaValue::TableValue { .. } => {}
        SsaValue::TableKey { .. } => {}
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

pub(super) fn scan_test_for_vars(test: &TestExpr, refs: &mut std::collections::HashSet<String>) {
    fn scan_expr(expr: &crate::ir::Expr, refs: &mut std::collections::HashSet<String>) {
        match &expr.kind {
            crate::ir::ExprKind::SimpleExpansion(name) => {
                if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
                    refs.insert(name.clone());
                }
            }
            crate::ir::ExprKind::DoubleQuoted(parts) => {
                for p in parts {
                    scan_expr(p, refs);
                }
            }
            _ => {}
        }
    }
    match test {
        TestExpr::Binary { left, right, .. } => {
            scan_expr(left, refs);
            scan_expr(right, refs);
        }
        TestExpr::Unary { operand, .. } => {
            scan_expr(operand, refs);
        }
        TestExpr::Not(inner) | TestExpr::Paren(inner) => {
            scan_test_for_vars(inner, refs);
        }
        TestExpr::And(a, b) | TestExpr::Or(a, b) => {
            scan_test_for_vars(a, refs);
            scan_test_for_vars(b, refs);
        }
    }
}
