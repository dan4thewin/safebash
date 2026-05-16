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
    use cranelift_codegen::ir::condcodes::IntCC;
    match op {
        ArithOp::Add => builder.ins().iadd(lhs, rhs),
        ArithOp::Sub => builder.ins().isub(lhs, rhs),
        ArithOp::Mul => builder.ins().imul(lhs, rhs),
        ArithOp::Div | ArithOp::Mod => {
            // Division by zero: bash prints error, sets $?=1, result is 0
            let zero = builder.ins().iconst(I64, 0);
            let is_zero = builder.ins().icmp(IntCC::Equal, rhs, zero);
            let safe_block = builder.create_block();
            let zero_block = builder.create_block();
            let merge_block = builder.create_block();
            builder.append_block_param(merge_block, I64);
            builder
                .ins()
                .brif(is_zero, zero_block, &[], safe_block, &[]);

            // Zero block: write error to stderr, set $?=1, result=0
            builder.switch_to_block(zero_block);
            builder.seal_block(zero_block);
            // Write "division by 0" to stderr
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
            builder.ins().jump(merge_block, &[BlockArg::Value(zero)]);

            // Safe block: do the division
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
        ArithOp::Eq => {
            let c = builder.ins().icmp(IntCC::Equal, lhs, rhs);
            builder.ins().uextend(I64, c)
        }
        ArithOp::Ne => {
            let c = builder.ins().icmp(IntCC::NotEqual, lhs, rhs);
            builder.ins().uextend(I64, c)
        }
        ArithOp::Lt => {
            let c = builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs);
            builder.ins().uextend(I64, c)
        }
        ArithOp::Le => {
            let c = builder.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs);
            builder.ins().uextend(I64, c)
        }
        ArithOp::Gt => {
            let c = builder.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs);
            builder.ins().uextend(I64, c)
        }
        ArithOp::Ge => {
            let c = builder
                .ins()
                .icmp(IntCC::SignedGreaterThanOrEqual, lhs, rhs);
            builder.ins().uextend(I64, c)
        }
        ArithOp::And => {
            let l = builder.ins().icmp_imm(IntCC::NotEqual, lhs, 0);
            let r = builder.ins().icmp_imm(IntCC::NotEqual, rhs, 0);
            let b = builder.ins().band(l, r);
            builder.ins().uextend(I64, b)
        }
        ArithOp::Or => {
            let l = builder.ins().icmp_imm(IntCC::NotEqual, lhs, 0);
            let r = builder.ins().icmp_imm(IntCC::NotEqual, rhs, 0);
            let b = builder.ins().bor(l, r);
            builder.ins().uextend(I64, b)
        }
        ArithOp::Not => panic!("ICE: unary ! in binary arithmetic context"),
        ArithOp::Shl => builder.ins().ishl(lhs, rhs),
        ArithOp::Shr => builder.ins().sshr(lhs, rhs),
        ArithOp::BitAnd => builder.ins().band(lhs, rhs),
        ArithOp::BitOr => builder.ins().bor(lhs, rhs),
        ArithOp::BitXor => builder.ins().bxor(lhs, rhs),
        ArithOp::Pow => {
            let pow_ref = module.declare_func_in_func(externs.pow, builder.func);
            let call = builder.ins().call(pow_ref, &[lhs, rhs]);
            builder.inst_results(call)[0]
        }
    }
}
