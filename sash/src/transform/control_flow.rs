//! Control flow: if/while/for/case → SSA blocks with branches.

use super::expressions::{expr_to_value, literal_name_of, resolve_args_name, test_to_ssa};
use super::helpers::{insert_phis_two_branch, update_loop_phi_sources};
use super::pipelines::{lower_for_argv_slice, lower_for_array};
use super::statements::lower_stmts;
use crate::ir::*;
use crate::ssa::*;

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

/// DD#24: if attest VAR [[ test ]]; then ... fi
/// Non-fatal attest: test branch gets clean var, else branch keeps tainted.
/// Owned decode of an attest form, so the borrow of `c.args` ends before the
/// version state is mutated.
enum AttestKind {
    Scalar(String),
    Coll {
        array: String,
        binders: Vec<String>,
        is_map: bool,
    },
}

impl AttestKind {
    fn of(form: &AttestForm) -> Self {
        match form {
            AttestForm::Scalar { var } => Self::Scalar(var.to_string()),
            AttestForm::Indexed { binder, array } => Self::Coll {
                array: array.to_string(),
                binders: vec![binder.to_string()],
                is_map: false,
            },
            AttestForm::Map { key, val, map } => Self::Coll {
                array: map.to_string(),
                binders: vec![key.to_string(), val.to_string()],
                is_map: true,
            },
        }
    }
}

/// DD#93 Part D in the `if` position: `if attest SOURCE (( TARGET = SOURCE ))`.
///
/// The conversion is the condition — `i?` on the source decides the branch,
/// with no abort (that is what distinguishes the `if` form from the statement
/// form). The target is declared before the branch so it exists on every path
/// (defaulted, DG 2026-09-07) and bound to the converted value only in the
/// then-branch, alongside the clean retag of the source.
#[allow(clippy::too_many_arguments)]
fn lower_if_attest_convert(
    i: &If,
    source_name: &str,
    target_name: &str,
    value: &ArithExpr,
    span: Span,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // Declared-but-unassigned (DG 2026-09-07): define the target before the
    // branch so a read on the else path (or after the `if`) finds the name.
    // `0` is what a declared-but-unassigned int reads as everywhere else in
    // sash (`int x; echo "$x $(( x ))"` → `0 0`), so the else path agrees with
    // the rest of the language rather than yielding an empty arithmetic value.
    let declared = state.define(target_name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: declared,
        value: SsaValue::Literal("0".to_string()),
    });

    let source = state.lookup(source_name);
    ssa.block_mut(block).insts.push(SsaInst::Test(
        SsaTest::Unary {
            op: "i?".to_string(),
            operand: SsaValue::Var(source.clone()),
        },
        TestOrigin::AttestPredicate,
    ));

    let then_block = ssa.new_block("if_attest_convert.then");
    let merge_block = ssa.new_block("if_attest_convert.merge");
    let pre_state = state.snapshot();

    ssa.block_mut(block).terminator = Terminator::Branch {
        true_block: then_block,
        false_block: merge_block,
        condition: "attest".to_string(),
    };

    // Then: the conversion succeeded — retag the source clean and bind the
    // target to its value.
    let source_ref = state.lookup(source_name);
    let clean_source = state.define(source_name);
    ssa.block_mut(then_block).insts.push(SsaInst::AttestRetag {
        target: clean_source,
        source: source_ref,
    });
    super::stmt_handlers::push_attest_convert_bind(
        ssa,
        state,
        then_block,
        target_name,
        value,
        span,
    );

    let then_exit = lower_stmts(&i.then_body, ssa, state, then_block);
    ssa.block_mut(then_exit).terminator = Terminator::Goto(merge_block);

    if let Some(else_body) = &i.else_body {
        let else_block = ssa.new_block("if_attest_convert.else");
        if let Terminator::Branch { false_block, .. } = &mut ssa.block_mut(block).terminator {
            *false_block = else_block;
        }
        state.restore(&pre_state);
        let else_exit = lower_stmts(else_body, ssa, state, else_block);
        ssa.block_mut(else_exit).terminator = Terminator::Goto(merge_block);
    } else {
        state.restore(&pre_state);
    }

    merge_block
}

