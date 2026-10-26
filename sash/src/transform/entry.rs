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
    ssa.user_fds = program_referenced_fds(program);
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
    super::reader_cleanup::install(&mut ssa, &state.reader_regions);

    // Copy exiting signals to program for codegen check point insertion
    ssa.exiting_signals = state.exiting_signals.clone();
    ssa.install_signal_checkpoints();
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

/// DD#55: the instruction whose status a Branch or Loop terminator reads is in
/// condition position, so a nonzero status there is the else / loop-exit path
/// and must not abort. See [`uncheck_last_condition_inst`].
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

/// Clear the abort on the instruction in condition position, as
/// [`crate::ssa::condition_operand_index`] defines it — the definition both
/// backends use for the redirect guard, so the two cannot pick different
/// instructions. Only a command or pipeline aborts on its status. A `[[ … ]]`,
/// a read, a `match`, `attest … in` and `$?` never do, and a redirect scope
/// recorded condition position on itself when it was built: those leave
/// nothing to clear, and they end the search — the command before them is not
/// in condition position and keeps its abort.
pub(super) fn uncheck_last_condition_inst(insts: &mut [SsaInst]) {
    let Some(idx) = crate::ssa::condition_operand_index(insts) else {
        return;
    };
    if let SsaInst::Command { checked, .. }
    | SsaInst::Pipeline { checked, .. }
    | SsaInst::PipelineFinish { checked, .. } = &mut insts[idx]
    {
        *checked = false;
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
            | SsaInst::PipelineFinish { checked, .. }
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
