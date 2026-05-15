//! Entry points for IR → SSA transformation.

use super::statements::lower_stmts;
use crate::ir::*;
use crate::ssa::*;
use std::collections::HashMap;

/// Transform an IR Program into SSA form.
pub fn ir_to_ssa(program: &Program) -> (SsaProgram, Vec<String>) {
    ir_to_ssa_typed(program, HashMap::new())
}

/// Transform an IR Program into SSA form with type information.
pub fn ir_to_ssa_typed(
    program: &Program,
    type_map: HashMap<String, VarType>,
) -> (SsaProgram, Vec<String>) {
    let mut ssa = SsaProgram::new();
    let mut state = VersionState::with_types(type_map);

    // First pass: collect function names so CommandSubstitution
    // can detect elidable function calls.
    let mut known_functions: std::collections::HashSet<String> = std::collections::HashSet::new();
    for stmt in &program.statements {
        if let StmtKind::Function(f) = &stmt.kind {
            known_functions.insert(f.name.clone());
        }
    }
    state.known_functions = known_functions;

    let entry = ssa.new_block("entry");
    ssa.entry = entry;

    let exit = lower_stmts(&program.statements, &mut ssa, &mut state, entry);

    // Ensure exit block terminates with Return before END rewrite
    ssa.block_mut(exit).terminator = Terminator::Return;

    // DD#56: Create a dedicated end_block for the END body.
    // All exit paths (normal, abort, explicit exit) goto end_block.
    // This ensures END always fires regardless of how the program exits.
    if let Some(trap_stmts) = state.exit_trap.take() {
        let end_block = ssa.new_block("__end__");
        let end_exit = lower_stmts(&trap_stmts, &mut ssa, &mut state, end_block);
        ssa.block_mut(end_exit).terminator = Terminator::Return;

        // Redirect all non-function Return terminators to goto end_block
        let return_blocks: Vec<usize> = ssa
            .blocks
            .iter()
            .filter(|(_, b)| {
                b.terminator == Terminator::Return
                    && !b.label.starts_with("fn_")
                    && !b.label.starts_with("__end__")
            })
            .map(|(&id, _)| id)
            .collect();
        for block_id in return_blocks {
            if block_id != end_block && block_id != end_exit {
                ssa.block_mut(block_id).terminator = Terminator::Goto(end_block);
            }
        }

        // Store end_block id for codegen abort paths
        ssa.end_block = Some(end_block);
    }

    // Copy exiting signals to program for codegen check point insertion
    ssa.exiting_signals = state.exiting_signals.clone();

    // DD#55: Mark commands/pipelines in condition position as unchecked.
    for block in ssa.blocks.values_mut() {
        if matches!(block.terminator, Terminator::Branch { .. }) {
            for inst in block.insts.iter_mut().rev() {
                match inst {
                    SsaInst::Command { checked, .. } => {
                        *checked = false;
                        break;
                    }
                    SsaInst::Pipeline { checked, .. } => {
                        *checked = false;
                        break;
                    }
                    _ => {}
                }
            }
        }
        // DD#56: Commands in END block are unchecked (cleanup must not abort)
        if block.label.starts_with("__end__") || block.label.contains("__end__") {
            for inst in block.insts.iter_mut() {
                match inst {
                    SsaInst::Command { checked, .. } => {
                        *checked = false;
                    }
                    SsaInst::Pipeline { checked, .. } => {
                        *checked = false;
                    }
                    _ => {}
                }
            }
        }
    }

    let diags = state.diagnostics.clone();
    (ssa, diags)
}
