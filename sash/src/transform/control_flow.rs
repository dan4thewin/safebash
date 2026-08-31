//! Control flow: if/while/for/case → SSA blocks with branches.

use super::expressions::{arg_to_value, expr_to_string, expr_to_value, test_to_ssa};
use super::helpers::insert_phis_two_branch;
use super::pipelines::{lower_for_argv_slice, lower_for_array};
use super::statements::lower_stmts;
use crate::ir::*;
use crate::ssa::*;

use super::resolution::resolve_pipeline_command;

pub(super) fn flatten_elif(i: &If) -> If {
    if i.elif_clauses.is_empty() {
        return i.clone();
    }
    // Build from the tail: start with the else body (or None)
    let mut else_body = i.else_body.clone();
    for elif in i.elif_clauses.iter().rev() {
        let nested = If {
            condition: elif.condition.clone(),
            then_body: elif.body.clone(),
            elif_clauses: vec![],
            else_body,
        };
        else_body = Some(vec![Statement {
            kind: StmtKind::If(nested),
            span: Span::default(),
        }]);
    }
    If {
        condition: i.condition.clone(),
        then_body: i.then_body.clone(),
        elif_clauses: vec![],
        else_body,
    }
}

/// DD#24: if bless VAR [[ test ]]; then ... fi
/// Non-fatal bless: test branch gets clean var, else branch keeps tainted.
fn lower_if_bless(
    i: &If,
    c: &crate::ir::Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let Some((form, test)) = crate::ir::classify_bless(&c.args) else {
        return block;
    };
    let test = test.clone();

    // Owned decode so the borrow of `c.args` ends before we mutate state.
    enum Kind {
        Scalar(String),
        Coll {
            array: String,
            binders: Vec<String>,
            is_map: bool,
        },
    }
    let kind = match form {
        BlessForm::Scalar { var } => Kind::Scalar(var.to_string()),
        BlessForm::Indexed { binder, array } => Kind::Coll {
            array: array.to_string(),
            binders: vec![binder.to_string()],
            is_map: false,
        },
        BlessForm::Map { key, val, map } => Kind::Coll {
            array: map.to_string(),
            binders: vec![key.to_string(), val.to_string()],
            is_map: true,
        },
    };

    // Emit the bless check into `block`, leaving pass/fail in the
    // test-result var for the branch below.
    match &kind {
        Kind::Scalar(_) => {
            ssa.block_mut(block)
                .insts
                .push(SsaInst::Test(test_to_ssa(&test, state)));
        }
        Kind::Coll {
            array,
            binders,
            is_map,
        } => {
            let source = state.lookup(array);
            // Fresh, unique binder versions (see push_bless_all); restore so
            // the binders stay loop-local.
            let snap = state.snapshot();
            let binder_refs: Vec<VarRef> = binders.iter().map(|b| state.define(b)).collect();
            let ssa_test = test_to_ssa(&test, state);
            state.restore(&snap);
            ssa.block_mut(block).insts.push(SsaInst::BlessAll {
                source,
                binders: binder_refs,
                test: ssa_test,
                is_map: *is_map,
                if_form: true,
            });
        }
    }

    let then_block = ssa.new_block("if_bless.then");
    let merge_block = ssa.new_block("if_bless.merge");

    let pre_state = state.snapshot();

    // Branch on test result
    ssa.block_mut(block).terminator = Terminator::Branch {
        true_block: then_block,
        false_block: merge_block,
        condition: "bless".to_string(),
    };

    // Then branch: retag the blessed entity clean (only reached on pass).
    match &kind {
        Kind::Scalar(var) => {
            let source_ref = state.lookup(var);
            // Resolve the predicate before defining the target version so
            // `$var` binds to the source (emit_bless evaluates the test
            // before the target slot exists). See dispatch.rs::emit_bless.
            let ssa_test = test_to_ssa(&test, state);
            let target_ref = state.define(var);
            ssa.block_mut(then_block).insts.push(SsaInst::Bless {
                target: target_ref,
                source: source_ref,
                test: ssa_test,
            });
        }
        Kind::Coll { array, .. } => {
            let source_ref = state.lookup(array);
            let target_ref = state.define(array);
            ssa.block_mut(then_block).insts.push(SsaInst::BlessRetag {
                target: target_ref,
                source: source_ref,
            });
        }
    }
    let then_exit = lower_stmts(&i.then_body, ssa, state, then_block);
    ssa.block_mut(then_exit).terminator = Terminator::Goto(merge_block);
    let _then_snap = state.snapshot();

    if let Some(else_body) = &i.else_body {
        let else_block = ssa.new_block("if_bless.else");
        if let Terminator::Branch { false_block, .. } = &mut ssa.block_mut(block).terminator {
            *false_block = else_block;
        }
        state.restore(&pre_state);
        let else_exit = lower_stmts(else_body, ssa, state, else_block);
        ssa.block_mut(else_exit).terminator = Terminator::Goto(merge_block);
    } else {
        state.restore(&pre_state);
    }

    // Merge: use then_state versions for variables modified in then
    // (simplified: just restore to pre-state, let Cranelift SSA handle merge)

    merge_block
}