/// Emit the attest check into the condition block, leaving pass/fail in the
/// test-result var for the branch that follows.
fn push_if_attest_condition(
    kind: &AttestKind,
    test: &TestExpr,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    match kind {
        AttestKind::Scalar(_) => {
            ssa.block_mut(block).insts.push(SsaInst::Test(
                test_to_ssa(test, state),
                TestOrigin::AttestPredicate,
            ));
        }
        AttestKind::Coll {
            array,
            binders,
            is_map,
        } => {
            // DD#100: the if-form attest-all resolves its slot too.
            let source = state.lookup(&resolve_args_name(array, state));
            // Fresh, unique binder versions (see push_attest_all); restore so
            // the binders stay loop-local.
            let snap = state.snapshot();
            let binder_refs: Vec<VarRef> = binders.iter().map(|b| state.define(b)).collect();
            let ssa_test = test_to_ssa(test, state);
            state.restore(&snap);
            ssa.block_mut(block).insts.push(SsaInst::AttestAll {
                source,
                binders: binder_refs,
                test: ssa_test,
                is_map: *is_map,
                if_form: true,
            });
        }
    }
}

/// Emit the clean-retag instruction into the pass block of an `if attest`.
///
/// Both arms define a *new version* of the subject, valid only on the passing
/// path. Codegen must therefore treat that version as an alias of the source
/// rather than a separately-defined value — see
/// `codegen/dispatch.rs::emit_attest_retag`.
fn push_attest_retag(
    kind: &AttestKind,
    test: &crate::ir::TestExpr,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    then_block: BlockId,
) {
    match kind {
        AttestKind::Scalar(var) => {
            let source_ref = state.lookup(var);
            // Resolve the predicate before defining the target version so
            // `$var` binds to the source (emit_attest evaluates the test
            // before the target slot exists). See dispatch.rs::emit_attest.
            let ssa_test = test_to_ssa(test, state);
            let target_ref = state.define(var);
            ssa.block_mut(then_block).insts.push(SsaInst::Attest {
                target: target_ref,
                source: source_ref,
                test: ssa_test,
            });
        }
        AttestKind::Coll { array, .. } => {
            let source_ref = state.lookup(array);
            let target_ref = state.define(array);
            ssa.block_mut(then_block).insts.push(SsaInst::AttestRetag {
                target: target_ref,
                source: source_ref,
            });
        }
    }
}

