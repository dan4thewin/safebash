//! [[ ]] test expression code generation.
//!
//! SECURITY: Test expressions compile to inline comparisons. No shell
//! interpretation — [[ $x == $y ]] becomes a direct memcmp.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::{I64, I8};
use cranelift_codegen::ir::{InstBuilder, MemFlags};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::types::{ExternFuncs, VarSlot};
use super::values::{lower_expr_value, lower_expr_value_as_int};
use crate::ir::TestExpr;

pub(super) fn emit_string_eq(
    builder: &mut FunctionBuilder,
    a_ptr: cranelift_codegen::ir::Value,
    a_len: cranelift_codegen::ir::Value,
    b_ptr: cranelift_codegen::ir::Value,
    b_len: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let result_var = builder.declare_var(I64);
    let zero_init = builder.ins().iconst(I64, 0);
    builder.def_var(result_var, zero_init);

    // If lengths differ, not equal — skip to merge with result=0.
    let len_eq = builder.ins().icmp(IntCC::Equal, a_len, b_len);

    let cmp_block = builder.create_block();
    let ne_block = builder.create_block();
    let eq_block = builder.create_block();
    let merge_block = builder.create_block();

    builder.ins().brif(len_eq, cmp_block, &[], ne_block, &[]);

    // ne_block: result stays 0
    builder.switch_to_block(ne_block);
    builder.seal_block(ne_block);
    builder.ins().jump(merge_block, &[]);

    // cmp_block: byte-by-byte comparison loop
    builder.switch_to_block(cmp_block);
    builder.seal_block(cmp_block);

    let i_var = builder.declare_var(I64);
    let zero_i = builder.ins().iconst(I64, 0);
    builder.def_var(i_var, zero_i);

    let loop_hdr = builder.create_block();
    let loop_body = builder.create_block();
    let loop_ne = builder.create_block();

    builder.ins().jump(loop_hdr, &[]);

    // loop header: check i < len
    builder.switch_to_block(loop_hdr);
    let i_val = builder.use_var(i_var);
    let in_bounds = builder.ins().icmp(IntCC::SignedLessThan, i_val, a_len);
    builder.ins().brif(in_bounds, loop_body, &[], eq_block, &[]);

    // loop body: compare bytes
    builder.switch_to_block(loop_body);
    builder.seal_block(loop_body);
    let a_addr = builder.ins().iadd(a_ptr, i_val);
    let b_addr = builder.ins().iadd(b_ptr, i_val);
    let a_byte = builder.ins().load(I8, MemFlags::new(), a_addr, 0);
    let b_byte = builder.ins().load(I8, MemFlags::new(), b_addr, 0);
    let bytes_eq = builder.ins().icmp(IntCC::Equal, a_byte, b_byte);
    let i_next = builder.ins().iadd_imm(i_val, 1);
    builder.def_var(i_var, i_next);
    builder.ins().brif(bytes_eq, loop_hdr, &[], loop_ne, &[]);

    // loop_ne: bytes differed — result stays 0
    builder.switch_to_block(loop_ne);
    builder.seal_block(loop_ne);
    builder.ins().jump(merge_block, &[]);

    // eq_block: all bytes matched — set result to 1
    builder.switch_to_block(eq_block);
    let one = builder.ins().iconst(I64, 1);
    builder.def_var(result_var, one);
    builder.ins().jump(merge_block, &[]);

    // Seal blocks that have all predecessors known
    builder.seal_block(loop_hdr); // entered from cmp_block and loop_body
    builder.seal_block(eq_block); // entered from loop_hdr only

    // merge_block: read the result variable
    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
    builder.use_var(result_var)
}