pub(super) fn lower_if(
    i: &If,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#24: detect if-bless conditional form
    if i.condition.len() == 1 {
        if let StmtKind::Command(c) = &i.condition[0].kind {
            if expr_to_string(&c.name) == "bless" && c.args.len() >= 2 {
                return lower_if_bless(i, c, ssa, state, block);
            }
        }
    }

    // Lower condition into current block
    state.in_condition += 1;
    let cond_block = lower_stmts(&i.condition, ssa, state, block);
    state.in_condition -= 1;
    let cond_str = "condition".to_string(); // placeholder

    let then_block = ssa.new_block("if.then");
    let merge_block = ssa.new_block("if.merge");

    // Snapshot state before branches
    let pre_state = state.snapshot();

    // Lower then branch
    let then_exit = lower_stmts(&i.then_body, ssa, state, then_block);
    ssa.block_mut(then_exit).terminator = Terminator::Goto(merge_block);
    let then_state = state.snapshot();

    if let Some(else_body) = &i.else_body {
        let else_block = ssa.new_block("if.else");

        // Restore to pre-branch state for else
        state.restore(&pre_state);
        let else_exit = lower_stmts(else_body, ssa, state, else_block);
        ssa.block_mut(else_exit).terminator = Terminator::Goto(merge_block);
        let else_state = state.snapshot();

        // Set branch terminator
        ssa.block_mut(cond_block).terminator = Terminator::Branch {
            condition: cond_str,
            true_block: then_block,
            false_block: else_block,
        };

        // Insert phi nodes at merge point for variables modified in either branch
        insert_phis_two_branch(
            state,
            ssa,
            merge_block,
            then_exit,
            &then_state,
            else_exit,
            &else_state,
        );
    } else {
        // No else — branch to then or skip to merge
        ssa.block_mut(cond_block).terminator = Terminator::Branch {
            condition: cond_str,
            true_block: then_block,
            false_block: merge_block,
        };

        // Phi nodes: then-branch version vs pre-branch version
        insert_phis_two_branch(
            state,
            ssa,
            merge_block,
            then_exit,
            &then_state,
            cond_block,
            &pre_state,
        );
    }

    // Handle elif clauses by chaining — each elif is a nested if
    // For simplicity in this implementation, elif clauses were already
    // flattened into the IR's if/elif structure. We handle the first
    // else and leave elif for future refinement.

    merge_block
}

// ── While lowering ────────────────────────────────────────────

