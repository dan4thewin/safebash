//! Integer arithmetic code generation.
//!
//! SECURITY: Overflow traps (DD#25). Division by zero traps.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;
use cranelift_object::ObjectModule;

use super::types::ExternFuncs;
use crate::ssa::{ArithOp, CheckedArithOp};

pub(super) fn emit_arith_op(
    op: ArithOp,
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    match op {
        ArithOp::Add | ArithOp::Sub | ArithOp::Mul => {
            // `from_arith` is `Some` for exactly these three (the arm guard) —
            // so `emit_checked_arith` takes a `CheckedArithOp` and stays total.
            let cop = CheckedArithOp::from_arith(op).expect("checked op by arm guard");
            emit_checked_arith(cop, lhs, rhs, builder, module, externs)
        }
        ArithOp::Div | ArithOp::Mod => {
            emit_div_or_mod_with_zero_check(op, lhs, rhs, builder, module, externs)
        }
        // Both general ArithBinOp paths intercept logical ops before lowering
        // either operand. The only other caller handles ArithExpr, built at
        // four loop-increment sites that pass Add. Eager emission here would
        // run the right operand's guards even when the left decides the result.
        ArithOp::And | ArithOp::Or => {
            unreachable!("logical ops handled by lower_logical_short_circuit")
        }
        ArithOp::Shl | ArithOp::Shr => emit_shift(op, lhs, rhs, builder, module, externs),
        ArithOp::BitAnd => builder.ins().band(lhs, rhs),
        ArithOp::BitOr => builder.ins().bor(lhs, rhs),
        ArithOp::BitXor => builder.ins().bxor(lhs, rhs),
        ArithOp::Pow => emit_pow(lhs, rhs, builder, module, externs),
        ArithOp::Eq => emit_icmp_bool(IntCC::Equal, lhs, rhs, builder),
        ArithOp::Ne => emit_icmp_bool(IntCC::NotEqual, lhs, rhs, builder),
        ArithOp::Lt => emit_icmp_bool(IntCC::SignedLessThan, lhs, rhs, builder),
        ArithOp::Le => emit_icmp_bool(IntCC::SignedLessThanOrEqual, lhs, rhs, builder),
        ArithOp::Gt => emit_icmp_bool(IntCC::SignedGreaterThan, lhs, rhs, builder),
        ArithOp::Ge => emit_icmp_bool(IntCC::SignedGreaterThanOrEqual, lhs, rhs, builder),
    }
}

/// DD#25: `+`, `-`, `*` with a runtime overflow trap (not silent
/// wraparound). Uses Cranelift's signed-overflow instructions, which
/// return `(result, overflow_flag)`; when the flag is set, branch to an
/// abort block that calls `sash_arith_overflow` (writes the diagnostic and
/// exits). bug-runtime-arith-overflow-wraps.
fn emit_checked_arith(
    op: CheckedArithOp,
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    let (result, overflow) = match op {
        CheckedArithOp::Add => builder.ins().sadd_overflow(lhs, rhs),
        CheckedArithOp::Sub => builder.ins().ssub_overflow(lhs, rhs),
        CheckedArithOp::Mul => builder.ins().smul_overflow(lhs, rhs),
    };
    // `overflow` is a 1-bit flag (i8); `brif` treats any non-zero value as
    // true, so it can gate the abort branch directly.
    let abort_block = builder.create_block();
    let cont_block = builder.create_block();
    builder
        .ins()
        .brif(overflow, abort_block, &[], cont_block, &[]);

    builder.switch_to_block(abort_block);
    builder.seal_block(abort_block);
    let ovf_ref = module.declare_func_in_func(externs.arith_overflow, builder.func);
    builder.ins().call(ovf_ref, &[]);
    builder
        .ins()
        .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());

    builder.switch_to_block(cont_block);
    builder.seal_block(cont_block);
    result
}

