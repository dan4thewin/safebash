//! Control flow: if/while/for/case → SSA blocks with branches.

use super::expressions::{expr_is_dynamic, expr_to_value, literal_name_of, test_to_ssa};
use super::functions::{lift_int_func_calls, lift_test_captures};
use super::helpers::{insert_phis_two_branch, update_loop_phi_sources};
use super::pipelines::{lower_for_argv_slice, lower_for_array};
use super::statements::lower_stmts;
use super::stmt_handlers::{push_attest_all_check, push_attest_retag};
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
            span: elif.span,
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

    /// The name the attest checks and then cleans, as written. The shared
    /// builders resolve a DD#100 slot themselves.
    fn subject(&self) -> &str {
        match self {
            Self::Scalar(var) => var,
            Self::Coll { array, .. } => array,
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

    // Then: the conversion succeeded — retag the source clean and bind the
    // target to its value.
    push_attest_retag(ssa, state, then_block, source_name);
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

    let then_state = state.snapshot();
    merge_attest_branches(
        i.else_body.as_deref(),
        source_name,
        ssa,
        state,
        block,
        then_block,
        merge_block,
        then_exit,
        &then_state,
        &pre_state,
    );

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
        } => push_attest_all_check(ssa, state, block, array, binders, test, *is_map, true),
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
        // This dispatcher has already selected attest; declining deletes the if.
        unreachable!("a selected if-attest cannot discard its condition or branches");
    };
    // DD#93 Part D in the `if` position: the conversion is the condition.
    // The target is *declared* before the branch (so it exists, defaulted, on
    // the else path — DG 2026-09-07) and *assigned* only in the then-branch,
    // where the conversion is known to have succeeded.
    let test = match pred {
        AttestPredicate::ArithConvert { target, value } => {
            // The classifier requires exactly one scalar name before this predicate.
            let var = literal_name_of(&c.args[0]).to_string();
            let target = target.to_string();
            let value = value.clone();
            let span = c.args.last().map(|a| a.span).unwrap_or_default();
            return lower_if_attest_convert(i, &var, &target, &value, span, ssa, state, block);
        }
        AttestPredicate::Test(test) => test.clone(),
    };

    let kind = AttestKind::of(&form);

    push_if_attest_condition(&kind, &test, ssa, state, block);

    let then_block = ssa.new_block("if_attest.then");
    let merge_block = ssa.new_block("if_attest.merge");

    let pre_state = state.snapshot();

    // Then branch: retag the attested entity clean (only reached on pass).
    push_attest_retag(ssa, state, then_block, kind.subject());
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
        kind.subject(),
        ssa,
        state,
        block,
        then_block,
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
    subject: &str,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    then_block: BlockId,
    merge_block: BlockId,
    then_exit: BlockId,
    then_state: &std::collections::HashMap<String, u32>,
    pre_state: &std::collections::HashMap<String, u32>,
) {
    let else_block = else_body.map_or(merge_block, |_| ssa.new_block("if_attest.else"));
    ssa.block_mut(block).terminator = Terminator::Branch {
        true_block: then_block,
        false_block: else_block,
        condition: "attest".to_string(),
    };

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

    state.restore(pre_state);
    let subject = super::expressions::resolve_args_name(subject, state);
    state.failed_attests.push(subject);
    let else_exit = lower_stmts(else_body, ssa, state, else_block);
    state.failed_attests.pop();
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

    // The condition runs inside the loop as much as the body does: a `break`
    // or `continue` in it targets this loop (SB5081 judges it in-loop too), so
    // the context is pushed before the condition is lowered, not after.
    state.loop_stack.push((header, exit_block));

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
            state.suppressed == 0,
        ));
    }

    // Set loop terminator
    ssa.block_mut(cond_exit).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

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
    // DD#125: a fixed name would clobber a legal source `__forglob` and alias
    // a nested glob loop's array with this one.
    let tmp_name = state.reserve_temp_name("__forglob".to_string(), VarType::String);
    let target = state.define(&tmp_name);
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

