//! Pipeline and for-loop variant lowering.

use super::control_flow::lower_while;
use super::helpers::insert_phis_two_branch;
use super::statements::{lower_one, lower_stmts};
use crate::ir::*;
use crate::ssa::*;

pub(super) fn lower_for_array(
    f: &For,
    array_name: &str,
    use_keys: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let header = ssa.new_block("for_arr.header");
    let body_block = ssa.new_block("for_arr.body");
    let exit_block = ssa.new_block("for_arr.exit");

    // __ai = 0
    let idx_var = state.define("__ai");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: idx_var,
        value: SsaValue::Literal("0".to_string()),
    });

    ssa.block_mut(block).terminator = Terminator::Goto(header);
    let pre_state = state.snapshot();

    // Condition: __ai < sash_table_len(arr)
    // We use arrlen as a ParamExpansion that codegen already handles
    let cond_test = crate::ir::TestExpr::Binary {
        left: Box::new(Expr {
            kind: ExprKind::SimpleExpansion("__ai".to_string()),
            span: Span::default(),
        }),
        op: "-lt".to_string(),
        right: Box::new(Expr {
            kind: ExprKind::ParameterExpansion(crate::ir::ParameterExpansion {
                name: array_name.to_string(),
                operator: Some("arrlen".to_string()),
                operand: None,
            }),
            span: Span::default(),
        }),
    };
    ssa.block_mut(header)
        .insts
        .push(SsaInst::Test(cond_test.clone()));
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // x = table_value(arr, __ai)
    let idx_ref = state.lookup("__ai");
    let loop_var = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var,
        value: if use_keys {
            SsaValue::TableKey {
                array: array_name.to_string(),
                index: idx_ref.clone(),
            }
        } else {
            SsaValue::TableValue {
                array: array_name.to_string(),
                index: idx_ref.clone(),
            }
        },
    });

    let body_exit_1 = lower_stmts(&f.body, ssa, state, body_block);

    // __ai = __ai + 1
    let idx_cur = state.lookup("__ai");
    let idx_next = state.define("__ai");
    ssa.block_mut(body_exit_1).insts.push(SsaInst::Assign {
        target: idx_next,
        value: SsaValue::ArithExpr {
            var: idx_cur,
            op: ArithOp::Add,
            operand: 1,
        },
    });
    let body_state_1 = state.snapshot();

    insert_phis_two_branch(
        state,
        ssa,
        header,
        block,
        &pre_state,
        body_exit_1,
        &body_state_1,
    );

    // --- Pass 2 ---
    ssa.block_mut(header).insts.clear();
    ssa.block_mut(body_block).insts.clear();

    ssa.block_mut(header).insts.push(SsaInst::Test(cond_test));
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    let idx_ref_2 = state.lookup("__ai");
    let loop_var_2 = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var_2,
        value: if use_keys {
            SsaValue::TableKey {
                array: array_name.to_string(),
                index: idx_ref_2.clone(),
            }
        } else {
            SsaValue::TableValue {
                array: array_name.to_string(),
                index: idx_ref_2.clone(),
            }
        },
    });

    let body_exit_2 = lower_stmts(&f.body, ssa, state, body_block);

    let idx_cur_2 = state.lookup("__ai");
    let idx_next_2 = state.define("__ai");
    ssa.block_mut(body_exit_2).insts.push(SsaInst::Assign {
        target: idx_next_2,
        value: SsaValue::ArithExpr {
            var: idx_cur_2,
            op: ArithOp::Add,
            operand: 1,
        },
    });

    ssa.block_mut(body_exit_2).terminator = Terminator::Goto(header);

    let body_state_2 = state.snapshot();
    for phi in ssa.block_mut(header).phis.iter_mut() {
        let var_name = &phi.target.name;
        if let Some(ver) = body_state_2.get(var_name) {
            phi.sources[1].1 = VarRef::new(var_name, *ver);
        }
    }

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
    let header = ssa.new_block("for_at.header");
    let body_block = ssa.new_block("for_at.body");
    let exit_block = ssa.new_block("for_at.exit");

    // Initialize index: __fi = start
    let idx_var = state.define("__fi");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: idx_var,
        value: SsaValue::Literal(start.to_string()),
    });

    // Capture the loop bound once: __argc = $(argc) (C argc, loop-invariant).
    // Materialising it into a variable keeps the loop condition a simple
    // var-vs-var numeric test that the codegen test handler evaluates.
    let argc_var = state.define("__argc");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: argc_var,
        value: SsaValue::CmdSubst {
            argv: vec![SsaValue::Literal("argc".to_string())],
            redirects: vec![],
        },
    });

    ssa.block_mut(block).terminator = Terminator::Goto(header);

    let pre_state = state.snapshot();

    // --- Pass 1: discover modified variables ---
    // Emit condition: __fi < __argc  (C argc — includes the program name)
    let cond_test = crate::ir::TestExpr::Binary {
        left: Box::new(Expr {
            kind: ExprKind::SimpleExpansion("__fi".to_string()),
            span: Span::default(),
        }),
        op: "-lt".to_string(),
        right: Box::new(Expr {
            kind: ExprKind::SimpleExpansion("__argc".to_string()),
            span: Span::default(),
        }),
    };
    ssa.block_mut(header)
        .insts
        .push(SsaInst::Test(cond_test.clone()));
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // Assign loop variable: x = argv[__fi]
    let idx_ref = state.lookup("__fi");
    let loop_var = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var,
        value: SsaValue::ArgvIndex(idx_ref.clone()),
    });

    // Lower body
    let body_exit_1 = lower_stmts(&f.body, ssa, state, body_block);

    // Increment index: __fi = __fi + 1
    let idx_cur = state.lookup("__fi");
    let idx_next = state.define("__fi");
    ssa.block_mut(body_exit_1).insts.push(SsaInst::Assign {
        target: idx_next,
        value: SsaValue::ArithExpr {
            var: idx_cur,
            op: ArithOp::Add,
            operand: 1,
        },
    });
    let body_state_1 = state.snapshot();

    // Insert phis at header
    insert_phis_two_branch(
        state,
        ssa,
        header,
        block,
        &pre_state,
        body_exit_1,
        &body_state_1,
    );

    // --- Pass 2: re-lower with phi-merged versions ---
    ssa.block_mut(header).insts.clear();
    ssa.block_mut(body_block).insts.clear();

    // Re-emit condition
    ssa.block_mut(header).insts.push(SsaInst::Test(cond_test));
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // Re-assign loop variable from argv
    let idx_ref_2 = state.lookup("__fi");
    let loop_var_2 = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var_2,
        value: SsaValue::ArgvIndex(idx_ref_2.clone()),
    });

    // Re-lower body
    let body_exit_2 = lower_stmts(&f.body, ssa, state, body_block);

    // Re-increment
    let idx_cur_2 = state.lookup("__fi");
    let idx_next_2 = state.define("__fi");
    ssa.block_mut(body_exit_2).insts.push(SsaInst::Assign {
        target: idx_next_2,
        value: SsaValue::ArithExpr {
            var: idx_cur_2,
            op: ArithOp::Add,
            operand: 1,
        },
    });

    ssa.block_mut(body_exit_2).terminator = Terminator::Goto(header);

    // Update phi sources
    let body_state_2 = state.snapshot();
    for phi in ssa.block_mut(header).phis.iter_mut() {
        let var_name = &phi.target.name;
        if let Some(ver) = body_state_2.get(var_name) {
            phi.sources[1].1 = VarRef::new(var_name, *ver);
        }
    }

    exit_block
}

// ── Pipe-while-read lowering ──────────────────────────────────

/// Lower `cmd | while read -r var; do body; done`
/// The Pipeline instruction with capture_read_fd=true precedes this.
/// This lowers the while-read loop with fd=-1 (sentinel for pipe fd).
pub(super) fn lower_pipe_while_read(
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if let StmtKind::While(w) = &stmt.kind {
        // The while condition should be `read -r var` — extract the var name
        // and reuse the standard while lowering, but patch ReadLine fd to -1
        let exit = lower_while(w, ssa, state, block);

        // Patch all ReadLine instructions in the loop to use fd=-1 (pipe fd sentinel)
        // Walk all blocks from header (block+1) to exit
        for bid in ssa.blocks.keys().cloned().collect::<Vec<_>>() {
            for inst in &mut ssa.block_mut(bid).insts {
                match inst {
                    SsaInst::ReadLine { fd, .. } if *fd == 0 => {
                        *fd = -1;
                    }
                    SsaInst::ReadLineMulti { fd, .. } if *fd == 0 => {
                        *fd = -1;
                    }
                    _ => {}
                }
            }
        }

        exit
    } else {
        // Shouldn't happen — caller verified this is a While
        lower_one(stmt, ssa, state, block)
    }
}