/// `lhs / rhs` or `lhs % rhs`. DD#25 (amended): a zero divisor ABORTS —
/// `<argv0>: division by zero`, exit 2 — exactly like overflow and like
/// `i64::MIN / -1` below. It used to write bash's "division by 0" to stderr,
/// yield 0 and continue, which made it the only arithmetic fault that
/// produced a value and kept running.
fn emit_div_or_mod_with_zero_check(
    op: ArithOp,
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    use cranelift_codegen::ir::condcodes::IntCC;
    let zero = builder.ins().iconst(I64, 0);
    let is_zero = builder.ins().icmp(IntCC::Equal, rhs, zero);
    let safe_block = builder.create_block();
    let zero_block = builder.create_block();
    // The check is deliberate, not a fallback. Letting the hardware #DE fire
    // instead would report `Floating point exception` with no argv0 and a
    // misleading "floating point" for an integer divide.
    builder
        .ins()
        .brif(is_zero, zero_block, &[], safe_block, &[]);

    builder.switch_to_block(zero_block);
    builder.seal_block(zero_block);
    let dz_ref = module.declare_func_in_func(externs.arith_divzero, builder.func);
    builder.ins().call(dz_ref, &[]);
    builder
        .ins()
        .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());

    builder.switch_to_block(safe_block);
    builder.seal_block(safe_block);
    let result = if matches!(op, ArithOp::Div) {
        // DD#25: i64::MIN / -1 overflows (and SIGFPEs on x86). Trap it
        // cleanly via sash_arith_overflow before the sdiv.
        // bug-runtime-arith-overflow-wraps.
        let min = builder.ins().iconst(I64, i64::MIN);
        let neg_one = builder.ins().iconst(I64, -1);
        let lhs_is_min = builder.ins().icmp(IntCC::Equal, lhs, min);
        let rhs_is_neg_one = builder.ins().icmp(IntCC::Equal, rhs, neg_one);
        let is_overflow = builder.ins().band(lhs_is_min, rhs_is_neg_one);
        let ovf_block = builder.create_block();
        let div_ok_block = builder.create_block();
        builder
            .ins()
            .brif(is_overflow, ovf_block, &[], div_ok_block, &[]);

        builder.switch_to_block(ovf_block);
        builder.seal_block(ovf_block);
        let ovf_ref = module.declare_func_in_func(externs.arith_overflow, builder.func);
        builder.ins().call(ovf_ref, &[]);
        builder
            .ins()
            .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());

        builder.switch_to_block(div_ok_block);
        builder.seal_block(div_ok_block);
        builder.ins().sdiv(lhs, rhs)
    } else {
        builder.ins().srem(lhs, rhs)
    };
    // No merge block: the zero path aborts, so `safe_block` is the only way
    // out of this construct and its value is the result.
    result
}

/// Shifts with an **unbounded, non-negative** count (DG, 2026-09-20).
///
/// The hardware masks the shift count to 6 bits, which is a property of the
/// register width rather than of the arithmetic: it makes `1 << 64` yield `1`
/// and `8 >> -64` yield `8`, wrong answers that look right. That is the same
/// class of silent-wrong that DD#25 rejects for overflow, so shifts stop
/// inheriting it. The rule is Go's:
///
/// - **count < 0** — abort (`sash_arith_negshift`). Meaningless in any model.
/// - **count >= 64** — every bit is shifted out. `<<` yields `0`; `>>` fills
///   with the sign, so it yields `0` for a non-negative value and `-1` for a
///   negative one.
/// - otherwise — the ordinary shift.
///
/// `>>` keeps filling with the **sign**, not zeros. On a signed type that is
/// the less representation-dependent choice: `-8 >> 1` is `-4`, which is
/// `floor(-8 / 2)`, whereas a zero fill would give `9223372036854775804` and
/// is only meaningful if the sign bit is reinterpreted as a value bit.
///
/// The `>= 64` case is branchless: `<<` selects a constant `0`, and `>>`
/// clamps the count to 63, whose sign fill *is* the saturated answer.
fn emit_shift(
    op: ArithOp,
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    use cranelift_codegen::ir::condcodes::IntCC;
    let zero = builder.ins().iconst(I64, 0);
    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, rhs, zero);
    let neg_block = builder.create_block();
    let ok_block = builder.create_block();
    builder.ins().brif(is_neg, neg_block, &[], ok_block, &[]);

    builder.switch_to_block(neg_block);
    builder.seal_block(neg_block);
    let ns_ref = module.declare_func_in_func(externs.arith_negshift, builder.func);
    builder.ins().call(ns_ref, &[]);
    builder
        .ins()
        .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());

    // No merge block: the negative path aborts, so `ok_block` is the only way
    // out and its value is the result. Same shape as the zero-divisor check.
    builder.switch_to_block(ok_block);
    builder.seal_block(ok_block);
    let width = builder.ins().iconst(I64, 64);
    let too_wide = builder
        .ins()
        .icmp(IntCC::SignedGreaterThanOrEqual, rhs, width);
    if matches!(op, ArithOp::Shl) {
        let shifted = builder.ins().ishl(lhs, rhs);
        builder.ins().select(too_wide, zero, shifted)
    } else {
        let max_shift = builder.ins().iconst(I64, 63);
        let count = builder.ins().select(too_wide, max_shift, rhs);
        builder.ins().sshr(lhs, count)
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
