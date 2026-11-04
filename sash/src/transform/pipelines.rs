//! Pipeline and for-loop variant lowering.

use super::expressions::expr_to_value;
use super::helpers::{insert_phis_two_branch, update_loop_phi_sources};
use super::statements::lower_stmts;
use crate::ir::*;
use crate::ssa::*;

fn reserve_loop_int(state: &mut VersionState, name: String) -> String {
    state.reserve_temp_name(name, VarType::Int)
}

/// Emit a for-loop bound check `left < right` on the `(( ))` arithmetic
/// path: an `ArithBinOp` comparison fed to `LetExit`, which sets
/// `test_result_var` for the enclosing `Terminator::Loop`. Keeps integer
/// comparisons out of `[[ ]]` (the synthesized loop bound is arithmetic,
/// not a test). Re-run per lowering pass so operands pick up phi-merged
/// versions; defines no SSA temp, so it doesn't perturb phi insertion.
fn emit_for_bound(
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    header: BlockId,
    left: &Expr,
    right: &Expr,
) {
    let cond = SsaValue::ArithBinOp {
        left: Box::new(expr_to_value(left, state)),
        op: ArithOp::Lt,
        right: Box::new(expr_to_value(right, state)),
    };
    ssa.block_mut(header).insts.push(SsaInst::internal(
        BuiltinCmd::LetExit,
        vec![cond],
        state.suppressed == 0,
    ));
}

/// `for k in @m` binds the KEY, `for v in @a` the VALUE. Hoisted out of
/// `lower_for_array`, which emits it once per SSA pass.
///
/// DD#91: `vouched` is the `!` taint-off modifier from `for v in @{a:!}`. The
/// binding is wrapped in `Trusted`, which codegen lowers as its source — the
/// emitted code is identical and only the taint passes read it. Without the
/// node the SSA backstop sees the element read inherit the array's taint and
/// rejects a bypass the author explicitly asked for, which is the whole
/// reason `Trusted` exists.
fn loop_binding_value(use_keys: bool, array: VarRef, index: &VarRef, vouched: bool) -> SsaValue {
    let index = index.clone();
    let bind = if use_keys {
        SsaValue::TableKey { array, index }
    } else {
        SsaValue::TableValue { array, index }
    };
    if vouched {
        SsaValue::Trusted {
            source: Box::new(bind),
        }
    } else {
        bind
    }
}

pub(super) fn lower_for_array(
    f: &For,
    array_name: &str,
    use_keys: bool,
    vouched: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let header = ssa.new_checkpoint_block("for_arr.header");
    let body_block = ssa.new_block("for_arr.body");
    let latch = ssa.new_block("for_arr.latch");
    let exit_block = ssa.new_block("for_arr.exit");
    let idx_name = reserve_loop_int(state, format!("__ai{}", header));

    // __ai = 0
    let idx_var = state.define(&idx_name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: idx_var,
        value: SsaValue::Literal("0".to_string()),
    });

    ssa.block_mut(block).terminator = Terminator::Goto(header);
    let pre_state = state.snapshot();

    // Condition: __ai < ${#arr[@]}, on the (( )) arith path (see
    // emit_for_bound) — synthesized loop bounds are arithmetic, not [[ ]]
    // tests. Built once, re-emitted per pass.
    let bound_left = Expr {
        kind: ExprKind::SimpleExpansion(idx_name.clone()),
        span: Span::default(),
    };
    let bound_right = Expr::param(array_name, Some("arrlen"), None, Span::default());
    emit_for_bound(ssa, state, header, &bound_left, &bound_right);
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // x = table_value(arr, __ai)
    let idx_ref = state.lookup(&idx_name);
    let loop_var = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var,
        value: loop_binding_value(use_keys, state.lookup(array_name), &idx_ref, vouched),
    });

    state.loop_stack.push((latch, exit_block));
    let body_exit_1 = lower_stmts(&f.body, ssa, state, body_block);
    state.loop_stack.pop();
    ssa.block_mut(body_exit_1).terminator = Terminator::Goto(latch);

    // __ai = __ai + 1
    let idx_cur = state.lookup(&idx_name);
    let idx_next = state.define(&idx_name);
    ssa.block_mut(latch).insts.push(SsaInst::Assign {
        target: idx_next,
        value: SsaValue::ArithExpr {
            var: idx_cur,
            op: ArithOp::Add,
            operand: 1,
        },
    });
    ssa.block_mut(latch).terminator = Terminator::Goto(header);
    let body_state_1 = state.snapshot();

    insert_phis_two_branch(state, ssa, header, block, &pre_state, latch, &body_state_1);

    // --- Pass 2 ---
    ssa.block_mut(header).insts.clear();
    ssa.block_mut(body_block).insts.clear();
    ssa.block_mut(latch).insts.clear();

    emit_for_bound(ssa, state, header, &bound_left, &bound_right);
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    let idx_ref_2 = state.lookup(&idx_name);
    let loop_var_2 = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var_2,
        value: loop_binding_value(use_keys, state.lookup(array_name), &idx_ref_2, vouched),
    });

    state.loop_stack.push((latch, exit_block));
    let body_exit_2 = lower_stmts(&f.body, ssa, state, body_block);
    state.loop_stack.pop();
    ssa.block_mut(body_exit_2).terminator = Terminator::Goto(latch);

    let idx_cur_2 = state.lookup(&idx_name);
    let idx_next_2 = state.define(&idx_name);
    ssa.block_mut(latch).insts.push(SsaInst::Assign {
        target: idx_next_2,
        value: SsaValue::ArithExpr {
            var: idx_cur_2,
            op: ArithOp::Add,
            operand: 1,
        },
    });

    ssa.block_mut(latch).terminator = Terminator::Goto(header);

    let body_state_2 = state.snapshot();
    update_loop_phi_sources(ssa, header, &body_state_2);

    exit_block
}