fn lower_if_attest(
    i: &If,
    c: &crate::ir::Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let Some((form, pred)) = crate::ir::classify_attest(&c.args) else {
        return block;
    };
    // DD#93 Part D in the `if` position: the conversion is the condition.
    // The target is *declared* before the branch (so it exists, defaulted, on
    // the else path — DG 2026-09-07) and *assigned* only in the then-branch,
    // where the conversion is known to have succeeded.
    if let (AttestForm::Scalar { var }, AttestPredicate::ArithConvert { target, value }) =
        (&form, &pred)
    {
        let (var, target) = (var.to_string(), target.to_string());
        let value = (*value).clone();
        let span = c.args.last().map(|a| a.span).unwrap_or_default();
        return lower_if_attest_convert(i, &var, &target, &value, span, ssa, state, block);
    }
    let AttestPredicate::Test(test) = pred else {
        return block;
    };
    let test = test.clone();

    let kind = AttestKind::of(&form);

    push_if_attest_condition(&kind, &test, ssa, state, block);

    let then_block = ssa.new_block("if_attest.then");
    let merge_block = ssa.new_block("if_attest.merge");

    let pre_state = state.snapshot();

    // Branch on test result
    ssa.block_mut(block).terminator = Terminator::Branch {
        true_block: then_block,
        false_block: merge_block,
        condition: "attest".to_string(),
    };

    // Then branch: retag the attested entity clean (only reached on pass).
    push_attest_retag(&kind, &test, ssa, state, then_block);
    let then_exit = lower_stmts(&i.then_body, ssa, state, then_block);
    ssa.block_mut(then_exit).terminator = Terminator::Goto(merge_block);
    let then_state = state.snapshot();

    // Merge like any other two-branch construct. DD#121 R2 is what makes this
    // safe *and* necessary: a caught attest "must leave its subject clean on
    // every path that falls through to the merge", so a clean version always
    // reaches here — from the then-branch's retag, from the else-branch's
    // resolution, or from the one surviving path when the other diverts.
    //
    // An earlier version restored `pre_state` instead, on the stated premise
    // that SB5089 left "no clean version to merge to". That reads R2 backwards:
    // R2 is precisely the guarantee that there *is* one. It also did not do
    // what it said — the restore only ran in the no-else case, so an `else`
    // leaked its own version to the merge and code after the `if` read a
    // version defined on exactly one path.
    merge_attest_branches(
        i.else_body.as_deref(),
        ssa,
        state,
        block,
        merge_block,
        then_exit,
        &then_state,
        &pre_state,
    );

    merge_block
}

/// Lower a caught attest's else path and merge it with the then path.
///
/// Split out of `lower_if_attest` to keep that function under the complexity
/// gate; the nesting here is the decision, not incidental.
#[allow(clippy::too_many_arguments)]
fn merge_attest_branches(
    else_body: Option<&[Statement]>,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    merge_block: BlockId,
    then_exit: BlockId,
    then_state: &std::collections::HashMap<String, u32>,
    pre_state: &std::collections::HashMap<String, u32>,
) {
    let Some(else_body) = else_body else {
        // SB5089 rejects this shape, but `--no-taint` reaches it: merge the
        // then-branch against the untaken fall-through edge from `block`.
        insert_phis_two_branch(
            state,
            ssa,
            merge_block,
            then_exit,
            then_state,
            block,
            pre_state,
        );
        return;
    };

    let else_block = ssa.new_block("if_attest.else");
    if let Terminator::Branch { false_block, .. } = &mut ssa.block_mut(block).terminator {
        *false_block = else_block;
    }
    state.restore(pre_state);
    let else_exit = lower_stmts(else_body, ssa, state, else_block);
    ssa.block_mut(else_exit).terminator = Terminator::Goto(merge_block);
    let else_state = state.snapshot();

    if !crate::validate::block_falls_through(else_body) {
        // R2's exemption: the else path exits/returns/breaks/continues, so it
        // is not a predecessor of the merge in any sense that matters. The
        // then-branch's state is the merge's state outright, and no phi is
        // emitted — which keeps the attested version off any path where it is
        // not defined.
        state.restore(then_state);
        return;
    }

    insert_phis_two_branch(
        state,
        ssa,
        merge_block,
        then_exit,
        then_state,
        else_exit,
        &else_state,
    );
}

/// DD#24's if-attest form: a condition that is exactly one `attest` command.
///
/// Extracted from `lower_if` so the detection is flat rather than three
/// nested `if`s — the shape the complexity gate reads as hidden nesting.
fn if_attest_condition(i: &If) -> Option<&Command> {
    let [cond] = i.condition.as_slice() else {
        return None;
    };
    super::statements::attest_command(cond)
}