pub(super) fn lower_while(
    w: &While,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let header = ssa.new_block("while.header");
    let body_block = ssa.new_block("while.body");
    let exit_block = ssa.new_block("while.exit");

    // Wire entry → header
    ssa.block_mut(block).terminator = Terminator::Goto(header);

    // Snapshot state before loop
    let pre_state = state.snapshot();

    // === Two-pass loop lowering ===
    // Pass 1: Lower body to discover which variables are modified
    let _state_at_header = state.snapshot();
    let body_exit_1 = lower_stmts(&w.body, ssa, state, body_block);
    let body_state_1 = state.snapshot();

    // Insert phi nodes at header for loop-carried variables
    // This creates new versions (e.g., x_3) for modified variables
    insert_phis_two_branch(
        state,
        ssa,
        header,
        block,
        &pre_state,
        body_exit_1,
        &body_state_1,
    );

    // Pass 2: Re-lower condition and body using phi-merged versions
    // Clear the body block's instructions (we're re-lowering)
    ssa.block_mut(body_block).insts.clear();
    ssa.block_mut(header).insts.clear();

    // Lower condition in header using post-phi state
    state.in_condition += 1;
    let cond_exit = lower_stmts(&w.condition, ssa, state, header);
    state.in_condition -= 1;

    // Set loop terminator
    ssa.block_mut(cond_exit).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // Push loop context for break/continue
    state.loop_stack.push((header, exit_block));

    // Re-lower body with phi-merged versions
    let body_exit = lower_stmts(&w.body, ssa, state, body_block);
    ssa.block_mut(body_exit).terminator = Terminator::Goto(header);

    // Pop loop context
    state.loop_stack.pop();

    // Update phi sources to use the correct versions from pass 2
    let body_state_2 = state.snapshot();
    let header_phis = &mut ssa.block_mut(header).phis;
    for phi in header_phis.iter_mut() {
        let var_name = &phi.target.name;
        // Update the body source (index 1) with the new version
        if let Some(ver) = body_state_2.get(var_name) {
            phi.sources[1].1 = VarRef::new(var_name, *ver);
        }
    }

    exit_block
}

// ── For lowering ──────────────────────────────────────────────

pub(super) fn lower_for(
    f: &For,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if let Some(b) = try_dispatch_argv_splat(f, ssa, state, block) {
        return b;
    }
    if let Some(b) = try_dispatch_array(f, ssa, state, block) {
        return b;
    }
    if let Some(b) = try_dispatch_cmdsubst(f, ssa, state, block) {
        return b;
    }
    if let Some(b) = try_dispatch_glob(f, ssa, state, block) {
        return b;
    }
    lower_for_static(f, ssa, state, block)
}

fn is_dynamic_single_word(f: &For) -> bool {
    let is_dynamic = f
        .words
        .iter()
        .any(|w| !matches!(&w.kind, ExprKind::Literal(_)));
    is_dynamic && f.words.len() == 1
}

/// Dispatch `for x in $(argv)` / `for x in $(argv N)` to the argv-slice
/// loop (DD#60). The validator guarantees the splat is the sole for-in
/// source; here we extract the start index (0 for bare `$(argv)`).
fn try_dispatch_argv_splat(
    f: &For,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    if f.words.len() != 1 {
        return None;
    }
    let start = argv_splat_start(&f.words[0])?;
    Some(lower_for_argv_slice(f, start, ssa, state, block))
}

/// If `word` is `$(argv)` or `$(argv N)`, return the start index
/// (0 for bare `$(argv)`, N for `$(argv N)` with a literal N).
fn argv_splat_start(word: &Expr) -> Option<i64> {
    let ExprKind::CommandSubstitution(stmts) = &word.kind else {
        return None;
    };
    let inner = match stmts.first().map(|s| &s.kind) {
        Some(StmtKind::Redirected(r)) => &r.body.kind,
        Some(other) => other,
        None => return None,
    };
    let StmtKind::Command(c) = inner else {
        return None;
    };
    if expr_to_string(&c.name) != "argv" {
        return None;
    }
    match c.args.first() {
        None => Some(0),
        Some(a) => expr_to_string(a).parse::<i64>().ok().filter(|n| *n >= 0),
    }
}

fn try_dispatch_array(
    f: &For,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    if !is_dynamic_single_word(f) {
        return None;
    }
    let (name, use_keys) = detect_array_iter(&f.words[0].kind)?;
    let name = super::expressions::resolve_args_name(&name, state);
    Some(lower_for_array(f, &name, use_keys, ssa, state, block))
}

