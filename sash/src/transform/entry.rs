//! Entry points for IR → SSA transformation.

use super::statements::lower_stmts;
use crate::ir::*;
use crate::ssa::*;
use std::collections::{HashMap, HashSet};

/// Transform an IR Program into SSA form with type information.
pub fn ir_to_ssa_typed(
    program: &Program,
    type_map: HashMap<String, VarType>,
) -> (SsaProgram, Vec<String>) {
    let mut ssa = SsaProgram::new();
    let mut state = VersionState::with_types(type_map);

    state.known_functions = collect_function_names(program);

    let entry = ssa.new_block("entry");
    ssa.entry = entry;

    let exit = lower_stmts(&program.statements, &mut ssa, &mut state, entry);
    // Ensure exit block terminates with Return before END rewrite
    ssa.block_mut(exit).terminator = Terminator::Return;

    install_end_block(&mut ssa, &mut state);

    // Copy exiting signals to program for codegen check point insertion
    ssa.exiting_signals = state.exiting_signals.clone();

    mark_unchecked_in_conditions(&mut ssa);
    mark_unchecked_in_end_blocks(&mut ssa);

    let diags = state.diagnostics.clone();
    (ssa, diags)
}

/// First pass over the IR: collect the names of every defined
/// function so that `CommandSubstitution` lowering can detect
/// elidable function calls without forking through the resolver.
fn collect_function_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in &program.statements {
        if let StmtKind::Function(f) = &stmt.kind {
            names.insert(f.name.clone());
        }
    }
    names
}

/// DD#56: install a dedicated `__end__` block that contains the
/// body of the EXIT trap. All non-function Return terminators are
/// rewritten to goto the new end block, ensuring the END handler
/// runs regardless of how the program exits (normal, abort,
/// explicit exit).
fn install_end_block(ssa: &mut SsaProgram, state: &mut VersionState) {
    let Some(trap_stmts) = state.exit_trap.take() else {
        return;
    };

    let end_block = ssa.new_block("__end__");
    let end_exit = lower_stmts(&trap_stmts, ssa, state, end_block);
    ssa.block_mut(end_exit).terminator = Terminator::Return;

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

    ssa.end_block = Some(end_block);
}

/// DD#55: any Command or Pipeline that produces a value consumed
/// by a Branch terminator (i.e. is in condition position) must be
/// marked `checked = false` so that codegen does not insert an
/// abort-on-nonzero point. The relevant inst is the *last* one in
/// the block before the branch, so we scan in reverse and stop at
/// the first match.
fn mark_unchecked_in_conditions(ssa: &mut SsaProgram) {
    for block in ssa.blocks.values_mut() {
        if !matches!(block.terminator, Terminator::Branch { .. }) {
            continue;
        }
        for inst in block.insts.iter_mut().rev() {
            match inst {
                SsaInst::Command { checked, .. } | SsaInst::Pipeline { checked, .. } => {
                    *checked = false;
                    break;
                }
                _ => {}
            }
        }
    }
}

/// DD#56: every Command and Pipeline reachable from the `__end__`
/// block runs as part of cleanup and must not abort the program if
/// it fails (otherwise a failed cleanup step would skip the rest
/// of END). Mark them all unchecked.
fn mark_unchecked_in_end_blocks(ssa: &mut SsaProgram) {
    for block in ssa.blocks.values_mut() {
        if !block.label.starts_with("__end__") && !block.label.contains("__end__") {
            continue;
        }
        for inst in block.insts.iter_mut() {
            match inst {
                SsaInst::Command { checked, .. } | SsaInst::Pipeline { checked, .. } => {
                    *checked = false;
                }
                _ => {}
            }
        }
    }
}