pub(super) fn lower_if(
    i: &If,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#24: detect if-attest conditional form
    if let Some(c) = if_attest_condition(i) {
        return lower_if_attest(i, c, ssa, state, block);
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
    let header = ssa.new_checkpoint_block("while.header");
    let body_block = ssa.new_block("while.body");
    let exit_block = ssa.new_block("while.exit");

    // Wire entry → header
    ssa.block_mut(block).terminator = Terminator::Goto(header);

    // Snapshot state before loop
    let pre_state = state.snapshot();

    // === Two-pass loop lowering ===
    // Pass 1: Lower body to discover which variables are modified.
    //
    // Pass 1's *instructions* are discarded below, so any state it sets that
    // gates emission must be rolled back too — otherwise pass 2 takes a
    // different path and emits different code. `collections`/`assoc_arrays`
    // are the guard-then-emit pair: `autodeclare_bulk_target` skips the
    // empty-collection Assign once the name is registered, which would leave
    // a bulk-read target (`read ARR`) with no table to fill.
    let _state_at_header = state.snapshot();
    let pre_collections = state.collections.clone();
    let pre_assoc_arrays = state.assoc_arrays.clone();
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
    state.collections = pre_collections;
    state.assoc_arrays = pre_assoc_arrays;

    // Lower condition in header using post-phi state
    state.in_condition += 1;
    let cond_exit = lower_stmts(&w.condition, ssa, state, header);
    state.in_condition -= 1;

    // `until` = `while ! cond`: negate the condition's result once, in the
    // block where the condition FINISHES. That is `cond_exit`, not `header`.
    // For `[[ ]]`, `(( ))` and a plain command the two are the same block,
    // which is why those forms were correct; a user-function condition is
    // inlined into fresh blocks, so `header` ends with a `Goto` into the
    // callee and the negate landed *before* the condition ran — on the
    // previous iteration's value, which the callee then overwrote. `until f`
    // silently behaved exactly like `while f`.
    //
    // This also covers `cmd | until readline v { }`, whose lexical reader
    // binding is installed by its enclosing pipeline consumer.
    if w.until {
        ssa.block_mut(cond_exit).insts.push(SsaInst::internal(
            BuiltinCmd::NegateTestResult,
            vec![],
            true,
        ));
    }

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
    update_loop_phi_sources(ssa, header, &body_state_2);

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
///
/// **The body must be exactly one undecorated statement.** This was
/// `stmts.first()`, which made `for x in $(argv; /usr/bin/touch f)` an
/// argv-slice loop and silently discarded the `touch` — `lower_for_argv_slice`
/// lowers the slice, not the body.
///
/// It also used to look *through* `StmtKind::Redirected`, which was the same
/// bug wearing the other hat: `for x in $(argv >f)` compiled, iterated the
/// arguments and never created the file. Narrowing to one statement had fixed
/// only the half anyone had thought to test. A redirected body now falls to
/// the `Command` match below and answers `None`.
///
/// `validate` rejects both shapes (SB5025); this is the half that made them
/// wrong, and it stays narrow so the two cannot drift back apart.
fn argv_splat_start(word: &Expr) -> Option<i64> {
    let ExprKind::CommandSubstitution(stmts) = &word.kind else {
        return None;
    };
    let [stmt] = stmts.as_slice() else {
        return None;
    };
    let StmtKind::Command(c) = &stmt.kind else {
        return None;
    };
    if literal_name_of(&c.name) != "argv" {
        return None;
    }
    match c.args.first() {
        None => Some(0),
        Some(a) => literal_name_of(a).parse::<i64>().ok().filter(|n| *n >= 0),
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
    let it = detect_array_iter(&f.words[0].kind)?;
    let name = super::expressions::resolve_args_name(&it.name, state);
    Some(lower_for_array(
        f,
        &name,
        it.use_keys,
        it.vouched,
        ssa,
        state,
        block,
    ))
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
    // A glob materialises into a fresh temp array; nothing to vouch for.
    Some(lower_for_array(
        f, &tmp_name, false, false, ssa, state, block,
    ))
}

/// What a `for`-in source word iterates, if it iterates a collection.
///
/// A struct rather than a tuple because `use_keys` and `vouched` are adjacent
/// booleans with unrelated meanings — the call site should have to name them.
pub(super) struct ArrayIter {
    pub name: String,
    /// `for k in $(keys m)` binds the key; `for v in @a` the value.
    pub use_keys: bool,
    /// DD#91: the `!` taint-off modifier, `for v in @{a:!}`.
    pub vouched: bool,
}

/// Recognise the two for-in sources that iterate a collection: `@a` (the
/// `arrvals` splat) and `$(keys m)`.
///
/// The `$(keys m)` shape is spelled out once, in `ir::for_head_keys`, and read
/// here. This function used to re-derive it with four `unreachable!` guards
/// quoting SB5009 and SB4045 — a duplicate of the gate's own predicate over the
/// same `ExprKind` the gate had already seen, which is the kind of backstop
/// that cannot catch anything the gate would not have caught itself.
///
/// Two arms were deleted here (2026-10-03) as DD#116 *rivals*: a
/// `DoubleQuoted` arm that recursed, so `for v in "@a"` would have iterated,
/// and a `Redirected` arm, so `$(keys m < f)` would have. Both contradicted an
/// authoritative rejection upstream — SB5026 and SB5073 — and an execution
/// probe over the whole spec suite confirmed neither could run.
fn detect_array_iter(kind: &ExprKind) -> Option<ArrayIter> {
    match kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            Some(ArrayIter {
                name: pe.name.clone(),
                use_keys: false,
                vouched: pe.taint_off,
            })
        }
        // Sash-native: $(keys arr) in for-loop → array key iteration (DD#41)
        ExprKind::CommandSubstitution(stmts) => {
            // `$(keys m)` has no splat to carry a modifier, so never vouched.
            let map = crate::ir::for_head_keys(stmts)?;
            Some(ArrayIter {
                name: literal_name_of(map).to_string(),
                use_keys: true,
                vouched: false,
            })
        }
        _ => None,
    }
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
    let iter_blocks: Vec<BlockId> = f
        .words
        .iter()
        .map(|_| ssa.new_checkpoint_block("for.iter"))
        .collect();

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
    ssa.block_mut(current).insts.push(SsaInst::Test(
        dispatch_test(value, pattern, op, state),
        TestOrigin::Plain,
    ));
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
        // Every pattern but the last falls through to a fresh block for the
        // next one; the last one's false edge leaves the arm. Advancing
        // `pat_current` unconditionally is safe — on the last iteration the
        // loop ends before it is read again.
        let false_block = if pi + 1 < patterns.len() {
            ssa.new_block(&format!("case.{}.pat.{}", arm_idx, pi + 1))
        } else {
            next_check
        };
        emit_pattern_link(
            ssa,
            pat_current,
            value,
            pat,
            op,
            state,
            case_block,
            false_block,
        );
        pat_current = false_block;
    }
}