/// DD#20/DD#69: `for f in *` is sugar for `array tmp=(*); for f in @tmp`.
/// Materialise the glob into a temp array (the existing `glob`
/// param-expansion → libc glob → table) and iterate it per-element. 0
/// matches → empty table → body never runs (lower_for_array bound).
fn try_dispatch_glob(
    f: &For,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    if f.words.len() != 1 {
        return None;
    }
    let ExprKind::Literal(s) = &f.words[0].kind else {
        return None;
    };
    if !s.contains('*') {
        return None;
    }
    let glob_val = SsaValue::ParamExpansion {
        var: VarRef::new(".", 0),
        operator: "glob".to_string(),
        operand: s.clone(),
    };
    let target = state.define("__forglob");
    let tmp_name = target.name.clone();
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target,
        value: glob_val,
    });
    Some(lower_for_array(f, &tmp_name, false, ssa, state, block))
}

fn detect_array_iter(kind: &ExprKind) -> Option<(String, bool)> {
    match kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            Some((pe.name.clone(), false))
        }
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrkeys") => {
            Some((pe.name.clone(), true))
        }
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => detect_array_iter(&parts[0].kind),
        // Sash-native: $(keys arr) in for-loop → array key iteration (DD#41)
        ExprKind::CommandSubstitution(stmts) if stmts.len() == 1 => {
            let inner = match &stmts[0].kind {
                StmtKind::Redirected(r) => &r.body.kind,
                other => other,
            };
            let StmtKind::Command(c) = inner else {
                return None;
            };
            let ExprKind::Literal(cmd_name) = &c.name.kind else {
                return None;
            };
            if cmd_name != "keys" {
                return None;
            }
            c.args.first().map(|a| (expr_to_string(a), true))
        }
        _ => None,
    }
}

fn try_dispatch_cmdsubst(
    f: &For,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    if !is_dynamic_single_word(f) {
        return None;
    }
    let ExprKind::CommandSubstitution(stmts) = &f.words[0].kind else {
        return None;
    };
    // DD#28/DD#40: silently transform `for x in $(cmd args)` to
    // `cmd args | while read -r x; do body; done`.
    let stages = build_pipeline_stages_from_cmdsubst(stmts, state);
    Some(emit_cmdsubst_pipe_while_read(f, stages, ssa, state, block))
}

fn build_pipeline_stages_from_cmdsubst(
    stmts: &[Statement],
    state: &mut VersionState,
) -> Vec<(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)> {
    let mut stages = vec![];
    for stmt in stmts {
        if let Some(stage) = build_cmdsubst_stage(stmt, state) {
            stages.push(stage);
        }
    }
    stages
}

/// Lower one `$( … )` pipeline stage statement to `(resolved_name, args,
/// redirects)`. Returns `None` for a non-command statement.
fn build_cmdsubst_stage(
    stmt: &Statement,
    state: &mut VersionState,
) -> Option<(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)> {
    let cmd_stmt = match &stmt.kind {
        StmtKind::Redirected(r) => &r.body,
        _ => stmt,
    };
    let StmtKind::Command(c) = &cmd_stmt.kind else {
        return None;
    };
    let name = expr_to_string(&c.name);
    let args: Vec<SsaValue> = c.args.iter().map(|a| arg_to_value(a, state)).collect();
    let redirs = match &stmt.kind {
        StmtKind::Redirected(r) => r.redirects.clone(),
        _ => vec![],
    };
    Some((resolve_pipeline_command(&name, state), args, redirs))
}

fn emit_cmdsubst_pipe_while_read(
    f: &For,
    stages: Vec<(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)>,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
        stages,
        capture_read_fd: true,
        checked: true,
        suppressed: vec![],
        negated: vec![],
    });

    let loop_var = &f.variable;
    let header = ssa.new_block("for_cmd.hdr");
    let body_block = ssa.new_block("for_cmd.body");
    let exit = ssa.new_block("for_cmd.exit");

    ssa.block_mut(block).terminator = Terminator::Goto(header);

    let target = state.define(loop_var);
    ssa.block_mut(header).insts.push(SsaInst::ReadLine {
        target,
        fd: -1,
        delim: None,
        redirects: vec![],
        clean_pipe: false,
    });
    ssa.block_mut(header).terminator = Terminator::Branch {
        condition: "__test_result".to_string(),
        true_block: body_block,
        false_block: exit,
    };

    state.loop_stack.push((header, exit));
    let body_end = lower_stmts(&f.body, ssa, state, body_block);
    state.loop_stack.pop();
    ssa.block_mut(body_end).terminator = Terminator::Goto(header);

    exit
}