/// Lower a `for x in $(argv N)` loop: iterate the program argv slice
/// argv[start..argc-1] (DD#60). `$(argv)` passes start=0 (whole array,
/// program name first); `$(argv N)` passes start=N. Out-of-range N yields
/// zero iterations (not an abort). The loop variable is bound directly to
/// argv[__fi] (0-based) and is tainted (program input).
pub(super) fn lower_for_argv_slice(
    f: &For,
    start: i64,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let header = ssa.new_checkpoint_block("for_at.header");
    let body_block = ssa.new_block("for_at.body");
    let latch = ssa.new_block("for_at.latch");
    let exit_block = ssa.new_block("for_at.exit");
    let idx_name = reserve_loop_int(state, format!("__fi{}", header));
    let argc_name = reserve_loop_int(state, format!("__argc{}", header));

    // Initialize index: __fi = start
    let idx_var = state.define(&idx_name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: idx_var,
        value: SsaValue::Literal(start.to_string()),
    });

    // Capture the loop bound once: __argc = $(argc) (C argc, loop-invariant).
    // Materialising it into a variable keeps the loop condition a simple
    // var-vs-var numeric test that the codegen test handler evaluates.
    let argc_var = state.define(&argc_name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: argc_var,
        value: SsaValue::CmdSubst {
            argv: vec![SsaValue::Literal("argc".to_string())],
            suppressed: false,
        },
    });

    ssa.block_mut(block).terminator = Terminator::Goto(header);

    let pre_state = state.snapshot();

    // --- Pass 1: discover modified variables ---
    // Condition: __fi < __argc (C argc — includes the program name), on the
    // (( )) arith path (see emit_for_bound).
    let bound_left = Expr {
        kind: ExprKind::SimpleExpansion(idx_name.clone()),
        span: Span::default(),
    };
    let bound_right = Expr {
        kind: ExprKind::SimpleExpansion(argc_name.clone()),
        span: Span::default(),
    };
    emit_for_bound(ssa, state, header, &bound_left, &bound_right);
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // Assign loop variable: x = argv[__fi]
    let idx_ref = state.lookup(&idx_name);
    let loop_var = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var,
        value: SsaValue::ArgvIndex(idx_ref.clone()),
    });

    // Lower body
    state.loop_stack.push((latch, exit_block));
    let body_exit_1 = lower_stmts(&f.body, ssa, state, body_block);
    state.loop_stack.pop();
    ssa.block_mut(body_exit_1).terminator = Terminator::Goto(latch);

    // Increment index: __fi = __fi + 1
    let idx_cur = state.lookup(&idx_name);
    let idx_next = state.define(&idx_name);
    ssa.block_mut(latch).insts.push(SsaInst::Assign {
        target: idx_next,
        value: SsaValue::ArithExpr {
            var: idx_cur,
            op: ArithOp::Add,
            operand: 1,
        },
    });
    ssa.block_mut(latch).terminator = Terminator::Goto(header);
    let body_state_1 = state.snapshot();

    // Insert phis at header
    insert_phis_two_branch(state, ssa, header, block, &pre_state, latch, &body_state_1);

    // --- Pass 2: re-lower with phi-merged versions ---
    ssa.block_mut(header).insts.clear();
    ssa.block_mut(body_block).insts.clear();
    ssa.block_mut(latch).insts.clear();

    // Re-emit condition
    emit_for_bound(ssa, state, header, &bound_left, &bound_right);
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // Re-assign loop variable from argv
    let idx_ref_2 = state.lookup(&idx_name);
    let loop_var_2 = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var_2,
        value: SsaValue::ArgvIndex(idx_ref_2.clone()),
    });

    // Re-lower body
    state.loop_stack.push((latch, exit_block));
    let body_exit_2 = lower_stmts(&f.body, ssa, state, body_block);
    state.loop_stack.pop();
    ssa.block_mut(body_exit_2).terminator = Terminator::Goto(latch);

    // Re-increment
    let idx_cur_2 = state.lookup(&idx_name);
    let idx_next_2 = state.define(&idx_name);
    ssa.block_mut(latch).insts.push(SsaInst::Assign {
        target: idx_next_2,
        value: SsaValue::ArithExpr {
            var: idx_cur_2,
            op: ArithOp::Add,
            operand: 1,
        },
    });

    ssa.block_mut(latch).terminator = Terminator::Goto(header);

    // Update phi sources
    let body_state_2 = state.snapshot();
    update_loop_phi_sources(ssa, header, &body_state_2);

    exit_block
}