/// DD#66 dispatch, evaluated lazily and in source order. The subject is
/// evaluated once, before any pattern. A pattern is evaluated only once every
/// pattern before it has failed to match, so the first match runs its arm and
/// nothing after it is evaluated. A default arm — always last, because the
/// parser appends it and stops — runs iff no pattern matched; without one, no
/// match is a no-op with `$?` = 0.
///
/// Each test is lowered in the block where the one before it missed, which is
/// what makes that order hold for a `$( )` in a pattern: it is lifted where it
/// is evaluated, so it never runs once an earlier arm has matched, and what it
/// does is visible in the arm it selects and to every pattern after it.
pub(super) fn lower_case(
    c: &Case,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let merge = ssa.new_block("dispatch.merge");
    let mut current = block;
    let subject = dispatch_subject(c, ssa, state, &mut current);
    let mut paths = Vec::with_capacity(c.items.len() + 1);
    for (i, item) in c.items.iter().enumerate() {
        let arm = ssa.new_block(&format!("case.{i}"));
        if item.is_default {
            ssa.block_mut(current).terminator = Terminator::Goto(arm);
            let entry = [(current, state.snapshot())];
            paths.push(lower_dispatch_arm(item, arm, &entry, merge, ssa, state));
            insert_case_phis(ssa, merge, &paths, state);
            return merge;
        }
        let entries = test_arm_patterns(c, i, &subject, arm, ssa, state, &mut current);
        paths.push(lower_dispatch_arm(item, arm, &entries, merge, ssa, state));
    }
    // Nothing matched and there is no default: a no-op whose status is 0.
    let zero = state.define("?");
    ssa.block_mut(current).insts.push(SsaInst::Assign {
        target: zero,
        value: SsaValue::Literal("0".to_string()),
    });
    ssa.block_mut(current).terminator = Terminator::Goto(merge);
    paths.push((current, state.snapshot()));
    insert_case_phis(ssa, merge, &paths, state);
    merge
}

type BranchSnapshot = (BlockId, std::collections::HashMap<String, u32>);

/// The subject, evaluated once. A dynamic pattern can run code before it is
/// compared — a `$(f)` may assign a global the subject reads — and both
/// backends read a variable's current storage, so the subject is copied into
/// a temp first and every test compares against the copy.
fn dispatch_subject(
    c: &Case,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    let value = lift_int_func_calls(expr_to_value(&c.value, state), ssa, state, block);
    let dynamic = c
        .items
        .iter()
        .flat_map(|i| &i.patterns)
        .any(expr_is_dynamic);
    if !dynamic || matches!(value, SsaValue::Literal(_)) {
        return value;
    }
    let name = state.reserve_temp_name(
        format!("__dispatch_subject_{}", ssa.blocks.len()),
        VarType::String,
    );
    let target = state.define(&name);
    ssa.block_mut(*block).insts.push(SsaInst::Assign {
        target: target.clone(),
        value,
    });
    SsaValue::Var(target)
}

/// Test arm `i`'s patterns in order, each in the block where the previous test
/// missed, branching to `arm` on a match. Returns the state at every matching
/// edge — the arm's entries — and leaves `current` at the final miss.
fn test_arm_patterns(
    c: &Case,
    i: usize,
    subject: &SsaValue,
    arm: BlockId,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    current: &mut BlockId,
) -> Vec<BranchSnapshot> {
    let patterns = &c.items[i].patterns;
    let mut entries = Vec::with_capacity(patterns.len());
    for (pj, pat) in patterns.iter().enumerate() {
        let test = dispatch_test(subject, pat, &c.op, state);
        let test = lift_test_captures(test, ssa, state, current);
        ssa.block_mut(*current)
            .insts
            .push(SsaInst::Test(test, TestOrigin::Plain));
        let miss = ssa.new_block(&miss_label(c, i, pj));
        ssa.block_mut(*current).terminator = Terminator::Branch {
            condition: "__test".to_string(),
            true_block: arm,
            false_block: miss,
        };
        entries.push((*current, state.snapshot()));
        *current = miss;
    }
    entries
}

/// Where arm `i`'s pattern `pj` goes on a miss: the arm's next pattern, else
/// the next arm's first, else the no-match path.
fn miss_label(c: &Case, i: usize, pj: usize) -> String {
    if pj + 1 < c.items[i].patterns.len() {
        format!("case.{i}.pat.{}", pj + 1)
    } else if i + 1 < c.items.len() {
        format!("case.check.{}", i + 1)
    } else {
        "case.nomatch".to_string()
    }
}

