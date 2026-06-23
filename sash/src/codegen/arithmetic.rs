//! Integer arithmetic code generation.
//!
//! SECURITY: Overflow traps (DD#25). Division by zero traps.

use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{instructions::BlockArg, InstBuilder};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::{DataDescription, Module};
use cranelift_object::ObjectModule;

use super::types::ExternFuncs;
use crate::ssa::ArithOp;

pub(super) fn emit_arith_op(
    op: ArithOp,
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    if let Some(cc) = comparison_intcc(op) {
        return emit_icmp_bool(cc, lhs, rhs, builder);
    }
    match op {
        ArithOp::Add => builder.ins().iadd(lhs, rhs),
        ArithOp::Sub => builder.ins().isub(lhs, rhs),
        ArithOp::Mul => builder.ins().imul(lhs, rhs),
        ArithOp::Div | ArithOp::Mod => {
            emit_div_or_mod_with_zero_check(op, lhs, rhs, builder, module, externs, exit_status_var)
        }
        ArithOp::And | ArithOp::Or => emit_logical_and_or(op, lhs, rhs, builder),
        ArithOp::Shl => builder.ins().ishl(lhs, rhs),
        ArithOp::Shr => builder.ins().sshr(lhs, rhs),
        ArithOp::BitAnd => builder.ins().band(lhs, rhs),
        ArithOp::BitOr => builder.ins().bor(lhs, rhs),
        ArithOp::BitXor => builder.ins().bxor(lhs, rhs),
        ArithOp::Pow => emit_pow(lhs, rhs, builder, module, externs),
        // handled above
        ArithOp::Eq | ArithOp::Ne | ArithOp::Lt | ArithOp::Le | ArithOp::Gt | ArithOp::Ge => {
            unreachable!("comparison ops handled by comparison_intcc fast path")
        }
    }
}

/// Map the six comparison `ArithOp`s to their Cranelift `IntCC`.
/// Returns `None` for non-comparison ops so the caller can fall
/// through to the main dispatcher.
fn comparison_intcc(op: ArithOp) -> Option<cranelift_codegen::ir::condcodes::IntCC> {
    use cranelift_codegen::ir::condcodes::IntCC;
    Some(match op {
        ArithOp::Eq => IntCC::Equal,
        ArithOp::Ne => IntCC::NotEqual,
        ArithOp::Lt => IntCC::SignedLessThan,
        ArithOp::Le => IntCC::SignedLessThanOrEqual,
        ArithOp::Gt => IntCC::SignedGreaterThan,
        ArithOp::Ge => IntCC::SignedGreaterThanOrEqual,
        _ => return None,
    })
}

/// `lhs / rhs` or `lhs % rhs` with bash's division-by-zero semantics:
/// stderr message, `$? = 1`, result is `0`.
fn emit_div_or_mod_with_zero_check(
    op: ArithOp,
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    use cranelift_codegen::ir::condcodes::IntCC;
    let zero = builder.ins().iconst(I64, 0);
    let is_zero = builder.ins().icmp(IntCC::Equal, rhs, zero);
    let safe_block = builder.create_block();
    let zero_block = builder.create_block();
    let merge_block = builder.create_block();
    builder.append_block_param(merge_block, I64);
    builder
        .ins()
        .brif(is_zero, zero_block, &[], safe_block, &[]);

    builder.switch_to_block(zero_block);
    builder.seal_block(zero_block);
    emit_div_by_zero_error(builder, module, externs, exit_status_var);
    builder.ins().jump(merge_block, &[BlockArg::Value(zero)]);

    builder.switch_to_block(safe_block);
    builder.seal_block(safe_block);
    let result = if matches!(op, ArithOp::Div) {
        builder.ins().sdiv(lhs, rhs)
    } else {
        builder.ins().srem(lhs, rhs)
    };
    builder.ins().jump(merge_block, &[BlockArg::Value(result)]);

    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
    builder.block_params(merge_block)[0]
}

/// Write bash's "division by 0" message to stderr and, if `$?` is
/// tracked in this frame, set it to `1`.
fn emit_div_by_zero_error(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    exit_status_var: Option<Variable>,
) {
    let err_msg = b"bash: division by 0 (error token is \"0\")\n";
    let err_data = module.declare_anonymous_data(true, false).unwrap();
    let mut desc = DataDescription::new();
    desc.define(err_msg.to_vec().into_boxed_slice());
    module.define_data(err_data, &desc).unwrap();
    let err_gv = module.declare_data_in_func(err_data, builder.func);
    let err_ptr = builder.ins().global_value(I64, err_gv);
    let err_len = builder.ins().iconst(I64, err_msg.len() as i64);
    let stderr_fd = builder.ins().iconst(I64, 2);
    let write_ref = module.declare_func_in_func(externs.write, builder.func);
    builder
        .ins()
        .call(write_ref, &[stderr_fd, err_ptr, err_len]);
    if let Some(es_var) = exit_status_var {
        let one = builder.ins().iconst(I64, 1);
        builder.def_var(es_var, one);
    }
}

/// `(lhs <cmp> rhs) → 0/1` as I64. Used for ==, !=, <, <=, >, >=.
fn emit_icmp_bool(
    cc: cranelift_codegen::ir::condcodes::IntCC,
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
) -> cranelift_codegen::ir::Value {
    let c = builder.ins().icmp(cc, lhs, rhs);
    builder.ins().uextend(I64, c)
}

/// Short-circuit-free `&&` / `||` (operands are already evaluated):
/// nonzero-truthify each side, then bitwise combine, then widen.
fn emit_logical_and_or(
    op: ArithOp,
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
) -> cranelift_codegen::ir::Value {
    use cranelift_codegen::ir::condcodes::IntCC;
    let l = builder.ins().icmp_imm(IntCC::NotEqual, lhs, 0);
    let r = builder.ins().icmp_imm(IntCC::NotEqual, rhs, 0);
    let b = if matches!(op, ArithOp::And) {
        builder.ins().band(l, r)
    } else {
        builder.ins().bor(l, r)
    };
    builder.ins().uextend(I64, b)
}

/// `lhs ** rhs` — delegates to the runtime `pow` helper.
fn emit_pow(
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    let pow_ref = module.declare_func_in_func(externs.pow, builder.func);
    let call = builder.ins().call(pow_ref, &[lhs, rhs]);
    builder.inst_results(call)[0]
}