/// Lower a TestExpr to a Cranelift i64 value (1=true, 0=false).
pub(super) fn lower_test_expr(
    test: &TestExpr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    #[allow(unused_variables)] exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    match test {
        TestExpr::Binary { left, op, right } => lower_test_binary(
            left,
            op,
            right,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        TestExpr::Unary { op, operand } => lower_test_unary(
            op.as_str(),
            operand,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
        TestExpr::Not(inner) => {
            let val = lower_test_expr(
                inner,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let one = builder.ins().iconst(I64, 1);
            builder.ins().isub(one, val)
        }
        TestExpr::And(a, b) => {
            let va = lower_test_expr(a, builder, module, vars, literals, externs, exit_status_var);
            let vb = lower_test_expr(b, builder, module, vars, literals, externs, exit_status_var);
            builder.ins().band(va, vb)
        }
        TestExpr::Or(a, b) => {
            let va = lower_test_expr(a, builder, module, vars, literals, externs, exit_status_var);
            let vb = lower_test_expr(b, builder, module, vars, literals, externs, exit_status_var);
            builder.ins().bor(va, vb)
        }
        TestExpr::Paren(inner) => lower_test_expr(
            inner,
            builder,
            module,
            vars,
            literals,
            externs,
            exit_status_var,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_test_unary(
    op: &str,
    operand: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    let (ptr, len) = lower_expr_value(
        operand,
        builder,
        module,
        vars,
        literals,
        externs,
        exit_status_var,
    );
    match op {
        "-n" => {
            // Non-empty string: len > 0
            let zero = builder.ins().iconst(I64, 0);
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, len, zero);
            builder.ins().uextend(I64, cmp)
        }
        "-z" => {
            // Empty string: len == 0
            let zero = builder.ins().iconst(I64, 0);
            let cmp = builder.ins().icmp(IntCC::Equal, len, zero);
            builder.ins().uextend(I64, cmp)
        }
        // Bare word in [[ ]] — implicit -n (non-empty test)
        "?" => {
            let zero = builder.ins().iconst(I64, 0);
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, len, zero);
            builder.ins().uextend(I64, cmp)
        }
        "-e" | "-f" | "-d" | "-r" | "-w" | "-x" | "-s" | "-L" | "-a" | "-c" | "-p" | "-k"
        | "-h" | "-b" | "-g" | "-u" | "-G" | "-O" | "-S" | "-N" => {
            let op_code = match op {
                "-e" | "-a" => 0i64, // -a is same as -e
                "-f" => 1,
                "-d" => 2,
                "-r" => 3,
                "-w" => 4,
                "-x" => 5,
                "-s" => 6,
                "-L" | "-h" => 7, // -h is same as -L
                "-c" => 8,
                "-p" => 9,
                "-k" => 10,
                "-b" => 11,
                "-g" => 12,
                "-u" => 13,
                "-G" => 14,
                "-O" => 15,
                "-S" => 16,
                "-N" => 17,
                _ => unreachable!(),
            };
            let op_val = builder.ins().iconst(I64, op_code);
            let ft_ref = module.declare_func_in_func(externs.file_test, builder.func);
            let call = builder.ins().call(ft_ref, &[ptr, len, op_val]);
            builder.inst_results(call)[0]
        }
        "-t" => {
            // -t fd: test if fd is a terminal
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[ptr, len]);
            let fd_val = builder.inst_results(call)[0];
            let isatty_ref = module.declare_func_in_func(externs.isatty, builder.func);
            let call2 = builder.ins().call(isatty_ref, &[fd_val]);
            builder.inst_results(call2)[0]
        }
        "-v" => {
            // Variable existence check: look up the variable by name
            // The operand is a Literal containing the variable name
            let var_name = match &operand.kind {
                crate::ir::ExprKind::Literal(s) => s.clone(),
                crate::ir::ExprKind::SimpleExpansion(s) => s.clone(),
                _ => String::new(),
            };
            if var_name.is_empty() {
                // Can't determine var name — treat as unset
                builder.ins().iconst(I64, 0)
            } else {
                // Look up all versioned names for this variable
                // If any version exists and has non-zero length, it's "set"
                let mut found = false;
                for key in vars.keys() {
                    if let Some(base) = key.rsplit('_').next_back() {
                        // key format is "name_version"
                        let base_name = if let Some(idx) = key.rfind('_') {
                            let suffix = &key[idx + 1..];
                            if suffix.chars().all(|c| c.is_ascii_digit()) {
                                &key[..idx]
                            } else {
                                key.as_str()
                            }
                        } else {
                            key.as_str()
                        };
                        let _ = base;
                        if base_name == var_name {
                            found = true;
                            break;
                        }
                    }
                }
                if found {
                    // Variable exists — check if current value is non-empty
                    // Find the highest version
                    let mut best_key = String::new();
                    let mut best_ver: i64 = -1;
                    for key in vars.keys() {
                        if let Some(idx) = key.rfind('_') {
                            let base = &key[..idx];
                            let suffix = &key[idx + 1..];
                            if base == var_name {
                                if let Ok(v) = suffix.parse::<i64>() {
                                    if v > best_ver {
                                        best_ver = v;
                                        best_key = key.clone();
                                    }
                                }
                            }
                        }
                    }
                    if !best_key.is_empty() {
                        if let Some(sv) = vars.get(&best_key) {
                            let v_len = builder.use_var(sv.str_len());
                            let zero = builder.ins().iconst(I64, 0);
                            let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, v_len, zero);
                            builder.ins().uextend(I64, cmp)
                        } else {
                            builder.ins().iconst(I64, 0)
                        }
                    } else {
                        builder.ins().iconst(I64, 0)
                    }
                } else {
                    builder.ins().iconst(I64, 0)
                }
            }
        }
        other => {
            panic!("ICE: unimplemented unary test operator: {}", other);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_test_binary(
    left: &crate::ir::Expr,
    op: &str,
    right: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    match op {
        "=" | "==" => {
            // In [[ ]] context: quoted RHS → exact match, unquoted → glob match
            let is_quoted = matches!(
                &right.kind,
                crate::ir::ExprKind::DoubleQuoted(_) | crate::ir::ExprKind::SingleQuoted(_)
            );
            let (l_ptr, l_len) = lower_expr_value(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (r_ptr, r_len) = lower_expr_value(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            if is_quoted {
                emit_string_eq(builder, l_ptr, l_len, r_ptr, r_len)
            } else {
                let fnmatch_ref = module.declare_func_in_func(externs.fnmatch, builder.func);
                let call = builder
                    .ins()
                    .call(fnmatch_ref, &[r_ptr, r_len, l_ptr, l_len]);
                builder.inst_results(call)[0]
            }
        }
        "=glob" => {
            // Case pattern glob matching: fnmatch(pattern, string)
            // Note: left=value, right=pattern (swap for fnmatch which takes pat first)
            let (s_ptr, s_len) = lower_expr_value(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (p_ptr, p_len) = lower_expr_value(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let fnmatch_ref = module.declare_func_in_func(externs.fnmatch, builder.func);
            let call = builder
                .ins()
                .call(fnmatch_ref, &[p_ptr, p_len, s_ptr, s_len]);
            builder.inst_results(call)[0]
        }
        "=~" => {
            let is_quoted = matches!(
                &right.kind,
                crate::ir::ExprKind::DoubleQuoted(_) | crate::ir::ExprKind::SingleQuoted(_)
            );
            let (l_ptr, l_len) = lower_expr_value(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (r_ptr, r_len) = lower_expr_value(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            if is_quoted {
                // Quoted regex RHS → literal string match (exact)
                emit_string_eq(builder, l_ptr, l_len, r_ptr, r_len)
            } else {
                let regex_ref = module.declare_func_in_func(externs.regex_match, builder.func);
                let call = builder.ins().call(regex_ref, &[l_ptr, l_len, r_ptr, r_len]);
                builder.inst_results(call)[0]
            }
        }
        "!=" => {
            let is_quoted = matches!(
                &right.kind,
                crate::ir::ExprKind::DoubleQuoted(_) | crate::ir::ExprKind::SingleQuoted(_)
            );
            let (l_ptr, l_len) = lower_expr_value(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (r_ptr, r_len) = lower_expr_value(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            if is_quoted {
                let eq = emit_string_eq(builder, l_ptr, l_len, r_ptr, r_len);
                let one = builder.ins().iconst(I64, 1);
                builder.ins().isub(one, eq)
            } else {
                let fnmatch_ref = module.declare_func_in_func(externs.fnmatch, builder.func);
                let call = builder
                    .ins()
                    .call(fnmatch_ref, &[r_ptr, r_len, l_ptr, l_len]);
                let matched = builder.inst_results(call)[0];
                let one = builder.ins().iconst(I64, 1);
                builder.ins().isub(one, matched)
            }
        }
        ">" | "<" => {
            // Lexicographic string comparison
            let (l_ptr, l_len) = lower_expr_value(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (r_ptr, r_len) = lower_expr_value(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let strcmp_ref = module.declare_func_in_func(externs.strcmp, builder.func);
            let call = builder
                .ins()
                .call(strcmp_ref, &[l_ptr, l_len, r_ptr, r_len]);
            let cmp_result = builder.inst_results(call)[0];
            let zero = builder.ins().iconst(I64, 0);
            let cc = if op == ">" {
                IntCC::SignedGreaterThan
            } else {
                IntCC::SignedLessThan
            };
            let cmp = builder.ins().icmp(cc, cmp_result, zero);
            builder.ins().uextend(I64, cmp)
        }
        "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" | ">=" | "<=" => {
            // Numeric comparison: use int vars directly, atoi for string vars
            let l_int = lower_expr_value_as_int(left, builder, module, vars, literals, externs);
            let r_int = lower_expr_value_as_int(right, builder, module, vars, literals, externs);
            let cc = match op {
                "-eq" | "==" => IntCC::Equal,
                "-ne" | "!=" => IntCC::NotEqual,
                "-lt" | "<" => IntCC::SignedLessThan,
                "-le" | "<=" => IntCC::SignedLessThanOrEqual,
                "-gt" | ">" => IntCC::SignedGreaterThan,
                "-ge" | ">=" => IntCC::SignedGreaterThanOrEqual,
                _ => unreachable!(),
            };
            let cmp = builder.ins().icmp(cc, l_int, r_int);
            builder.ins().uextend(I64, cmp)
        }
        "-nt" => {
            // file1 -nt file2: file1 is newer than file2
            let (l_ptr, l_len) = lower_expr_value(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (r_ptr, r_len) = lower_expr_value(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let fn_ref = module.declare_func_in_func(externs.file_newer, builder.func);
            let call = builder.ins().call(fn_ref, &[l_ptr, l_len, r_ptr, r_len]);
            builder.inst_results(call)[0]
        }
        "-ot" => {
            // file1 -ot file2: file1 is older than file2 (reverse of -nt)
            let (l_ptr, l_len) = lower_expr_value(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (r_ptr, r_len) = lower_expr_value(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let fn_ref = module.declare_func_in_func(externs.file_newer, builder.func);
            // Swap args: file2 -nt file1
            let call = builder.ins().call(fn_ref, &[r_ptr, r_len, l_ptr, l_len]);
            builder.inst_results(call)[0]
        }
        "-ef" => {
            // file1 -ef file2: same device and inode
            let (l_ptr, l_len) = lower_expr_value(
                left,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let (r_ptr, r_len) = lower_expr_value(
                right,
                builder,
                module,
                vars,
                literals,
                externs,
                exit_status_var,
            );
            let fn_ref = module.declare_func_in_func(externs.file_same_dev, builder.func);
            let call = builder.ins().call(fn_ref, &[l_ptr, l_len, r_ptr, r_len]);
            builder.inst_results(call)[0]
        }
        other => {
            panic!("ICE: unimplemented test operator: {}", other);
        }
    }
}
