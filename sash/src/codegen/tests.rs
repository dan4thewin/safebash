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
use super::values::{atoi_call, lower_literal_value, lower_value, lower_value_as_int};
use crate::ssa::{SsaTest, SsaTestRhs, SsaValue};

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

/// Lower a `[[ ]]` test (operands pre-lowered to `SsaValue`) to a
/// Cranelift i64 value (1=true, 0=false).
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_test_expr(
    test: &SsaTest,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    match test {
        SsaTest::Binary { left, op, right } => lower_test_binary(
            left,
            op,
            right,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
            exit_status_var,
        ),
        SsaTest::Unary { op, operand } => lower_test_unary(
            op.as_str(),
            operand,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
            exit_status_var,
        ),
        SsaTest::Not(inner) => {
            let val = lower_test_expr(
                inner,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                exit_status_var,
            );
            let one = builder.ins().iconst(I64, 1);
            builder.ins().isub(one, val)
        }
        SsaTest::And(a, b) => {
            let va = lower_test_expr(
                a,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                exit_status_var,
            );
            let vb = lower_test_expr(
                b,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                exit_status_var,
            );
            builder.ins().band(va, vb)
        }
        SsaTest::Or(a, b) => {
            let va = lower_test_expr(
                a,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                exit_status_var,
            );
            let vb = lower_test_expr(
                b,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                exit_status_var,
            );
            builder.ins().bor(va, vb)
        }
        SsaTest::Paren(inner) => lower_test_expr(
            inner,
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

#[allow(clippy::too_many_arguments)]
fn lower_test_unary(
    op: &str,
    operand: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    let (ptr, len) = lower_value(
        operand,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
        exit_status_var,
    );
    match op {
        // Implicit -n (bare word in [[ ]]) shares the -n path.
        "-n" | "?" => emit_len_cmp(builder, len, IntCC::SignedGreaterThan),
        "-z" => emit_len_cmp(builder, len, IntCC::Equal),
        "-t" => emit_isatty_test(builder, module, externs, ptr, len),
        other => match file_test_op_code(other) {
            Some(code) => emit_file_test(builder, module, externs, ptr, len, code),
            None => panic!("ICE: unimplemented unary test operator: {}", other),
        },
    }
}

/// 1 if `len <cc> 0`, else 0. Used for -n, -z, and implicit-? bare-word.
fn emit_len_cmp(
    builder: &mut FunctionBuilder,
    len: cranelift_codegen::ir::Value,
    cc: IntCC,
) -> cranelift_codegen::ir::Value {
    let zero = builder.ins().iconst(I64, 0);
    let cmp = builder.ins().icmp(cc, len, zero);
    builder.ins().uextend(I64, cmp)
}

/// stat(2)-style file test ops → opaque opcode passed to runtime
/// `sash_file_test(ptr, len, op_code)`. Returns None for non-file-test ops.
fn file_test_op_code(op: &str) -> Option<i64> {
    Some(match op {
        "-e" | "-a" => 0, // -a is same as -e
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
        _ => return None,
    })
}

fn emit_file_test(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    ptr: cranelift_codegen::ir::Value,
    len: cranelift_codegen::ir::Value,
    op_code: i64,
) -> cranelift_codegen::ir::Value {
    let op_val = builder.ins().iconst(I64, op_code);
    let ft_ref = module.declare_func_in_func(externs.file_test, builder.func);
    let call = builder.ins().call(ft_ref, &[ptr, len, op_val]);
    builder.inst_results(call)[0]
}

/// `-t fd`: atoi(ptr,len) → isatty → 1/0.
fn emit_isatty_test(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    ptr: cranelift_codegen::ir::Value,
    len: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let fd_val = atoi_call(builder, module, externs, ptr, len);
    let isatty_ref = module.declare_func_in_func(externs.isatty, builder.func);
    let call2 = builder.ins().call(isatty_ref, &[fd_val]);
    builder.inst_results(call2)[0]
}

/// Lower a Binary test's operands to `(l_ptr, l_len, r_ptr, r_len)`.
/// DD#62: operands are pre-lowered `SsaValue`s (the right side may be a
/// literal regex/glob `Pattern`). All comparisons consume `(ptr, len)`.
#[allow(clippy::too_many_arguments)]
fn lower_pair(
    left: &SsaValue,
    right: &SsaTestRhs,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> (
    cranelift_codegen::ir::Value,
    cranelift_codegen::ir::Value,
    cranelift_codegen::ir::Value,
    cranelift_codegen::ir::Value,
) {
    let (l_ptr, l_len) = lower_value(
        left,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
        exit_status_var,
    );
    let (r_ptr, r_len) = match right {
        SsaTestRhs::Value(v) => lower_value(
            v,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
            exit_status_var,
        ),
        SsaTestRhs::Pattern(p) => lower_literal_value(p, builder, module, literals),
    };
    (l_ptr, l_len, r_ptr, r_len)
}

#[allow(clippy::too_many_arguments)]
fn lower_test_binary(
    left: &SsaValue,
    op: &str,
    right: &SsaTestRhs,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    // Numeric comparisons compare operands as integers, not strings. These
    // reach codegen via compiler-synthesized loop-bound tests (e.g. for-in
    // lowers `__ai -lt ${arr arrlen}`); user-written numeric ops in `[[ ]]`
    // are rejected earlier by SB5019, but the synthetic ones are not.
    if let Some(cc) = numeric_cc(op) {
        let r_val = match right {
            SsaTestRhs::Value(v) => v,
            SsaTestRhs::Pattern(_) => {
                panic!("ICE: numeric test operator {} with a pattern operand", op)
            }
        };
        let l_int = lower_value_as_int(
            left,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
        );
        let r_int = lower_value_as_int(
            r_val,
            builder,
            module,
            vars,
            literals,
            func_constants,
            externs,
        );
        let cmp = builder.ins().icmp(cc, l_int, r_int);
        return builder.ins().uextend(I64, cmp);
    }
    let (l_ptr, l_len, r_ptr, r_len) = lower_pair(
        left,
        right,
        builder,
        module,
        vars,
        literals,
        func_constants,
        externs,
        exit_status_var,
    );
    match op {
        // DD#62: `==`/`=` are exact string equality — no glob. Pattern
        // matching is via `=~`.
        "=" | "==" => emit_string_eq(builder, l_ptr, l_len, r_ptr, r_len),
        // DD#62: `!=` is exact string inequality — no glob.
        "!=" => {
            let matched = emit_string_eq(builder, l_ptr, l_len, r_ptr, r_len);
            let one = builder.ins().iconst(I64, 1);
            builder.ins().isub(one, matched)
        }
        // Case-dispatch glob (injected by lowering): fnmatch(pattern,
        // string). Note the arg swap — pattern is the right operand.
        "=glob" => {
            let fnmatch_ref = module.declare_func_in_func(externs.fnmatch, builder.func);
            let call = builder
                .ins()
                .call(fnmatch_ref, &[r_ptr, r_len, l_ptr, l_len]);
            builder.inst_results(call)[0]
        }
        // DD#62: `=~` RHS is always a regex — quoting is grouping, not
        // literalization.
        "=~" => {
            let regex_ref = module.declare_func_in_func(externs.regex_match, builder.func);
            let call = builder.ins().call(regex_ref, &[l_ptr, l_len, r_ptr, r_len]);
            builder.inst_results(call)[0]
        }
        ">" | "<" => {
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
        "-nt" => {
            let fn_ref = module.declare_func_in_func(externs.file_newer, builder.func);
            let call = builder.ins().call(fn_ref, &[l_ptr, l_len, r_ptr, r_len]);
            builder.inst_results(call)[0]
        }
        "-ot" => {
            // Swap args: file2 -nt file1.
            let fn_ref = module.declare_func_in_func(externs.file_newer, builder.func);
            let call = builder.ins().call(fn_ref, &[r_ptr, r_len, l_ptr, l_len]);
            builder.inst_results(call)[0]
        }
        "-ef" => {
            let fn_ref = module.declare_func_in_func(externs.file_same_dev, builder.func);
            let call = builder.ins().call(fn_ref, &[l_ptr, l_len, r_ptr, r_len]);
            builder.inst_results(call)[0]
        }
        // `=glob`/`=~`/`==`/`!=`/`<`/`>`/file ops are handled above; any
        // other operator reaching codegen is a compiler bug.
        other => panic!("ICE: unimplemented test operator: {}", other),
    }
}

/// Condition code for a numeric `[[ ]]` comparison operator, or `None`
/// if `op` is not a numeric comparison. Only the dash forms occur — the
/// parser never emits `>=`/`<=` as test operators.
fn numeric_cc(op: &str) -> Option<IntCC> {
    Some(match op {
        "-eq" => IntCC::Equal,
        "-ne" => IntCC::NotEqual,
        "-lt" => IntCC::SignedLessThan,
        "-le" => IntCC::SignedLessThanOrEqual,
        "-gt" => IntCC::SignedGreaterThan,
        "-ge" => IntCC::SignedGreaterThanOrEqual,
        _ => return None,
    })
}