fn lower_for_static(
    f: &For,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let exit_block = ssa.new_block("for.exit");
    let mut current = block;

    // Pre-create iteration blocks so `continue` can jump to the next one.
    let iter_blocks: Vec<BlockId> = f.words.iter().map(|_| ssa.new_block("for.iter")).collect();

    for (i, word) in f.words.iter().enumerate() {
        let iter_block = iter_blocks[i];
        ssa.block_mut(current).terminator = Terminator::Goto(iter_block);
        current = iter_block;

        let continue_target = if i + 1 < iter_blocks.len() {
            iter_blocks[i + 1]
        } else {
            exit_block
        };
        state.loop_stack.push((continue_target, exit_block));

        let val = expr_to_value(word, state);
        let loop_var = state.define(&f.variable);
        ssa.block_mut(current).insts.push(SsaInst::Assign {
            target: loop_var,
            value: val,
        });
        current = lower_stmts(&f.body, ssa, state, current);

        state.loop_stack.pop();
    }

    ssa.block_mut(current).terminator = Terminator::Goto(exit_block);
    exit_block
}

// ── Case lowering ─────────────────────────────────────────────

pub(super) fn lower_case(
    c: &Case,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let merge_block = ssa.new_block("dispatch.merge");

    if c.items.is_empty() {
        ssa.block_mut(block).terminator = Terminator::Goto(merge_block);
        return merge_block;
    }

    let pre_state = state.snapshot();
    let (case_blocks, case_exits, branch_states) = build_case_bodies(c, ssa, state, &pre_state);

    let check_blocks = build_check_blocks(ssa, c.items.len(), block);

    wire_case_exits(ssa, &case_exits, merge_block);

    build_dispatch_chain(c, ssa, state, &case_blocks, &check_blocks, merge_block);

    let no_match_path = wire_no_match_path(c, ssa, state, &pre_state, &check_blocks, merge_block);

    insert_case_phis(
        ssa,
        merge_block,
        &branch_states,
        no_match_path.as_ref(),
        state,
    );

    merge_block
}

type BranchSnapshot = (BlockId, std::collections::HashMap<String, u32>);

/// Lower each arm's body into its own block; capture exit blocks and the
/// post-body version snapshot per arm. Each arm starts from `pre_state`.
fn build_case_bodies(
    c: &Case,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    pre_state: &std::collections::HashMap<String, u32>,
) -> (Vec<BlockId>, Vec<BlockId>, Vec<BranchSnapshot>) {
    let mut case_blocks = Vec::with_capacity(c.items.len());
    let mut case_exits = Vec::with_capacity(c.items.len());
    let mut branch_states = Vec::with_capacity(c.items.len());

    for (i, item) in c.items.iter().enumerate() {
        state.restore(pre_state);
        let case_block = ssa.new_block(&format!("case.{}", i));
        let case_exit = lower_stmts(&item.body, ssa, state, case_block);
        branch_states.push((case_exit, state.snapshot()));
        case_blocks.push(case_block);
        case_exits.push(case_exit);
    }
    (case_blocks, case_exits, branch_states)
}

/// Pre-create dispatch-chain blocks: index 0 is the entry block; the rest
/// are fresh `case.check.N` blocks a non-matching arm falls through to.
fn build_check_blocks(ssa: &mut SsaProgram, n_items: usize, entry: BlockId) -> Vec<BlockId> {
    let mut check_blocks = Vec::with_capacity(n_items);
    check_blocks.push(entry);
    for i in 1..n_items {
        check_blocks.push(ssa.new_block(&format!("case.check.{}", i)));
    }
    check_blocks
}

/// Wire each dispatch arm's body-exit to the merge block. (DD#66 dropped
/// fall-through/resume, so an arm always exits straight to the merge.)
fn wire_case_exits(ssa: &mut SsaProgram, case_exits: &[BlockId], merge_block: BlockId) {
    for &exit in case_exits {
        ssa.block_mut(exit).terminator = Terminator::Goto(merge_block);
    }
}

