//! Extracted handlers for large SsaValue match arms in lower_value.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{InstBuilder, MemFlags, StackSlotData, StackSlotKind, TrapCode};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::redirects::emit_redirects;
use super::types::{emit_int_to_str, ExternFuncs, VarSlot};
use super::values::lower_value;
use crate::ssa::*;

/// Handle SsaValue::ParamExpansion — ${var:op:operand} forms.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_param_expansion(
    var: &VarRef,
    operator: &str,
    operand: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    _func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    _exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    // glob: arr=(*.sh) → sash_glob(".", pattern); glob_regex uses sash_glob_regex.
    if operator == "glob" || operator == "glob_regex" {
        let use_regex = operator == "glob_regex";
        return emit_glob_expansion(operand, use_regex, builder, module, literals, externs);
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
        let (result_ptr, result_len) = emit_int_to_str(builder, module, externs, s_len);
        return (result_ptr, result_len);
    }

    // ${#arr[@]} — array length
    if operator == "arrlen" {
        // s_ptr IS the table pointer (stored in str_ptr slot for arrays)
        let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
        let call = builder.ins().call(len_ref, &[s_ptr]);
        let arr_len = builder.inst_results(call)[0];
        let (result_ptr, result_len) = emit_int_to_str(builder, module, externs, arr_len);
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
        let out_len_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let out_len_addr = builder.ins().stack_addr(I64, out_len_slot, 0);
        let call = builder.ins().call(tv_ref, &[s_ptr, idx_val, out_len_addr]);
        let _elem_ptr = builder.inst_results(call)[0];
        let elem_len = builder.ins().stack_load(I64, out_len_slot, 0);
        // itoa the length
        let (result_ptr, result_len) = emit_int_to_str(builder, module, externs, elem_len);
        return (result_ptr, result_len);
    }
    if operator == "regex_replace" {
        return emit_regex_replace_op(operand, s_ptr, s_len, builder, module, literals, externs);
    }
    if operator == "arrkeys"
        || operator == "arrvals"
        || operator == "arrjoin"
        || operator.starts_with("arrvals_")
    {
        return emit_array_join_expansion(
            operator, operand, s_ptr, builder, module, literals, externs,
        );
    }

    // Intern the operand pattern
    let pat_data_id = literals.get(operand).unwrap_or_else(|| {
        panic!("ICE: param expansion operand not interned: {:?}", operand);
    });
    let pat_gv = module.declare_data_in_func(*pat_data_id, builder.func);
    let pat_ptr = builder.ins().global_value(I64, pat_gv);
    let pat_len = builder.ins().iconst(I64, operand.len() as i64);

    emit_string_op_expansion(
        operator, operand, s_ptr, s_len, pat_ptr, pat_len, builder, module, literals, externs,
    )
}

fn emit_glob_expansion(
    operand: &str,
    use_regex: bool,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let dir_str = ".";
    let dir_id = literals
        .get(dir_str)
        .unwrap_or_else(|| panic!("ICE: missing literal: {}", dir_str));
    let dir_gv = module.declare_data_in_func(*dir_id, builder.func);
    let dir_ptr = builder.ins().global_value(I64, dir_gv);
    let dir_len = builder.ins().iconst(I64, dir_str.len() as i64);
    let pat_id = literals
        .get(operand)
        .unwrap_or_else(|| panic!("ICE: missing literal: {}", operand));
    let pat_gv = module.declare_data_in_func(*pat_id, builder.func);
    let pat_ptr = builder.ins().global_value(I64, pat_gv);
    let pat_len = builder.ins().iconst(I64, operand.len() as i64);
    let func_id = if use_regex {
        externs.glob_regex
    } else {
        externs.glob
    };
    let func_ref = module.declare_func_in_func(func_id, builder.func);
    let call = builder
        .ins()
        .call(func_ref, &[dir_ptr, dir_len, pat_ptr, pat_len]);
    let table_ptr = builder.inst_results(call)[0];
    let zero = builder.ins().iconst(I64, 0);
    (table_ptr, zero)
}

#[allow(clippy::too_many_arguments)]
fn emit_regex_replace_op(
    operand: &str,
    s_ptr: cranelift_codegen::ir::Value,
    s_len: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
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
    (result_ptr, result_len)
}