/// Lower one arm's body from the join of its entry edges, exiting to `merge`.
/// Returns the arm's exit as a merge path, and restores the state the chain
/// continues from — the last pattern's miss.
fn lower_dispatch_arm(
    item: &CaseItem,
    arm: BlockId,
    entries: &[BranchSnapshot],
    merge: BlockId,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
) -> BranchSnapshot {
    let miss = state.snapshot();
    join_paths(ssa, arm, entries, state);
    let exit = lower_stmts(&item.body, ssa, state, arm);
    ssa.block_mut(exit).terminator = Terminator::Goto(merge);
    let path = (exit, state.snapshot());
    state.restore(&miss);
    path
}

/// Join `paths` at `block`: continue from the last path's state, and give a
/// phi to every variable the paths disagree about. An empty list joins
/// nothing.
fn join_paths(
    ssa: &mut SsaProgram,
    block: BlockId,
    paths: &[BranchSnapshot],
    state: &mut VersionState,
) {
    if let Some((_, last)) = paths.last() {
        state.restore(last);
    }
    let vars: std::collections::HashSet<String> =
        paths.iter().flat_map(|(_, s)| s.keys().cloned()).collect();
    for var in &vars {
        insert_one_case_phi(ssa, block, paths, var, state);
    }
}

fn dispatch_test(value: &SsaValue, pattern: &Expr, op: &str, state: &VersionState) -> SsaTest {
    // DD#66/#71 dispatch ops (`==` exact / `=~` regex) compare against the
    // operand word: a static pattern is a quote-stripped `Pattern` string; a
    // dynamic (interpolated) pattern is a runtime `Value`.
    let right = if expr_is_dynamic(pattern) {
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

/// Insert phi nodes at the merge block across all incoming paths: one per arm
/// `lower_case` lowered, plus the no-match path when there is no default.
///
/// **There are always at least two paths.** The `else if all_paths.len() == 1`
/// that once stood here was a remnant of the removed `case` construct — its
/// comment said "wildcard-only case", and `case x in *) … ;; esac` was indeed
/// one path. The DD#66 dispatch form that replaced it cannot express that, and
/// the chain is short enough to state in full:
///
/// 1. `parse_dispatch` seeds `items` with one `is_default: false` arm *before*
///    calling `parse_dispatch_arms`, so `items[0]` is never the default and
///    `items.len() >= 1`;
/// 2. `parse_dispatch_arms` is the only place that sets `is_default: true`, and
///    it appends and stops — so a default arm is last and implies
///    `items.len() >= 2`. A default-only dispatch (`[[ $x == || { … } ]]`) is
///    SB4014 at parse;
/// 3. `lower_case` pushes one path per arm, stops after a default, and adds the
///    no-match path only when its loop ran out of arms without meeting one.
///
/// So with a default arm there are `items.len() >= 2` paths, and without one
/// `items.len() + 1 >= 2`. `StmtKind::Case` has a single producer
/// (`parse/parser.rs`'s dispatch form), so there is no second route in.
///
/// The ICE is not a restatement of the grammar's question. The path list is
/// built downstream, by `lower_case`, and the regression it catches is one
/// there: a default met too early, or a no-match path left off, would leave a
/// non-matching value reaching the merge without `$? = 0`. The old one-path
/// branch absorbed exactly that into a plausible-looking `state.restore` and
/// miscompiled in silence.
fn insert_case_phis(
    ssa: &mut SsaProgram,
    merge_block: BlockId,
    paths: &[BranchSnapshot],
    state: &mut VersionState,
) {
    if paths.len() < 2 {
        unreachable!("{DISPATCH_HAS_TWO_PATHS}");
    }
    join_paths(ssa, merge_block, paths, state);
}

/// One variable's phi where dispatch paths join — an arm's entry or the merge —
/// inserted only when the paths disagree about its version. A path with no
/// entry for `var` reads as version 0, which is the pre-dispatch value.
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

    let target = state.define_merge(var);
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
     — the grammar seeds one non-default arm and `lower_case` adds a \
     no-match path unless it met a default arm (which the grammar only \
     appends after another), so one of those two guarantees has been lost";

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
    /// claim that `lower_case` still adds a no-match path whenever there is no
    /// default arm; a regression there would arrive here as one path and used
    /// to be absorbed silently.
    #[test]
    #[should_panic(expected = "fewer than two incoming paths")]
    fn a_single_path_dispatch_is_a_loud_ice() {
        let mut ssa = SsaProgram::new();
        let merge = ssa.new_block("dispatch.merge");
        let arm = ssa.new_block("case.0");
        let mut state = VersionState::with_types(std::collections::HashMap::new());
        let only_path: BranchSnapshot = (arm, state.snapshot());
        insert_case_phis(&mut ssa, merge, &[only_path], &mut state);
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

    #[test]
    #[should_panic(expected = "a selected if-attest cannot discard its condition or branches")]
    fn a_malformed_caught_attest_cannot_disappear() {
        let condition = Statement::command(
            Expr::literal("attest", sp()),
            vec![
                Expr::literal("subject", sp()),
                Expr::literal("not-a-predicate", sp()),
            ],
            sp(),
        );
        let i = If {
            condition: vec![condition],
            then_body: vec![Statement::true_cmd(sp())],
            elif_clauses: vec![],
            else_body: None,
        };
        let mut ssa = SsaProgram::new();
        let block = ssa.new_block("entry");
        let mut state = VersionState::with_types(std::collections::HashMap::new());
        lower_if(&i, &mut ssa, &mut state, block);
    }

    #[test]
    #[should_panic(expected = "fewer than two incoming paths")]
    fn an_empty_dispatch_cannot_silently_pass_through() {
        let c = Case {
            value: Expr::literal("subject", sp()),
            items: vec![],
            op: "==".to_string(),
        };
        let mut ssa = SsaProgram::new();
        let block = ssa.new_block("entry");
        let mut state = VersionState::with_types(std::collections::HashMap::new());
        lower_case(&c, &mut ssa, &mut state, block);
    }

    #[test]
    fn an_argv_slice_does_not_discard_a_substitution_body_or_wrapper() {
        let argv = Statement::command(Expr::literal("argv", sp()), vec![], sp());
        let suppressed = Statement {
            kind: StmtKind::Suppressed(Box::new(argv.clone())),
            span: sp(),
        };
        let redirected = Statement {
            kind: StmtKind::Redirected(Redirected {
                body: Box::new(argv.clone()),
                redirects: vec![],
            }),
            span: sp(),
        };
        for stmts in [
            vec![],
            vec![argv, Statement::true_cmd(sp())],
            vec![suppressed],
            vec![redirected],
        ] {
            let word = Expr {
                kind: ExprKind::CommandSubstitution(stmts),
                span: sp(),
            };
            assert_eq!(argv_splat_start(&word), None, "{word:?}");
        }
    }

    /// `flatten_elif` nests each `elif` as an `If` in the previous clause's
    /// else body. That `If` is a statement in its own right: `lower_one` makes
    /// its span the position of any diagnostic raised about it as a whole, so
    /// it must carry its `elif`'s position. It used to be `Span::default()`,
    /// which prints as 0:0. No spec can see this, because no If-level
    /// transform diagnostic is reachable today and `--trace` prints no
    /// positions.
    #[test]
    fn a_flattened_elif_keeps_its_keyword_position() {
        let clause = |line| ElifClause {
            condition: vec![Statement::true_cmd(Span::new(line, 6))],
            body: vec![Statement::true_cmd(Span::new(line + 1, 3))],
            span: Span::new(line, 3),
        };
        let i = If {
            condition: vec![Statement::true_cmd(Span::new(1, 4))],
            then_body: vec![Statement::true_cmd(Span::new(2, 3))],
            elif_clauses: vec![clause(3), clause(5)],
            else_body: Some(vec![Statement::true_cmd(Span::new(8, 3))]),
        };
        let flat = flatten_elif(&i);
        assert!(flat.elif_clauses.is_empty());

        let mut nested_spans = Vec::new();
        let mut else_body = flat.else_body;
        while let Some(body) = else_body.take() {
            if let [Statement {
                kind: StmtKind::If(nested),
                span,
            }] = body.as_slice()
            {
                nested_spans.push(*span);
                else_body = nested.else_body.clone();
            }
        }
        assert_eq!(nested_spans, [Span::new(3, 3), Span::new(5, 3)]);
    }
}