/// Build the pattern-dispatch chain across `check_blocks`. Wildcard arms
/// become unconditional gotos; single-pattern arms emit one test;
/// multi-pattern arms chain sub-blocks so any match jumps to the body.
fn build_dispatch_chain(
    c: &Case,
    ssa: &mut SsaProgram,
    state: &VersionState,
    case_blocks: &[BlockId],
    check_blocks: &[BlockId],
    merge_block: BlockId,
) {
    let value_ssa = expr_to_value(&c.value, state);

    for (i, item) in c.items.iter().enumerate() {
        let next_check = if i + 1 < check_blocks.len() {
            check_blocks[i + 1]
        } else {
            merge_block
        };
        emit_dispatch_arm(
            ssa,
            i,
            item,
            &value_ssa,
            check_blocks[i],
            case_blocks[i],
            next_check,
            &c.op,
            state,
        );
    }
}

/// Wire one dispatch arm's check block: wildcard → unconditional goto;
/// single pattern → one test branch; multiple → a test chain.
#[allow(clippy::too_many_arguments)]
fn emit_dispatch_arm(
    ssa: &mut SsaProgram,
    arm_idx: usize,
    item: &CaseItem,
    value: &SsaValue,
    current: BlockId,
    case_block: BlockId,
    next_check: BlockId,
    op: &str,
    state: &VersionState,
) {
    if is_default_arm(item) {
        ssa.block_mut(current).terminator = Terminator::Goto(case_block);
        return;
    }
    if item.patterns.len() == 1 {
        emit_single_pattern_branch(
            ssa,
            current,
            value,
            &item.patterns[0],
            case_block,
            next_check,
            op,
            state,
        );
    } else {
        emit_multi_pattern_chain(
            ssa,
            arm_idx,
            current,
            value,
            &item.patterns,
            case_block,
            next_check,
            op,
            state,
        );
    }
}

/// DD#71: the `|| { … }` default arm runs unconditionally when reached.
/// `*` is an ordinary literal pattern, not a wildcard.
fn is_default_arm(item: &CaseItem) -> bool {
    item.is_default
}