#[allow(clippy::too_many_arguments)]
fn emit_array_join_expansion(
    operator: &str,
    operand: &str,
    s_ptr: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    // Build separator-joined list of keys or values from a table.
    let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
    let call = builder.ins().call(len_ref, &[s_ptr]);
    let arr_len = builder.inst_results(call)[0];

    // Buffer for concatenated output
    let buf_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 4096, 3));
    let buf_ptr = builder.ins().stack_addr(I64, buf_slot, 0);
    // Track current write position
    let pos_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let zero = builder.ins().iconst(I64, 0);
    builder.ins().stack_store(zero, pos_slot, 0);

    let item_fn = if operator == "arrkeys" {
        module.declare_func_in_func(externs.table_key, builder.func)
    } else {
        module.declare_func_in_func(externs.table_value, builder.func)
    };

    // Separator: arrjoin uses operand, others use space
    let sep_str = if operator == "arrjoin" { operand } else { " " };
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
    let item_len_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let item_len_addr = builder.ins().stack_addr(I64, item_len_slot, 0);
    let call = builder.ins().call(item_fn, &[s_ptr, cur_i2, item_len_addr]);
    let item_ptr = builder.inst_results(call)[0];
    let item_len = builder.ins().stack_load(I64, item_len_slot, 0);

    // DD#33: per-element operation (if arrvals_OP)
    let (item_ptr, item_len) = if let Some(sub_op) = operator.strip_prefix("arrvals_") {
        let op_data_id = literals.get(operand).unwrap_or_else(|| {
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
                let regex_ref = module.declare_func_in_func(externs.regex_replace, builder.func);
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
                let strip_ref = module.declare_func_in_func(externs.strip_prefix, builder.func);
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
                let strip_ref = module.declare_func_in_func(externs.strip_suffix, builder.func);
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
    (buf_ptr, final_len)
}

#[allow(clippy::too_many_arguments)]
fn emit_string_op_expansion(
    operator: &str,
    operand: &str,
    s_ptr: cranelift_codegen::ir::Value,
    s_len: cranelift_codegen::ir::Value,
    pat_ptr: cranelift_codegen::ir::Value,
    pat_len: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    match operator {
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
            let func_id = match operator {
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

/// Handle SsaValue::CmdSubst — command substitution $(cmd args...).
///
/// TRUST: Command names are resolved at compile time. Builtins (echo, printf)
/// are handled inline without fork. External commands use fork/exec.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_cmdsub(
    argv_values: &[SsaValue],
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    if let Some(SsaValue::Literal(name)) = argv_values.first() {
        if let Some(result) = try_native_intrinsic(
            name,
            argv_values,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
            exit_status_var,
        ) {
            return result;
        }
    }
    emit_cmdsub_exec(
        argv_values,
        redirects,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
        exit_status_var,
    )
}

/// Compile-time intrinsic dispatch for `$(epochseconds)`, `$(getppid)`,
/// `$(getcwd)`, `$(getenv NAME)`, `$(argc)`, `$(signum)`, `$(argv N)`,
/// `$(dtoi N "s")`, `$(itod N V)`. Returns `Some((ptr, len))` when
/// `name` matches a known intrinsic and the arity check passes; `None`
/// to fall through to the external-command exec path.
#[allow(clippy::too_many_arguments)]
fn try_native_intrinsic(
    name: &str,
    argv_values: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> Option<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> {
    match name {
        "epochseconds" => {
            let func_ref = module.declare_func_in_func(externs.epochseconds, builder.func);
            let call = builder.ins().call(func_ref, &[]);
            let ptr = builder.inst_results(call)[0];
            let len = builder.inst_results(call)[1];
            Some((ptr, len))
        }
        "getppid" => {
            let func_ref = module.declare_func_in_func(externs.getppid, builder.func);
            let call = builder.ins().call(func_ref, &[]);
            let pid = builder.inst_results(call)[0];
            Some(emit_int_to_str(builder, module, externs, pid))
        }
        "getcwd" => {
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
            Some((ptr, len))
        }
        "getenv" if argv_values.len() >= 2 => {
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
            Some((val_ptr, val_len))
        }
        "argc" => {
            let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
            let argc_base = builder.ins().global_value(I64, argc_gv);
            let argc_val = builder.ins().load(I64, MemFlags::new(), argc_base, 0);
            // argc includes $0, so subtract 1 for $# equivalence
            let one = builder.ins().iconst(I64, 1);
            let count = builder.ins().isub(argc_val, one);
            Some(emit_int_to_str(builder, module, externs, count))
        }
        "signum" => {
            let signum_gv = module.declare_data_in_func(externs.signum, builder.func);
            let signum_base = builder.ins().global_value(I64, signum_gv);
            let signum_val = builder.ins().load(I64, MemFlags::new(), signum_base, 0);
            Some(emit_int_to_str(builder, module, externs, signum_val))
        }
        "argv" if argv_values.len() >= 2 => {
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
            let argv_get_ref = module.declare_func_in_func(externs.argv_get, builder.func);
            let call = builder.ins().call(argv_get_ref, &[idx, out_len_addr]);
            let str_ptr = builder.inst_results(call)[0];
            let str_len = builder.ins().stack_load(I64, out_len_slot, 0);
            Some((str_ptr, str_len))
        }
        "dtoi" if argv_values.len() >= 3 => {
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
            Some(emit_int_to_str(builder, module, externs, int_val))
        }
        "itod" if argv_values.len() >= 3 => {
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
            Some((ptr, len))
        }
        _ => None,
    }
}

/// External `$(cmd ...)`: pipe → fork → (child: redirect stdout to
/// pipe, apply redirects, exec) ‖ (parent: read loop into a 64 KiB
/// heap buffer, wait, strip trailing newline, propagate DD#55 exit).
#[allow(clippy::too_many_arguments)]
fn emit_cmdsub_exec(
    argv_values: &[SsaValue],
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
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
    let pipe_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 3));
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
