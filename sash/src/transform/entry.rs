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
    reserve_source_names(program, &mut state.reserved_names);

    register_functions(program, &mut state);

    let entry = ssa.new_block("entry");
    ssa.entry = entry;

    // DD#56: the END body is its own field on the Program, not a statement, so
    // it is seeded here rather than recognised by name while lowering.
    state.exit_trap = program.end_block.clone();

    let exit = lower_stmts(&program.statements, &mut ssa, &mut state, entry);
    // Ensure exit block terminates with Return before END rewrite
    ssa.block_mut(exit).terminator = Terminator::Return;

    install_end_block(&mut ssa, &mut state);

    // Copy exiting signals to program for codegen check point insertion
    ssa.exiting_signals = state.exiting_signals.clone();
    // Surface the collected external-program manifest (see VersionState::external_deps).
    ssa.external_deps = state.external_deps.borrow().clone();

    mark_unchecked_in_conditions(&mut ssa);
    mark_unchecked_in_end_blocks(&mut ssa);

    // Exact-dedup diagnostics, preserving first-seen order. A function body is
    // lowered both as a standalone template (lower_function_stmt) and at each
    // inline site, so a body-level diagnostic (e.g. the SB5037 recursion
    // reject) is naturally produced more than once for the same cause.
    let mut seen = std::collections::HashSet::new();
    let diags: Vec<String> = state
        .diagnostics
        .iter()
        .filter(|d| seen.insert((*d).clone()))
        .cloned()
        .collect();
    (ssa, diags)
}

fn reserve_source_names(program: &Program, names: &mut HashSet<String>) {
    for body in std::iter::once(program.statements.as_slice()).chain(program.end_block.as_deref()) {
        walk_stmt_tree(body, &mut |stmt| reserve_stmt_names(stmt, names));
    }
}

/// Total over statement metadata; do not add a wildcard.
/// The shared walkers reach functions and captures.
/// Literal operands cover command binders without duplicating their argument grammar.
fn reserve_stmt_names(stmt: &Statement, names: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            names.insert(a.name.clone());
        }
        StmtKind::Declaration(d) => {
            names.extend(d.assignments.iter().map(|a| a.name.clone()));
        }
        StmtKind::For(f) => {
            names.insert(f.variable.clone());
        }
        StmtKind::Command(c) => {
            names.extend(crate::validate::attest_convert_target(c).map(str::to_owned));
        }
        StmtKind::Pipeline(_)
        | StmtKind::If(_)
        | StmtKind::While(_)
        | StmtKind::Case(_)
        | StmtKind::Function(_)
        | StmtKind::Redirected(_)
        | StmtKind::List(_)
        | StmtKind::Group(_)
        | StmtKind::Suppressed(_)
        | StmtKind::Negated(_)
        | StmtKind::ArithTest(_)
        | StmtKind::ReturnProducer(_) => {}
    }
    walk_own_exprs(stmt, &mut |expr| {
        if let ExprKind::Literal(name) | ExprKind::SingleQuoted(name) = &expr.kind {
            names.insert(name.clone());
        }
    });
}

/// Pre-register names and integer returns for forward references; nested
/// definitions are still registered lazily by `lower_function`.
fn register_functions(program: &Program, state: &mut VersionState) {
    for stmt in &program.statements {
        let StmtKind::Function(f) = &stmt.kind else {
            continue;
        };
        state.known_functions.insert(f.name.clone());
        if f.return_type == VarType::Int {
            state.function_int_typed.insert(f.name.clone());
        }
    }
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
        // `if`/`&&`/`||` conditions end in a Branch; `while`/`until` conditions
        // end in a Loop. In both, the last command is in condition position and
        // must not abort on a nonzero exit (it is the loop-exit / else path).
        if matches!(
            block.terminator,
            Terminator::Branch { .. } | Terminator::Loop { .. }
        ) {
            uncheck_last_condition_inst(&mut block.insts);
        }
    }
}

/// Mark the last status-bearing inst of a condition block `checked = false`.
/// DD#70: a `NegateTestResult` sigil sits *after* the operand it negates, so
/// skip it — the real command/pipeline (which the `!` catches) is the one to
/// uncheck.
pub(super) fn uncheck_last_condition_inst(insts: &mut [SsaInst]) {
    for inst in insts.iter_mut().rev() {
        match inst {
            SsaInst::Command { name, .. } if name == "__negate_test_result" => continue,
            // A redirect scope (`f > out.txt`) always closes its block with
            // the Branch that skips the body on a failed `open(2)`, so a
            // Branch here says nothing about whether the *call* is in
            // condition position — the transform already recorded that on the
            // scope itself. Stop, so an unrelated command earlier in the same
            // block is not unchecked on the strength of a branch that is not
            // its own.
            SsaInst::RedirectPush { .. } => return,
            SsaInst::Command { checked, .. } | SsaInst::Pipeline { checked, .. } => {
                *checked = false;
                return;
            }
            _ => {}
        }
    }
}

/// DD#56: every Command and Pipeline reachable from the `__end__`
/// block runs as part of cleanup and must not abort the program if
/// it fails (otherwise a failed cleanup step would skip the rest
/// of END). Mark them all unchecked.
fn mark_unchecked_in_end_blocks(ssa: &mut SsaProgram) {
    // Reachability, not a label test. Only END's *entry* block is named
    // `__end__`; the blocks an `if`/`for` inside END creates get ordinary
    // labels, so the label test marked the first block and left the rest
    // checked. A failing command inside `END { if … { … } }` then aborted,
    // and the abort routes through END — from inside END, a jump to itself.
    for id in ssa.end_region() {
        mark_block_unchecked(ssa.block_mut(id));
    }
}

fn mark_block_unchecked(block: &mut BasicBlock) {
    for inst in block.insts.iter_mut() {
        match inst {
            SsaInst::Command { checked, .. }
            | SsaInst::Pipeline { checked, .. }
            // A redirect scope must be unchecked here for the same reason,
            // and doubly so: its abort jumps to the END block, which inside
            // END is a jump to itself. Unchecked, a failed open skips the
            // call and the rest of the cleanup still runs.
            | SsaInst::RedirectPush { checked, .. } => {
                *checked = false;
            }
            _ => {}
        }
    }
}