fn dispatch_test(value: &SsaValue, pattern: &Expr, op: &str, state: &VersionState) -> SsaTest {
    // DD#66/#71 dispatch ops (`==` exact / `=~` regex) compare against the
    // operand word: a static pattern is a quote-stripped `Pattern` string; a
    // dynamic (interpolated) pattern is a runtime `Value`.
    let right = if crate::transform::expressions::expr_is_dynamic(pattern) {
        SsaTestRhs::Value(expr_to_value(pattern, state))
    } else {
        SsaTestRhs::Pattern(crate::transform::expressions::operand_pattern_string(
            pattern,
        ))
    };
    SsaTest::Binary {
        left: value.clone(),
        op: op.to_string(),
        right,
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_single_pattern_branch(
    ssa: &mut SsaProgram,
    current: BlockId,
    value: &SsaValue,
    pattern: &Expr,
    true_block: BlockId,
    false_block: BlockId,
    op: &str,
    state: &VersionState,
) {
    ssa.block_mut(current)
        .insts
        .push(SsaInst::Test(dispatch_test(value, pattern, op, state)));
    ssa.block_mut(current).terminator = Terminator::Branch {
        condition: "__test".to_string(),
        true_block,
        false_block,
    };
}

#[allow(clippy::too_many_arguments)]
fn emit_multi_pattern_chain(
    ssa: &mut SsaProgram,
    arm_idx: usize,
    current: BlockId,
    value: &SsaValue,
    patterns: &[Expr],
    case_block: BlockId,
    next_check: BlockId,
    op: &str,
    state: &VersionState,
) {
    let mut pat_current = current;
    for (pi, pat) in patterns.iter().enumerate() {
        ssa.block_mut(pat_current)
            .insts
            .push(SsaInst::Test(dispatch_test(value, pat, op, state)));

        let (true_block, false_block) = if pi + 1 < patterns.len() {
            let next_pat = ssa.new_block(&format!("case.{}.pat.{}", arm_idx, pi + 1));
            (case_block, next_pat)
        } else {
            (case_block, next_check)
        };
        ssa.block_mut(pat_current).terminator = Terminator::Branch {
            condition: "__test".to_string(),
            true_block,
            false_block,
        };
        if pi + 1 < patterns.len() {
            pat_current = false_block;
        }
    }
}

/// If no arm is a wildcard, a no-match path exists. Build a dedicated
/// `case.nomatch` block that defines `?` = 0 (POSIX: exit status is zero
/// when no pattern matches) and re-wire dispatch false-branches that
/// previously landed on `merge_block` to land there instead.
fn wire_no_match_path(
    c: &Case,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    pre_state: &std::collections::HashMap<String, u32>,
    check_blocks: &[BlockId],
    merge_block: BlockId,
) -> Option<BranchSnapshot> {
    let has_wildcard = c.items.iter().any(is_default_arm);
    if has_wildcard {
        return None;
    }

    let no_match_block = ssa.new_block("case.nomatch");
    state.restore(pre_state);
    let zero_ref = state.define("?");
    let no_match_snapshot = state.snapshot();
    ssa.block_mut(no_match_block).insts.push(SsaInst::Assign {
        target: zero_ref,
        value: SsaValue::Literal("0".to_string()),
    });
    ssa.block_mut(no_match_block).terminator = Terminator::Goto(merge_block);

    redirect_merge_false_branches(ssa, check_blocks, merge_block, no_match_block);

    Some((no_match_block, no_match_snapshot))
}

/// Find every block whose terminator is `Branch { false_block: merge, .. }`
/// and redirect the false branch to `no_match`. Considers both the
/// dispatch check_blocks and any multi-pattern sub-blocks (which live
/// elsewhere in `ssa.blocks`).
fn redirect_merge_false_branches(
    ssa: &mut SsaProgram,
    check_blocks: &[BlockId],
    merge_block: BlockId,
    no_match_block: BlockId,
) {
    for bid in check_blocks {
        if let Terminator::Branch { false_block, .. } = &mut ssa.block_mut(*bid).terminator {
            if *false_block == merge_block {
                *false_block = no_match_block;
            }
        }
    }
    let all_block_ids: Vec<usize> = ssa.blocks.keys().cloned().collect();
    for bid in all_block_ids {
        if check_blocks.contains(&bid) || bid == merge_block || bid == no_match_block {
            continue;
        }
        if let Terminator::Branch { false_block, .. } = &mut ssa.block_mut(bid).terminator {
            if *false_block == merge_block {
                *false_block = no_match_block;
            }
        }
    }
}

/// Insert phi nodes at the merge block across all incoming paths (arms +
/// optional no-match). Single-path inputs (e.g. wildcard-only case)
/// just adopt that path's snapshot.
fn insert_case_phis(
    ssa: &mut SsaProgram,
    merge_block: BlockId,
    branch_states: &[BranchSnapshot],
    no_match_path: Option<&BranchSnapshot>,
    state: &mut VersionState,
) {
    let mut all_paths: Vec<BranchSnapshot> = branch_states.to_vec();
    if let Some(nm) = no_match_path {
        all_paths.push(nm.clone());
    }

    if all_paths.len() >= 2 {
        let all_vars: std::collections::HashSet<String> = all_paths
            .iter()
            .flat_map(|(_, s)| s.keys().cloned())
            .collect();

        for var in &all_vars {
            let versions: Vec<(BlockId, u32)> = all_paths
                .iter()
                .map(|(bid, s)| (*bid, s.get(var).copied().unwrap_or(0)))
                .collect();

            let first_ver = versions[0].1;
            if versions.iter().any(|(_, v)| *v != first_ver) {
                let target = state.define(var);
                let sources: Vec<(BlockId, VarRef)> = versions
                    .iter()
                    .map(|(bid, ver)| (*bid, VarRef::new(var, *ver)))
                    .collect();
                ssa.block_mut(merge_block)
                    .phis
                    .push(PhiNode { target, sources });
            }
        }
    } else if all_paths.len() == 1 {
        let (_, ref path_state) = all_paths[0];
        state.restore(path_state);
    }
}