/// One link of a multi-pattern arm: test `pat` against the subject, branch to
/// the arm body on a match and to `false_block` otherwise.
#[allow(clippy::too_many_arguments)]
fn emit_pattern_link(
    ssa: &mut SsaProgram,
    block: BlockId,
    value: &SsaValue,
    pat: &Expr,
    op: &str,
    state: &VersionState,
    case_block: BlockId,
    false_block: BlockId,
) {
    ssa.block_mut(block).insts.push(SsaInst::Test(
        dispatch_test(value, pat, op, state),
        TestOrigin::Plain,
    ));
    ssa.block_mut(block).terminator = Terminator::Branch {
        condition: "__test".to_string(),
        true_block: case_block,
        false_block,
    };
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
/// optional no-match).
///
/// **There are always at least two paths.** The `else if all_paths.len() == 1`
/// that stood here was a remnant of the removed `case` construct — its comment
/// said "wildcard-only case", and `case x in *) … ;; esac` was indeed one path.
/// The DD#66 dispatch form that replaced it cannot express that, and the chain
/// is short enough to state in full:
///
/// 1. `parse_dispatch` seeds `items` with one `is_default: false` arm *before*
///    calling `parse_dispatch_arms`, so `items[0]` is never the default and
///    `items.len() >= 1`;
/// 2. `parse_dispatch_arms` is the only place that sets `is_default: true`, and
///    it appends — so any default arm implies `items.len() >= 2`. A
///    default-only dispatch (`[[ $x == || { … } ]]`) is SB4014 at parse;
/// 3. `lower_case` returns early when `items.is_empty()`;
/// 4. `build_case_bodies` pushes exactly one snapshot per item;
/// 5. `wire_no_match_path` returns `None` **iff** some item is the default.
///
/// So with a default arm `all_paths == items.len() >= 2`, and without one
/// `all_paths == items.len() + 1 >= 2`. `StmtKind::Case` has a single producer
/// (`parse/parser.rs`'s dispatch form), so there is no second route in.
///
/// The ICE is not a restatement of the grammar's question. `all_paths` is
/// computed downstream by `build_case_bodies` and `wire_no_match_path`, and
/// the regression it catches is one in *those*: a `has_wildcard` test that
/// wrongly reported a default would skip wiring the no-match block, so a
/// non-matching value would reach the merge without `$? = 0`. The old branch
/// absorbed exactly that into a plausible-looking `state.restore` and
/// miscompiled in silence.
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

    if all_paths.len() < 2 {
        unreachable!("{DISPATCH_HAS_TWO_PATHS}");
    }

    let all_vars: std::collections::HashSet<String> = all_paths
        .iter()
        .flat_map(|(_, s)| s.keys().cloned())
        .collect();

    for var in &all_vars {
        insert_one_case_phi(ssa, merge_block, &all_paths, var, state);
    }
}

