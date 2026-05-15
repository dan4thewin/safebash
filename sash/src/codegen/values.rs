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
use super::value_handlers::{handle_cmdsub, handle_param_expansion};
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