/// One variable's phi at a dispatch merge, inserted only when the paths
/// disagree about its version. A path with no entry for `var` reads as
/// version 0, which is the pre-dispatch value.
fn insert_one_case_phi(
    ssa: &mut SsaProgram,
    merge_block: BlockId,
    all_paths: &[BranchSnapshot],
    var: &str,
    state: &mut VersionState,
) {
    let versions: Vec<(BlockId, u32)> = all_paths
        .iter()
        .map(|(bid, s)| (*bid, s.get(var).copied().unwrap_or(0)))
        .collect();

    let first_ver = versions[0].1;
    if versions.iter().all(|(_, v)| *v == first_ver) {
        return;
    }

    let target = state.define(var);
    let sources: Vec<(BlockId, VarRef)> = versions
        .iter()
        .map(|(bid, ver)| (*bid, VarRef::new(var, *ver)))
        .collect();
    ssa.block_mut(merge_block)
        .phis
        .push(PhiNode { target, sources });
}

const DISPATCH_HAS_TWO_PATHS: &str =
    "ICE: a dispatch reached the merge with fewer than two incoming paths \
     — the grammar seeds one non-default arm and `wire_no_match_path` adds \
     a no-match path unless a default arm exists (which implies a second \
     arm), so one of those two guarantees has been lost";

#[cfg(test)]
mod invariant_tests {
    use super::*;

    fn sp() -> Span {
        Span::new(1, 1)
    }

    /// A dispatch always reaches `insert_case_phis` with at least two paths:
    /// one non-default arm the grammar seeds, plus either a second arm (when a
    /// `||` default is present) or the synthesised no-match path. The branch
    /// that used to handle one path was a leftover from the removed `case`
    /// construct, where `case x in *) … ;; esac` really was single-path.
    ///
    /// No spec can make this assertion — the parser rejects the only source
    /// spelling that would produce it (`[[ $x == || { … } ]]` is SB4014), so
    /// the shape has to be handed to the function directly. The ICE is the
    /// claim that `wire_no_match_path` still wires a no-match path whenever
    /// there is no default arm; a regression there would arrive here as one
    /// path and used to be absorbed silently.
    #[test]
    #[should_panic(expected = "fewer than two incoming paths")]
    fn a_single_path_dispatch_is_a_loud_ice() {
        let mut ssa = SsaProgram::new();
        let merge = ssa.new_block("dispatch.merge");
        let arm = ssa.new_block("case.0");
        let mut state = VersionState::with_types(std::collections::HashMap::new());
        let only_path: BranchSnapshot = (arm, state.snapshot());
        insert_case_phis(&mut ssa, merge, &[only_path], None, &mut state);
    }

    /// Hand `detect_array_iter` the four for-head shapes SB5009 and SB4045
    /// reject, and assert each is declined rather than mis-lowered.
    ///
    /// These ICE'd between 2026-10-04 and 2026-10-05, on the reasoning that a
    /// `return None` would be silently wrong if SB5009 narrowed. The panics
    /// went because they re-asked SB5009's own question about the same
    /// `ExprKind` SB5009 had already inspected — a duplicate predicate, free to
    /// drift from the rule it quoted and unable to catch anything the gate
    /// would not have caught itself.
    ///
    /// What this asserts now is the property that actually matters, and the one
    /// the ICEs were reaching for: an unrecognised substitution body is **not**
    /// mistaken for a collection. Declining it routes the head to the word-list
    /// path, which SB5009 has already vetted. No spec can make this assertion,
    /// because every shape here is rejected before transform runs.
    #[test]
    fn a_for_head_cmdsub_that_is_not_keys_is_declined() {
        let non_command = ExprKind::CommandSubstitution(vec![Statement {
            kind: StmtKind::Suppressed(Box::new(Statement::true_cmd(sp()))),
            span: sp(),
        }]);
        let non_literal_head = ExprKind::CommandSubstitution(vec![Statement::command(
            Expr {
                kind: ExprKind::SingleQuoted("keys".to_string()),
                span: sp(),
            },
            vec![Expr::literal("m", sp())],
            sp(),
        )]);
        let not_keys = ExprKind::CommandSubstitution(vec![Statement::command(
            Expr::literal("cat", sp()),
            vec![Expr::literal("f", sp())],
            sp(),
        )]);
        let keys_no_args = ExprKind::CommandSubstitution(vec![Statement::command(
            Expr::literal("keys", sp()),
            vec![],
            sp(),
        )]);
        let two_statements = ExprKind::CommandSubstitution(vec![
            Statement::command(
                Expr::literal("cat", sp()),
                vec![Expr::literal("f", sp())],
                sp(),
            ),
            Statement::command(
                Expr::literal("keys", sp()),
                vec![Expr::literal("m", sp())],
                sp(),
            ),
        ]);

        for (what, kind) in [
            ("a non-command statement (SB5009)", non_command),
            ("a non-literal command name (SB5009)", non_literal_head),
            ("a literal that is not `keys` (SB5009)", not_keys),
            ("a zero-argument `keys` (SB4045)", keys_no_args),
            ("`keys` among several statements (SB5073)", two_statements),
        ] {
            assert!(
                detect_array_iter(&kind).is_none(),
                "{what} must be declined, not lowered as a collection"
            );
        }
    }

    /// The companion: the arm these guards shadow is *warm*, so it must keep
    /// answering. `for x in a b c` has an ordinary word-list head and reaches
    /// `_ => None` (measured count 3 in the spec suite), and `$(keys m)` is
    /// the live array-key iteration this function exists to detect.
    #[test]
    fn the_live_arms_still_answer() {
        assert!(detect_array_iter(&ExprKind::Literal("a".to_string())).is_none());

        let keys = ExprKind::CommandSubstitution(vec![Statement::command(
            Expr::literal("keys", sp()),
            vec![Expr::literal("m", sp())],
            sp(),
        )]);
        let it = detect_array_iter(&keys).expect("$(keys m) is an array iteration");
        assert_eq!(it.name, "m");
        assert!(it.use_keys);
    }
}
