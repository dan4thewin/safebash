//! Extracted handlers for the largest StmtKind match arms.

use super::expressions::{
    arg_to_value, expr_to_value, literal_name_of, resolve_args_name, subscript_to_value,
    test_to_ssa,
};
use super::functions::{lift_int_func_args, lift_int_func_calls, needs_lift};
use super::resolution::resolve_pipeline_command;
use super::statements::{lower_one, spill_scalar_redirect_targets};
use crate::ir::*;
use crate::ssa::*;

/// Handle StmtKind::Command — command execution, function inlining, attest, regex.
///
/// This is the largest arm because commands can be:
/// - User-defined function calls (inlined at the call site)
/// - Attest statements (runtime taint validation)
/// - Regex mutation (var =~ s/re/str/flags)
/// - External commands (emitted as SsaInst::Command)
pub(super) fn handle_command(
    c: &Command,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    let name = literal_name_of(&c.name);
    let kind = BuiltinCmd::from_name(name);

    // DD#70 (B): `match` and a bare `[[ … ]]` are booleans that must be
    // consumed — a bare or `-`-suppressed one is a compile error.
    check_boolean_must_consume(kind, stmt, state);

    // DD#55/#126: a bare `wait` discards the pid it reaped. Checked here
    // rather than beside the user-function case below because `try_builtin`
    // consumes the statement first and the check would never be reached.
    check_non_void_bare_wait(kind, stmt, state);

    // DD#63: the 3-arg out-array form `match S P ARR` (the 2-arg boolean
    // and `$(match …)` value forms fall through).
    if let Some(b) = try_handle_match_into(kind, c, ssa, state, block) {
        return b;
    }

    if let Some(b) = try_builtin(kind, name, c, stmt, ssa, state, &mut block) {
        return b;
    }

    check_non_void_bare_call(name, stmt, state);
    super::cmd_handlers::handle_function_call_or_command(name, c, stmt, ssa, state, block)
}

/// Dispatch a statement builtin to its handler. A selected builtin consumes
/// the statement; unrelated kinds fall through to general command handling.
fn try_builtin(
    kind: BuiltinCmd,
    name: &str,
    c: &Command,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> Option<BlockId> {
    match kind {
        BuiltinCmd::Attest => Some(handle_attest_stmt(c, ssa, state, *block)),
        BuiltinCmd::Test => Some(handle_test_command(c, ssa, state, *block)),
        BuiltinCmd::Break => Some(handle_break_stmt(c, state, ssa, *block)),
        BuiltinCmd::Continue => Some(handle_continue_stmt(c, state, ssa, *block)),
        BuiltinCmd::Return => Some(handle_return_stmt(c, ssa, state, block)),
        BuiltinCmd::Argv => Some(handle_argv(c, ssa, state, *block)),
        BuiltinCmd::Keys => Some(handle_keys(c, ssa, state, *block)),
        BuiltinCmd::Signal => Some(super::cmd_handlers::handle_signal(
            name, c, stmt, ssa, state, *block,
        )),
        BuiltinCmd::Unset => Some(super::cmd_handlers::handle_unset(
            name, c, stmt, ssa, state, *block,
        )),
        BuiltinCmd::Read => Some(super::cmd_handlers::handle_read(c, ssa, state, *block)),
        _ => None,
    }
}

/// DD#63: `match S P ARR` out-array form (exactly 3 args). Builds a
/// `MatchInto` inst that fills the named array with `[whole, g1..gN]` and
/// sets the boolean result. Returns `Some` only for the 3-arg form; the
/// 2-arg boolean `match` and the `$(match …)` value form fall through
/// (`None`) to the existing paths. Must-consume (SB4018/SB4019) is checked
/// by the caller via `check_match_must_consume`, keyed on the kind, so it
/// applies to this form too.
fn try_handle_match_into(
    kind: BuiltinCmd,
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    if !matches!(kind, BuiltinCmd::Match | BuiltinCmd::MatchAll) || c.args.len() != 3 {
        return None;
    }
    let subject = expr_to_value(&c.args[0], state);
    let pattern = expr_to_value(&c.args[1], state);
    let array = super::expressions::fill_target_name(&c.args[2], state);
    // DD#63: the out-array form fills (and auto-declares) its trailing
    // array, for singular `match` exactly as for `matchall`.
    autodeclare_bulk_target(&array, ssa, state, block);
    ssa.block_mut(block).insts.push(SsaInst::MatchInto {
        subject,
        pattern,
        array: array.clone(),
        all: kind == BuiltinCmd::MatchAll,
    });
    if state.failed_attests.contains(&array) {
        push_attest_retag(ssa, state, block, &array);
    }
    Some(block)
}

/// `parse_test_command` produces exactly one Test operand. Once dispatch has
/// selected `[[`, declining or ignoring extra operands would discard it.
fn handle_test_command(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let [Expr {
        kind: ExprKind::Test(test_expr),
        ..
    }] = c.args.as_slice()
    else {
        unreachable!("a selected [[ command requires exactly one Test operand");
    };
    lower_test_tree(test_expr, ssa, state, block)
}

/// DD#95: `&&` and `||` inside `[[ ]]` short-circuit. The right operand — every
/// capture it lifts and every expansion that can abort — runs only when the
/// left one has not already decided the result. Lifting the whole test before
/// one `Test` ran `[[ -n $z && $(f) == y ]]`'s `f` even when `$z` was empty.
///
/// Each operand is its own `Test`, reading the versions current at its own
/// point, and the branch is the statement-level `&&`/`||` shape without that
/// form's status rules: a `[[ … ]]` never aborts, and the operand that ran last
/// has already set both the result and `$?`. A subtree with no junction keeps
/// the single `Test` it always had.
fn lower_test_tree(
    test: &TestExpr,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    match test {
        TestExpr::And(left, right) => lower_test_junction(left, right, true, ssa, state, block),
        TestExpr::Or(left, right) => lower_test_junction(left, right, false, ssa, state, block),
        TestExpr::Not(inner) if test_has_junction(inner) => {
            // DD#70's sigil follows the operand it negates and flips both the
            // result and `$?`, as `!` does for a pipeline.
            let end = lower_test_tree(inner, ssa, state, block);
            ssa.block_mut(end).insts.push(SsaInst::internal(
                BuiltinCmd::NegateTestResult,
                vec![],
                true,
            ));
            end
        }
        TestExpr::Not(_) | TestExpr::Unary { .. } | TestExpr::Binary { .. } => {
            lower_test_operand(test, ssa, state, block)
        }
    }
}

fn test_has_junction(test: &TestExpr) -> bool {
    match test {
        TestExpr::And(..) | TestExpr::Or(..) => true,
        TestExpr::Not(inner) => test_has_junction(inner),
        TestExpr::Unary { .. } | TestExpr::Binary { .. } => false,
    }
}

/// `left && right` / `left || right` inside `[[ ]]`. A capture can write a
/// global, so the merge carries phis for the SSA taint backstop exactly as an
/// `if` without `else` does: the skipped path keeps the left operand's versions.
fn lower_test_junction(
    left: &TestExpr,
    right: &TestExpr,
    is_and: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let tag = if is_and { "test.and" } else { "test.or" };
    let left_end = lower_test_tree(left, ssa, state, block);
    let left_state = state.snapshot();
    let right_block = ssa.new_block(&format!("{tag}.right"));
    let merge = ssa.new_block(&format!("{tag}.merge"));
    let (true_block, false_block) = match is_and {
        true => (right_block, merge),
        false => (merge, right_block),
    };
    ssa.block_mut(left_end).terminator = Terminator::Branch {
        condition: left_end.to_string(),
        true_block,
        false_block,
    };
    let right_end = lower_test_tree(right, ssa, state, right_block);
    let right_state = state.snapshot();
    ssa.block_mut(right_end).terminator = Terminator::Goto(merge);
    super::helpers::insert_phis_two_branch(
        state,
        ssa,
        merge,
        left_end,
        &left_state,
        right_end,
        &right_state,
    );
    merge
}

/// One junction-free operand: a single `Test`. A capture may open a new block,
/// so the `Test` goes where the lift ends.
fn lower_test_operand(
    test: &TestExpr,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    let ssa_test = test_to_ssa(test, state);
    let ssa_test = super::functions::lift_test_captures(ssa_test, ssa, state, &mut block);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::Test(ssa_test, TestOrigin::Plain));
    block
}

/// `var s/pattern/replacement/flags` — bash-style sed substitution
/// against the named variable. DD#39 compile-time regex validation.
/// Single-arg form only. Returns `None` when the arg does not start
/// with `s/` or the substitution body is malformed.
/// DD#126: `wait` returns a pid, so a bare `wait` statement drops a value.
/// Same rule and same remedy as SB4016 on a `-> int` function — `- wait` is
/// the "reap one, I don't care which" spelling.
///
/// Unlike the function case there is no `in_condition` escape. A boolean
/// context consumes a *status*, and `wait`'s value is a pid: `if wait { … }`
/// would read "the pid was 0", which is never true. That is DD#74's reasoning
/// for int functions, applied to the one int builtin.
fn check_non_void_bare_wait(kind: BuiltinCmd, stmt: &Statement, state: &mut VersionState) {
    if kind != BuiltinCmd::Wait || state.suppressed > 0 {
        return;
    }
    state.diagnostics.push(format!(
        "{}:{}: error [SB4016] `wait` returns the pid it reaped — capture it \
         with `$(wait)` or `$(( wait ))`, test it with `(( (wait) > 0 ))`, or \
         discard it with `- wait`",
        stmt.span.line, stmt.span.col
    ));
}

/// DD#55/SB4016: a bare call of a non-void function in statement position
/// must be handled — an `-> int` function's int captured by `$((…))`, a
/// status function's status by `if`/`||` — or discarded with `-`. The class
/// is that of the definition the call runs (`VersionState::call_class`).
/// Emits a diagnostic when violated; does not affect codegen (the caller
/// still threads the call through).
fn check_non_void_bare_call(name: &str, stmt: &Statement, state: &mut VersionState) {
    // DD#70 addendum: `- f` discards the returned value, like any other
    // result, and a condition handles it.
    if state.suppressed > 0 || state.in_condition > 0 {
        return;
    }
    // DD#74: an `-> int` function is rejected in *every* boolean context
    // (SB4030), so `if`/`||` is not advice the author could act on for one.
    // A status function's status is what `if`/`||` consume.
    let advice = match state.call_class(name) {
        Some(FnClass::Int) => {
            format!("returns an int — capture with $(({name})) or discard with `-`")
        }
        Some(FnClass::Status) => {
            "returns a status — handle it with `if` or `||`, or discard it with `-`".to_string()
        }
        Some(FnClass::Void) | None => return,
    };
    state.diagnostics.push(format!(
        "{}:{}: error [SB4016] function '{}' {}",
        stmt.span.line, stmt.span.col, name, advice
    ));
}

/// DD#70 (B): `match` / `matchall` / a bare `[[ … ]]` are booleans that
/// **must be consumed**. A consuming position (if/while/&&/||) consumes it,
/// and `|| :` or `-` discards it.
///
/// DD#70 addendum (2026-09-09): `-` used to be rejected here (SB4019) on the
/// grounds that a (B) construct has no runtime verdict to suppress. But
/// `|| :` was always allowed, so the rule never protected the result — it
/// only dictated *which* explicit discard the author had to write, and
/// mandated the oblique one (`|| :` achieves discard as a side effect of
/// short-circuit control flow; `-` says it directly). What SB4018 actually
/// guards against is the **silent** discard, and `-` is not silent.
fn check_boolean_must_consume(kind: BuiltinCmd, stmt: &Statement, state: &mut VersionState) {
    let construct = match kind {
        BuiltinCmd::Match => "match",
        BuiltinCmd::MatchAll => "matchall",
        BuiltinCmd::Test => "[[ … ]]",
        _ => return,
    };
    if state.in_condition > 0 || state.suppressed > 0 {
        return;
    }
    state.diagnostics.push(format!(
        "{}:{}: error [SB4018] `{}` result is unused — consume it with if/`&&`/`||`, or discard with `-` or `|| :`",
        stmt.span.line, stmt.span.col, construct
    ));
}

/// DD#93 Part D: lower `attest SOURCE (( TARGET = SOURCE ))`.
///
/// Two steps onto existing instructions, so no backend learns a new form:
///  1. an `Attest` gated on `i?` — the same strict-i64 grammar the conversion
///     needs, so a failure aborts with the usual attest-fail message, and a
///     pass yields a clean version of the source;
///  2. an ordinary int assignment of the (now validated) source to the target,
///     exactly what `(( TARGET = SOURCE ))` would have emitted on its own.
///
/// The `i?` gate is what makes step 2 safe: `sash_atoi` cannot overflow on a
/// value that has already been range-checked.
#[allow(clippy::too_many_arguments)]
pub(super) fn push_attest_arith_convert(
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    source_name: &str,
    target_name: &str,
    value: &crate::ir::ArithExpr,
    span: crate::ir::Span,
) {
    let source = state.lookup(source_name);
    let test = SsaTest::Unary {
        op: "i?".to_string(),
        operand: SsaValue::Var(source.clone()),
    };
    let clean_source = state.define(source_name);
    ssa.block_mut(block).insts.push(SsaInst::Attest {
        target: clean_source,
        source,
        test,
    });
    push_attest_convert_bind(ssa, state, block, target_name, value, span);
}

/// The binding half of the DD#93 Part D form: `TARGET = $(( EXPR ))` evaluated
/// against the current (post-gate, clean) version of the source. `EXPR` is the
/// predicate's own right-hand side, so a wider RHS (`(( t = s * 2 ))`) binds the
/// computed value rather than the bare conversion.
pub(super) fn push_attest_convert_bind(
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    target_name: &str,
    value: &crate::ir::ArithExpr,
    span: crate::ir::Span,
) {
    let value = expr_to_value(
        &Expr {
            kind: ExprKind::Arithmetic(Box::new(value.clone())),
            span,
        },
        state,
    );
    let target = state.define(target_name);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::Assign { target, value });
}

fn handle_attest_stmt(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#24 scalar / DD#61 array & map attest forms.
    //
    // `attest` is a reserved word (`Tok::Attest`), so every occurrence in
    // statement position goes through `parse_attest`, which fixes the shape
    // before lowering — a malformed one is SB4014 at parse time, and SB5096
    // rejects a quoted head. No unclassifiable `attest` Command reaches here.
    let Some((form, pred)) = classify_attest(&c.args) else {
        unreachable!("`attest` shape is fixed by parse_attest before lowering (SB4014)")
    };
    let test = match pred {
        // DD#93 Part D: the classifier requires one scalar conversion source.
        AttestPredicate::ArithConvert { target, value } => {
            let var = literal_name_of(&c.args[0]).to_string();
            let target = target.to_string();
            let value = value.clone();
            let span = c.args.last().map(|a| a.span).unwrap_or_default();
            push_attest_arith_convert(ssa, state, block, &var, &target, &value, span);
            return block;
        }
        AttestPredicate::Test(test) => test.clone(),
    };
    match form {
        AttestForm::Scalar { var } => {
            let var = var.to_string();
            let source = state.lookup(&var);
            // Resolve the predicate's operands BEFORE defining the target
            // version, so `$var` in the test binds to the source (the
            // value being attested). emit_attest evaluates the test before
            // the target slot exists. See dispatch.rs::emit_attest.
            let ssa_test = test_to_ssa(&test, state);
            let target = state.define(&var);
            ssa.block_mut(block).insts.push(SsaInst::Attest {
                target,
                source,
                test: ssa_test,
            });
        }
        AttestForm::Indexed { binder, array } => {
            push_attest_all(
                ssa,
                state,
                block,
                array,
                vec![binder.to_string()],
                test,
                false,
            );
        }
        AttestForm::Map { key, val, map } => {
            let binders = vec![key.to_string(), val.to_string()];
            push_attest_all(ssa, state, block, map, binders, test, true);
        }
    }
    block
}

/// DD#61: emit a statement-form attest-all — `AttestAll` (exits on a failing
/// element) followed by `AttestRetag` (produces the clean version).
fn push_attest_all(
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    array: &str,
    binders: Vec<String>,
    test: crate::ir::TestExpr,
    is_map: bool,
) {
    push_attest_all_check(ssa, state, block, array, &binders, &test, is_map, false);
    push_attest_retag(ssa, state, block, array);
}

/// DD#61: evaluate an attest-all predicate over every element of `array`.
///
/// The one builder for both forms: the statement form (`if_form: false`)
/// aborts on a failing element; the `if` form (`if_form: true`) leaves the
/// all-passed boolean for the branch that follows.
///
/// DD#100: `attest v in A` attests the *caller's* collection — the deferral
/// Part E describes. Unresolved, this ICEd on an undefined array `A`.
#[allow(clippy::too_many_arguments)]
pub(super) fn push_attest_all_check(
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    array: &str,
    binders: &[String],
    test: &crate::ir::TestExpr,
    is_map: bool,
    if_form: bool,
) {
    let source = state.lookup(&resolve_args_name(array, state));
    // Define the binders so they receive globally-unique SSA versions,
    // resolve the predicate against them, then restore current versions so
    // the binders stay loop-local (DD#61 shadow semantics). The version
    // counter is not rewound by restore, so the binder versions remain
    // unique and match the slots emit_attest_all creates for them.
    let snap = state.snapshot();
    let binder_refs: Vec<VarRef> = binders.iter().map(|b| state.define(b)).collect();
    let ssa_test = test_to_ssa(test, state);
    state.restore(&snap);
    ssa.block_mut(block).insts.push(SsaInst::AttestAll {
        source,
        binders: binder_refs,
        test: ssa_test,
        is_map,
        if_form,
    });
}

/// Retag an attest's subject clean once its predicate has passed.
///
/// The one builder for every form whose predicate has already been
/// evaluated: the statement attest-all, after its `AttestAll`, and every
/// caught `if attest`, in its pass block. It only relabels. The caught
/// scalar form used to re-evaluate its predicate here through a second,
/// aborting `Attest`, so a file test stat'd its path twice and, if the
/// answer changed in between, the *caught* form aborted through the hard
/// gate instead of taking its else branch.
///
/// It resolves `subject` through any DD#100 slot exactly as the check does
/// (a scalar resolves to itself). When only the check resolved, the caught
/// attest-all retagged the slot name — a version nothing defines — so
/// native codegen ICEd for want of a slot, or a later read of the slot
/// failed the SSA taint backstop on a program the validator had accepted.
///
/// The new version is valid only on the passing path, so codegen treats it
/// as an alias of the source — see `codegen/dispatch.rs::emit_attest_retag`.
pub(super) fn push_attest_retag(
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    subject: &str,
) {
    let subject = resolve_args_name(subject, state);
    let source = state.lookup(&subject);
    let target = state.define(&subject);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::AttestRetag { target, source });
}

/// The loop-stack entry a `break`/`continue` count selects, or `None` during
/// pass 1 of `lower_while`.
///
/// `break` and `continue` are twins here — same count, same clamp, same stack —
/// so they share this rather than keeping two copies of it. The copies had
/// already drifted: `continue N` was uncovered by any spec while `break 2` was
/// specified, which is how the count faults SB5097 now rejects survived in both.
///
/// **An empty stack is not an error.** `lower_while` lowers the body twice —
/// pass 1 only to discover which variables the body modifies, its instructions
/// then discarded — and pushes the loop context after pass 1, just before the
/// condition (which belongs to the loop as much as the body does). So a
/// `break`/`continue` inside a `while` body is reached once with no entry for
/// its own loop, and the right answer at an empty stack is to emit nothing and
/// keep lowering into the same block. (`for` pushes before its single pass,
/// which is why this only shows up under `while`.) SB5081 is what guarantees a
/// *genuinely* unenclosed one never gets here.
///
/// A count larger than the nesting depth clamps to the outermost loop, which is
/// also what bash does (`continue 9` inside one loop continues that loop).
/// SB5097 guarantees a literal decimal of at least 1, so the clamp is always a
/// valid index; saturating on overflow keeps a huge literal clamping, where
/// `unwrap_or(1)` used to turn every unparsable count into `continue 1`.
fn loop_target(c: &Command, state: &VersionState) -> Option<(BlockId, BlockId)> {
    let levels = c.args.first().map(literal_name_of);
    crate::ir::loop_jump_index(levels, state.loop_stack.len()).map(|i| state.loop_stack[i])
}

fn handle_break_stmt(
    c: &Command,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> BlockId {
    let Some((_header, exit)) = loop_target(c, state) else {
        return block;
    };
    ssa.block_mut(block).terminator = Terminator::Goto(exit);
    ssa.new_block("after_break")
}

fn handle_continue_stmt(
    c: &Command,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> BlockId {
    let Some((header, _exit)) = loop_target(c, state) else {
        return block;
    };
    ssa.block_mut(block).terminator = Terminator::Goto(header);
    ssa.new_block("after_continue")
}

fn handle_return_stmt(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> BlockId {
    let exit_val = if let Some(arg) = c.args.first() {
        let val = expr_to_value(arg, state);
        // DD#27/§5: `return $(( (F …) ))` where F is the enclosing function is a
        // self tail-call — lower to a loop back-edge instead of inlining.
        if let Some(after) = try_int_tail_call(&val, ssa, state, block) {
            return after;
        }
        lift_int_func_calls(val, ssa, state, block)
    } else {
        SsaValue::Literal("0".to_string())
    };
    emit_return_value(exit_val, ssa, state, *block)
}

fn emit_return_value(
    exit_val: SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let es_var = state.define("?");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: es_var,
        value: exit_val,
    });
    if let Some(target) = state.return_target {
        ssa.block_mut(block).terminator = Terminator::Goto(target);
    } else {
        ssa.block_mut(block).terminator = Terminator::Return;
    }
    ssa.new_block("after_return")
}

/// DD#27/§5: if `val` is a self tail-call to the enclosing function
/// (`return $(( (F …) ))`), emit the loop back-edge and return the trailing
/// block. `None` when it is not a tail self-call (the caller lifts normally).
fn try_int_tail_call(
    val: &SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> Option<BlockId> {
    let unwrapped;
    let val = if let SsaValue::CmdSubstBody { body } = val {
        unwrapped = super::functions::suppressed_capture_value(body, state)?;
        &unwrapped
    } else {
        val
    };
    lower_int_tail_call(val, ssa, state, block)
}

fn lower_int_tail_call(
    val: &SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> Option<BlockId> {
    let SsaValue::IntFuncCall { name, args, colls } = val else {
        return None;
    };
    // DD#100: the tail loop rebinds only `args`, not the collection slots, so a
    // tail self-call that passes `=name` is not a tail call here. Falling
    // through routes it to `inline_int_func`, which rejects it as recursion
    // rather than silently reusing the caller's slot bindings.
    if !colls.is_empty() {
        return None;
    }
    let tail = state.tail_loop.as_ref()?;
    if *name != tail.func {
        return None;
    }
    let (header, aname) = (tail.header, tail.args_name.clone());
    // DD#81 Part B checks arity at *every* call site. A self-tail call lowers
    // to this back edge rather than through `inline_int_func`, so it is
    // checked here.
    let body = state
        .function_bodies
        .get(name)
        .cloned()
        .expect("a self-tail loop is open only while its own function body is lowered");
    super::cmd_handlers::check_callee_arity(name, &body, args, state);
    let lifted = lift_int_func_args(args.clone(), ssa, state, block);
    Some(super::functions::emit_tail_call(
        header, &aname, lifted, ssa, state, *block,
    ))
}

/// DD#60: `argv [N] a` out-param builtin. The trailing positional is the
/// destination array; an optional leading non-negative integer is the
/// slice start (`argv a` = whole argv, `argv N a` = argv[N:]). Emits an
/// `ArgvFill` instruction (auto-declares + taints the array, like `read`).
/// Returns `None` for any other shape.
/// `argv [N] a` (DD#60).
///
/// `validate::check_argv_form` (SB4045/SB5085) has already fixed the shape:
/// one or two arguments, and a bare non-negative integer literal index that
/// fits `i64`. Lowering therefore cannot fail, and says so.
///
/// This used to return `Option` and yield `None` for any shape it could not
/// lower. `None` in `try_builtin` means "not a builtin", so the statement
/// became an ordinary command named `argv`: it compiled clean and the binary
/// died with `sash: bug: bare command passed to sash_exec`. Eight spellings
/// reached that, `argv "1" a` among them.
fn handle_argv(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let args: Vec<String> = c
        .args
        .iter()
        .map(|a| literal_name_of(a).to_string())
        .collect();
    let Some(target) = args.last() else {
        unreachable!("SB4045 rejects a zero-argument `argv`")
    };
    let array = super::expressions::resolve_args_name(target, state);
    let start = match args.len() {
        1 => 0,
        2 => match args[0].parse::<i64>() {
            Ok(n) => n,
            Err(_) => unreachable!("SB5085 rejects a non-integer `argv` index"),
        },
        _ => unreachable!("SB4045 rejects `argv` with more than two arguments"),
    };
    // Auto-declare (DG 2026-07-08): if `a` isn't already an array, create
    // the versioned slot the same way `array a=()` does, so the fill and later
    // len/iteration/element reads agree with no prior `array a=()`.
    autodeclare_bulk_target(&array, ssa, state, block);
    // Define a LATER version than the autodeclare's empty-array Assign, so the
    // SSA taint backstop resolves this array to the fill's taint, not to the
    // clean empty array the declaration produced.
    let target = state.define(&array);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::ArgvFill { target, start });
    block
}

/// DD#75 §3: `keys m k` out-param builtin — snapshot map `m`'s string keys
/// into array `k`. Emits a `KeysFill`, which auto-declares the target exactly
/// as `ArgvFill` does.
///
/// `validate::check_keys_form` (SB5073/SB4045) has already fixed the shape to
/// exactly two arguments, so lowering cannot fail and says so rather than
/// returning `None`. That is not caution for its own sake: `None` here means
/// "not a builtin", which would turn the statement into an ordinary command
/// named `keys` that compiles clean and dies with `sash: bug: bare command
/// passed to sash_exec` — the failure eight `argv` spellings reached before
/// `handle_argv` was given the same treatment.
///
/// The one-argument `$(keys m)` splat never arrives here: it is an
/// *expression*, resolved by `detect_array_iter` in the for-in header.
fn handle_keys(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let [map, dest] = &c.args[..] else {
        unreachable!("SB5073/SB4045 reject every `keys` statement but `keys m k`")
    };
    let source = super::expressions::resolve_args_name(literal_name_of(map), state);
    let array = super::expressions::resolve_args_name(literal_name_of(dest), state);
    autodeclare_bulk_target(&array, ssa, state, block);
    // Later uses refer to the fill, not its empty initialization.
    let target = state.define_replacement(&array);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::KeysFill { source, target });
    block
}

/// Auto-declare a bulk-read target array (mirrors argv, DG 2026-07-08): if
/// `name` isn't already a collection, emit the empty-array Assign that
/// creates the versioned `name_0` slot — so the fill (`argv`/`read`) and any
/// later `len`/iteration/element read resolve without a prior `array
/// name=()`. Without this the codegen `find_array_slot` panics (ICE).
pub(super) fn autodeclare_bulk_target(
    name: &str,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    if state.collections.contains(name) {
        return;
    }
    state.collections.insert(name.to_string());
    let target = state.define(name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target,
        value: SsaValue::IndexedArray(vec![]),
    });
}

/// Handle StmtKind::Pipeline — multi-command pipe chains.
pub(super) fn handle_pipeline(
    p: &Pipeline,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#70: `! X` is a (B) must-consume boolean — bare/consequent → SB4018
    // (`- ! X` is SB4019 at parse). And `!` *consumes* its operand, so lower
    // it in a boolean context (an inner `[[ … ]]`/`match` must not itself
    // fire — the `!` is its consumer).
    if !p.negated {
        return lower_pipeline_body(p, ssa, state, block);
    }
    check_negation_must_consume(stmt, state);
    state.in_condition += 1;
    let block = lower_pipeline_body(p, ssa, state, block);
    state.in_condition -= 1;
    ssa.block_mut(block).insts.push(SsaInst::internal(
        BuiltinCmd::NegateTestResult,
        vec![],
        state.suppressed == 0,
    ));
    block
}

/// Lower the pipeline's stages (ignoring negation, handled by the caller):
/// dispatch multi-stage forms (while/mapfile/lastpipe/regular) or lower a
/// single command directly.
fn lower_pipeline_body(
    p: &Pipeline,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    if p.commands.len() > 1 {
        return dispatch_multistage_pipeline(p, ssa, state, block);
    }
    for cmd in &p.commands {
        block = lower_one(cmd, ssa, state, block);
    }
    block
}

/// Route a genuine multi-stage pipeline (`len > 1`) to the matching lowering
/// based on its final stage beneath that stage's `-`/`!` marks: a `while` or
/// `read` consumer, a `readline` consumer, or a regular fork/exec chain.
///
/// The marks are peeled first. Classifying the raw stage sent
/// `a | - readline x` down the fork/exec path as a bare `readline` exec, which
/// died at run time, and `a | - while readline x { … }` into the
/// compound-stage ICE.
fn dispatch_multistage_pipeline(
    p: &Pipeline,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let last = FinalStage::peel(&p.commands[p.commands.len() - 1], state);
    if last.consumes_as_statement() {
        return handle_pipe_while_or_array_read(p, last, ssa, state, block);
    }
    if let Some(command) = lastpipe_read_command(last.body) {
        handle_lastpipe_read(p, command, last, ssa, state, block)
    } else {
        handle_regular_pipeline(p, ssa, state, block);
        block
    }
}

/// A pipeline's final stage beneath its DD#55 `-` and DD#70 `!` marks.
#[derive(Clone, Copy)]
struct FinalStage<'a> {
    body: &'a Statement,
    suppressed: bool,
    negated: bool,
}

impl<'a> FinalStage<'a> {
    /// DD#70: a terminal `!` is the same must-consume boolean a standalone
    /// `! cmd` is, so it is SB4018 wherever nothing consumes it — at
    /// statement level and in a background child alike.
    fn peel(stage: &'a Statement, state: &mut VersionState) -> Self {
        let (body, suppressed, negated) = stage.stage_marks();
        if negated {
            check_negation_must_consume(stage, state);
        }
        Self {
            body,
            suppressed,
            negated,
        }
    }

    /// A `while` or `read` consumer lowers as a statement bound to the pipe.
    ///
    /// Structural on purpose: DD#122/SB4073 has already rejected a `while`
    /// final stage with no reader in it, so reaching here means the loop
    /// really does consume the pipe. This predicate used to be the *only*
    /// thing deciding it, which is how `cmd | while (( n < 1 )); do /bin/cat;
    /// done` compiled and silently discarded the pipeline's output.
    ///
    /// A `! while` final stage is SB4022 at parse
    /// (`reject_nonconsuming_final_stage`): the loop's status never joins the
    /// pipeline's, so the `!` has nothing to invert. The `!self.negated` test
    /// stays as a backstop — left unclassified, a negated loop reaches the
    /// regular path, whose stage lowering ICEs on any compound.
    fn consumes_as_statement(&self) -> bool {
        let is_while = matches!(self.body.kind, StmtKind::While(_)) && !self.negated;
        is_while || is_array_read_last(self.body)
    }

    /// Whether the consumer's own verdict joins the producers' status: a
    /// reader's EOF test does. A completed `while` succeeds, and DD#55
    /// discards a `-` consumer's verdict.
    fn verdict_joins(&self) -> bool {
        !self.suppressed && !matches!(self.body.kind, StmtKind::While(_))
    }

    /// Lower the consumer beneath its marks. DD#55: a `-` consumer's own
    /// commands run unchecked, as a statement-level `- while`'s do; their
    /// operands and the function bodies they run stay checked (DD#55 2026-10-30).
    /// DD#70: a `!` reader's verdict is inverted before it joins the
    /// producers'.
    fn lower(
        &self,
        ssa: &mut SsaProgram,
        state: &mut VersionState,
        block: BlockId,
        body: impl FnOnce(&mut SsaProgram, &mut VersionState, BlockId) -> BlockId,
    ) -> BlockId {
        if self.suppressed {
            state.suppressed += 1;
        }
        let exit = body(ssa, state, block);
        if self.suppressed {
            state.suppressed -= 1;
        }
        if self.negated {
            ssa.block_mut(exit).insts.push(SsaInst::internal(
                BuiltinCmd::NegateTestResult,
                vec![],
                state.suppressed == 0,
            ));
        }
        exit
    }
}

/// DD#70 (B): `! X` produces a must-consume boolean (for any operand). A
/// consuming position (if/while/&&/||) is fine, and so is a `!` nested in a
/// `-` scope (`- { ! cmd }`), which discards it; a bare result is SB4018.
/// `- ! cmd` itself is SB4019 at parse, so the advice drops the `!`.
fn check_negation_must_consume(stmt: &Statement, state: &mut VersionState) {
    if state.in_condition > 0 || state.suppressed > 0 {
        return;
    }
    state.diagnostics.push(format!(
        "{}:{}: error [SB4018] `!` result is unused — consume it with if/`&&`/`||`, or write `- cmd` without the `!`",
        stmt.span.line, stmt.span.col
    ));
}

/// The ARRAY-filling lastpipe consumers. DD#99 stage 6: `read` is now the
/// array form, so it belongs here rather than with the scalar reads —
/// `readline` is the only scalar one left.
fn is_array_read_last(last: &Statement) -> bool {
    match &last.kind {
        StmtKind::Command(c) => literal_name_of(&c.name) == "read",
        _ => false,
    }
}

/// The SCALAR lastpipe consumer — `readline NAME` only (DD#99).
fn lastpipe_read_command(last: &Statement) -> Option<&Command> {
    match &last.kind {
        StmtKind::Command(c) if literal_name_of(&c.name) == "readline" => Some(c),
        _ => None,
    }
}

/// Build ordinary and capturing pipelines through the same stage classifier,
/// preserving suppression, negation and forked (function and command) stages.
fn build_pipeline_inst(
    cmds: &[Statement],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    reader_site: Option<u32>,
) -> SsaInst {
    let mut stages = vec![];
    let mut suppressed = vec![];
    let mut negated = vec![];
    for (idx, cmd) in cmds.iter().enumerate() {
        push_pipeline_stage(
            cmd,
            idx,
            ssa,
            state,
            block,
            &mut stages,
            &mut suppressed,
            &mut negated,
        );
    }
    SsaInst::Pipeline {
        stages,
        capture: reader_site.map_or(PipelineCapture::None, PipelineCapture::ReadFd),
        checked: state.in_condition == 0 && state.suppressed == 0,
        suppressed,
        negated,
    }
}

/// Push a `Command` pipeline stage into `stages`: a known function inlines as a
/// forked compound child (`__compound_<id>`, args bound); anything else resolves
/// (DD#23 compile-time PATH) to a direct stage, or to a forked child of its own
/// when an argument needs lifting (see `command_pipeline_stage`). Shared
/// by the regular pipeline and the producer-stage (pipe-while-read / lastpipe)
/// builders.
fn push_command_stage(
    c: &Command,
    redirects: Vec<Redirect>,
    idx: usize,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    stages: &mut Vec<(String, Vec<SsaValue>, Vec<Redirect>)>,
) {
    // Spill `${x:-f}` / `$a[i]` stage-redirect targets to a synthetic scalar
    // (assigned into `block`, before the Pipeline inst) exactly as a top-level
    // redirect does, so codegen's stage-redirect lowering only sees scalar
    // reads it can resolve (was: silently redirected to an empty filename).
    let redirects = spill_scalar_redirect_targets(&redirects, ssa, state, block);
    let name = literal_name_of(&c.name);
    if state.known_functions.contains(name) {
        if let Some(stage) = build_function_pipeline_stage(name, c, ssa, state, idx) {
            stages.push((stage, vec![], redirects));
            return;
        }
    }
    // DD#81 Part A: reaching here means this stage resolved to an EXTERNAL —
    // either the name is not a function at all, or it is one whose `fn` has not
    // been lowered yet (`known_functions` is a whole-program pre-pass,
    // `function_bodies` is execution-order, so a forward call is in the first
    // and not the second). Record the use so a later `fn NAME` raises SB5041,
    // exactly as the plain-command path does at `cmd_handlers.rs`.
    //
    // Without this, `echo hi | up` before `fn up { … }` passed SB5011 — which
    // exempts `up` because *validate's* function table is whole-program — and
    // then compiled to a bare `exec("up")`, dying at runtime with the runtime's
    // own "bare command passed to sash_exec" and status 127.
    if !state.lowering_fn_template {
        state.externally_used.borrow_mut().insert(name.to_string());
    }
    let args: Vec<SsaValue> = c.args.iter().map(|a| arg_to_value(a, state)).collect();
    // DD#99: a `write` stage is an `echo -n` stage (see rewrite_write_as_echo).
    let (name, args) = super::cmd_handlers::rewrite_write_as_echo(name, args);
    let command = resolve_pipeline_command(&name, state);
    let (stage, args) = command_pipeline_stage(command, args, ssa, state, idx);
    stages.push((stage, args, redirects));
}

/// `cmd1 | ... | while readline var` or `cmd1 | ... | read arr`, each
/// possibly marked (`| - while …`, `| ! read arr`).
/// Register producer stages and lower the consumer against its owned pipe site.
fn handle_pipe_while_or_array_read(
    p: &Pipeline,
    last: FinalStage,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let site = state.next_reader_site;
    state.next_reader_site += 1;
    let pipeline = build_pipeline_inst(
        &p.commands[..p.commands.len() - 1],
        ssa,
        state,
        block,
        Some(site),
    );
    ssa.block_mut(block).insts.push(pipeline);
    lower_pipe_consumer(ssa, state, block, site, last, |ssa, state, block| {
        lower_one(last.body, ssa, state, block)
    })
}

fn lower_pipe_consumer(
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    site: u32,
    stage: FinalStage,
    body: impl FnOnce(&mut SsaProgram, &mut VersionState, BlockId) -> BlockId,
) -> BlockId {
    let before = state.snapshot();
    let consumer = ssa.new_block("pipe.consumer");
    let done = ssa.new_block("pipe.done");
    ssa.block_mut(block).insts.push(SsaInst::internal(
        BuiltinCmd::SetTestFromPipe,
        vec![SsaValue::Literal(reader_pipe_fd(site).to_string())],
        state.suppressed == 0,
    ));
    ssa.block_mut(block).terminator = Terminator::Branch {
        condition: "pipe-ready".into(),
        true_block: consumer,
        false_block: done,
    };
    let pipe_reader = state.pipe_reader.replace(reader_pipe_fd(site));
    let exit = stage.lower(ssa, state, consumer, body);
    state.pipe_reader = pipe_reader;
    ssa.block_mut(exit).terminator = Terminator::Goto(done);
    let after = state.snapshot();
    super::helpers::insert_phis_two_branch(state, ssa, done, exit, &after, block, &before);
    state.reader_regions.push(ReaderRegion {
        site,
        entry: consumer,
        done,
        candidates: ssa
            .blocks
            .range(consumer..)
            .map(|(&id, _)| id)
            .filter(|&id| id != done)
            .collect(),
        checked: state.in_condition == 0 && state.suppressed == 0,
        verdict_joins: stage.verdict_joins(),
    });
    done
}

/// DD#20: lastpipe — `cmd | read var` runs `read` in the main shell.
/// Emit producer stages with captured pipe fd, then emit a ReadLine that
/// consumes from that pipe in the main process, beneath the reader's own
/// `-`/`!` mark.
fn handle_lastpipe_read(
    p: &Pipeline,
    command: &Command,
    last: FinalStage,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let producers = &p.commands[..p.commands.len() - 1];
    let site = state.next_reader_site;
    state.next_reader_site += 1;
    let pipeline = build_pipeline_inst(producers, ssa, state, block, Some(site));
    ssa.block_mut(block).insts.push(pipeline);
    lower_pipe_consumer(ssa, state, block, site, last, |ssa, state, block| {
        let target = state.define(&crate::ir::readline_target(&command.args));
        ssa.block_mut(block).insts.push(SsaInst::ReadLine {
            target,
            fd: reader_pipe_fd(site),
            redirects: vec![],
        });
        block
    })
}

/// Regular pipeline: `cmd1 | cmd2 | ...`. A function-call stage, and a
/// command stage whose arguments need lifting, becomes a forked child —
/// lowered into its own SSA blocks and recorded in
/// `SsaProgram::compound_stages` for codegen to fork and run.
fn handle_regular_pipeline(
    p: &Pipeline,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    let pipeline = build_pipeline_inst(&p.commands, ssa, state, block, None);
    ssa.block_mut(block).insts.push(pipeline);
}

/// Lower one pipeline stage `cmd` and extend the `stages`/`suppressed`/`negated`
/// masks. DD#55: a `-`-suppressed stage still RUNS (real pipe stage) — only its
/// status is excluded from the aggregate. DD#70: a `!`-negated stage runs with
/// its status inverted. One source stage may lower to several SSA stages
/// (function inlining), so each added stage inherits the mask.
#[allow(clippy::too_many_arguments)]
fn push_pipeline_stage(
    cmd: &Statement,
    idx: usize,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    stages: &mut Vec<(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)>,
    suppressed: &mut Vec<bool>,
    negated: &mut Vec<bool>,
) {
    let (unwrapped, is_suppressed, is_negated) = cmd.stage_marks();
    let stages_before = stages.len();
    lower_one_pipeline_stage(unwrapped, idx, ssa, state, block, stages);
    suppressed.resize(stages.len(), false);
    negated.resize(stages.len(), false);
    suppressed[stages_before..].fill(is_suppressed);
    negated[stages_before..].fill(is_negated);
}

/// Lower a single (already `-`/`!`-peeled) pipeline stage into `stages`.
/// It is always a `Command`: an external or builtin becomes a direct stage,
/// or a forked child when an argument needs lifting (see
/// `command_pipeline_stage`); a function call becomes a forked child
/// (see `build_function_pipeline_stage`). The parser rejects every compound
/// stage.
fn lower_one_pipeline_stage(
    stmt: &Statement,
    idx: usize,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    stages: &mut Vec<(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)>,
) {
    let (inner, redirects) = match &stmt.kind {
        StmtKind::Redirected(r) => (&r.body.kind, r.redirects.clone()),
        other => (other, vec![]),
    };
    let StmtKind::Command(c) = inner else {
        unreachable!("{COMPOUND_STAGE_IS_REJECTED_AT_PARSE}");
    };
    push_command_stage(c, redirects, idx, ssa, state, block, stages);
}

/// Every pipeline stage is a `Command`, because the *parser* says so — this is
/// shadowed one pass earlier than most, not by `validate`. Measured 2026-10-11
/// over eight spellings, all rejected before the transform runs: a `{ }` group
/// first or last in the pipe, a `while`/`if`/`for` head, and `(( … ))` are
/// each **SB4022**; a redirected group is SB4021; `case` is SB4029. A `return`
/// producer that is a compound — the comma group `return (( a, b ))` builds —
/// does not pass through `reject_compound_postfix`, and reached this ICE as
/// `return (( a, b )) | cat`; `wrap_pipeline_part` now gives it SB4022 too
/// (2026-10-27).
///
/// The arm this replaced was not a wrong answer — it was a complete, plausible
/// lowering (fork a compound child as `__compound_<id>`) that had never once
/// run. That is the dangerous shape: relax SB4022 and the route opens *in
/// silence*, with no spec having ever exercised it. The sibling machinery is
/// still live and still tested — `build_function_pipeline_stage` forks a
/// child for a function-call stage — so nothing is lost by making this entry
/// say what it knows.
const COMPOUND_STAGE_IS_REJECTED_AT_PARSE: &str =
    "ICE: a compound command reached a pipeline stage — SB4022 rejects one at \
     parse, so a stage is always a Command; the compound-stage lowering was \
     never exercised and must be written knowingly";

/// A `Command` pipeline stage that names a known function: the call is inlined
/// into a forked child of its own, starting at a fresh entry block recorded in
/// [`SsaProgram::compound_stages`], and the child exits with the call's status.
/// It is an ordinary call in every other respect — the shared inliner evaluates
/// the arguments (inside the child), checks arity, binds `args` and collection
/// slots, targets `return`, turns a self tail-call into a loop and refuses
/// recursion. This used to be a private copy of that inliner, and the copy
/// lacked the tail-call loop: a self-tail-recursive stage reported SB5037.
/// Returns the stage name, or `None` if the body is not registered yet (a
/// forward reference, which resolves as an external).
fn build_function_pipeline_stage(
    name: &str,
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    idx: usize,
) -> Option<String> {
    let body = state.function_bodies.get(name).cloned()?;
    if state.call_class(name) == Some(FnClass::Int) {
        state.error_at_stmt(format!(
            "[SB4030] int function `{name}` used as a pipeline stage — integer values are not process statuses, even with `-`"
        ));
    }
    let entry = ssa.new_checkpoint_block(&format!("pipe_fn_{}_{}", name, idx));
    let parent = state.enter_forked_child();
    let end = super::cmd_handlers::inline_function_call(name, &body, c, ssa, state, entry);
    ssa.block_mut(end).terminator = Terminator::ChildExit;
    state.exit_forked_child(parent);
    Some(ssa.compound_stages.record(entry, name))
}

/// A resolved command's pipeline stage, as the `(name, args)` it contributes
/// to [`SsaInst::Pipeline`]. A direct stage's arguments are evaluated by
/// codegen where it forks, with no block to lift into, so when an argument
/// needs lifting ([`needs_lift`]: a function call, or a capture codegen cannot
/// evaluate in place) the stage forks a child of its own instead, recorded in
/// [`SsaProgram::compound_stages`] as a function stage is, and takes no
/// arguments. The child lifts the arguments and then runs the command as a
/// direct stage would: `echo` and `:` in place, anything else by exec. The
/// stage's redirects apply before the child's entry, so a capture among the
/// arguments runs under them (a statement evaluates its captures before its
/// redirects). A failure reports the command's resolved path, as a direct
/// stage's does.
fn command_pipeline_stage(
    command: String,
    args: Vec<SsaValue>,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    idx: usize,
) -> (String, Vec<SsaValue>) {
    if !args.iter().any(needs_lift) {
        return (command, args);
    }
    // The label omits the command: a head too dynamic to judge (`*?[$`) may
    // contain `*/`, and the C backend prints block labels inside a comment.
    let entry = ssa.new_checkpoint_block(&format!("pipe_cmd_{idx}"));
    let parent = state.enter_forked_child();
    let mut block = entry;
    let args = lift_int_func_args(args, ssa, state, &mut block);
    let inst = if matches!(
        BuiltinCmd::from_name(&command),
        BuiltinCmd::Echo | BuiltinCmd::Noop
    ) {
        SsaInst::command(command.as_str(), args, vec![], state.suppressed == 0)
    } else {
        let argv = std::iter::once(SsaValue::Literal(command.clone()));
        SsaInst::command(
            "exec",
            argv.chain(args).collect(),
            vec![],
            state.suppressed == 0,
        )
    };
    ssa.block_mut(block).insts.push(inst);
    ssa.block_mut(block).terminator = Terminator::ChildExit;
    state.exit_forked_child(parent);
    (ssa.compound_stages.record(entry, &command), vec![])
}

/// Handle StmtKind::Assignment — variable assignment with optional function collision check.
pub(super) fn handle_assignment(
    a: &Assignment,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    check_var_func_collision(&a.name, stmt, state);
    check_bare_collection_literal(a, stmt, state);
    if let Some(ref idx) = a.index {
        return handle_subscript_assign(a, idx, state, ssa, block);
    }
    if a.op == crate::ir::AssignOp::Append {
        return handle_append_assign(a, state, ssa, block);
    }
    handle_plain_assign(a, state, ssa, block)
}

/// DD#97: a collection is introduced by the `array`/`map` keyword. A bare
/// `f=(…)`/`f+=(…)` on a name that is not already a collection used to compile
/// and only half-work: element reads and `for-in` were fine, but the name was
/// never registered, so `len` read the table as a string (0), `join`/`unset`
/// raised SB4030, and the append form reached codegen as an ICE ("undefined
/// array"). Re-assigning an already-declared collection stays legal — the rule
/// is about *introducing* one without the keyword.
///
/// Both literal shapes count. `([key]=value)` was missing from this match, so
/// `str s=([b]=2)` silently emptied `s` instead of being told to declare a
/// `map`; it is the same mistake as `str s=(x)` and gets the same answer.
fn check_bare_collection_literal(a: &Assignment, stmt: &Statement, state: &mut VersionState) {
    if a.index.is_some() || !matches!(a.value.kind, ExprKind::Array(_) | ExprKind::AssocArray(_)) {
        return;
    }
    if state.collections.contains(&a.name) {
        return;
    }
    state.diagnostics.push(format!(
        "{}:{}: error [SB5055] `{}` is given a collection value without a \
         collection declaration — declare it `array {}=(…)` (or `map {}=(…)` \
         for string keys); the keyword is what makes it a collection",
        stmt.span.line, stmt.span.col, a.name, a.name, a.name
    ));
}

/// DD#58: emit SB4019 if a variable name collides with a known function.
fn check_var_func_collision(name: &str, stmt: &Statement, state: &mut VersionState) {
    if state.known_functions.contains(name) {
        state.diagnostics.push(format!(
            "{}:{}: error [SB4019] variable '{}' collides with function '{}' — rename one",
            stmt.span.line, stmt.span.col, name, name
        ));
    }
}

/// `arr[i] = val` / `map[key] = val` — subscript assignment. The
/// index is integer-keyed unless the array was declared associative.
fn handle_subscript_assign(
    a: &Assignment,
    idx: &Expr,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    mut block: BlockId,
) -> BlockId {
    let rhs = expr_to_value(&a.value, state);
    // DD#100: `A[i]=v` writes through a collection-parameter slot, so resolve
    // to the caller's collection first — that binding is what makes the write
    // land on the caller's array (Part E). Unresolved, the name `A` reached
    // codegen as an undeclared array and ICE'd.
    let name = super::expressions::resolve_args_name(&a.name, state);
    let is_int_key = !state.assoc_arrays.contains(&name);
    let is_int_val = state.type_map.get(&name).copied() == Some(crate::ir::VarType::Int);
    let val = build_subscript_rhs(&name, a, idx, rhs, is_int_val, state);
    let idx_val = expr_to_value(idx, state);
    // A `$(f)` capture in either operand has to be materialized into a temp
    // first, in source order — an unlifted call reaching codegen is an ICE.
    let idx_val = lift_int_func_calls(idx_val, ssa, state, &mut block);
    let val = lift_int_func_calls(val, ssa, state, &mut block);
    ssa.block_mut(block).insts.push(SsaInst::SubscriptAssign {
        array: name,
        index: idx_val,
        value: val,
        is_integer_key: is_int_key,
    });
    block
}

/// Build the stored value for a subscript assignment, honoring DD#76 int cells
/// and the `+=` read-modify-write. Plain `a[i] = v` discards the current
/// element (correct); `a[i] += v` reads it back: arithmetic add for an int
/// cell, string concat otherwise.
fn build_subscript_rhs(
    name: &str,
    a: &Assignment,
    idx: &Expr,
    rhs: SsaValue,
    is_int_val: bool,
    state: &VersionState,
) -> SsaValue {
    let append = a.op == crate::ir::AssignOp::Append;
    match (append, is_int_val) {
        (true, true) => {
            let current = subscript_to_value(name, idx, state);
            SsaValue::IntCell(Box::new(SsaValue::ArithBinOp {
                left: Box::new(current),
                op: ArithOp::Add,
                right: Box::new(rhs),
            }))
        }
        (true, false) => {
            let current = subscript_to_value(name, idx, state);
            SsaValue::Concat(vec![current, rhs])
        }
        (false, true) => SsaValue::IntCell(Box::new(rhs)),
        (false, false) => rhs,
    }
}

/// `var += val` — append form. An `( … )` literal appends element by element;
/// anything else is a scalar append (string concat, or arithmetic add for an
/// int-typed var).
///
/// There is deliberately no map case. SB5091 rejects `+=` on a map outright and
/// SB5090 rejects every non-`( … )` value on an array, so by the time lowering
/// runs the only collection append left is the one that works. This used to
/// branch on `state.assoc_arrays` and emit an `ArrayAppend` carrying a whole
/// `AssocArray`: `emit_array_append_c` appends at the integer index
/// `sash_table_len(tbl)`, which is meaningless for a map, and the SSA taint
/// pass classes `AssocArray` as assign-only, so the same shape also ICEd.
fn handle_append_assign(
    a: &Assignment,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    mut block: BlockId,
) -> BlockId {
    let val = expr_to_value(&a.value, state);
    // Option C: `var += $(f)` on an `-> int` function is an IntFuncCall;
    // materialize it (like `$((f))`) before the append. No-op otherwise.
    let val = lift_int_func_calls(val, ssa, state, &mut block);
    match val {
        SsaValue::IndexedArray(elems) => {
            for elem in elems {
                ssa.block_mut(block).insts.push(SsaInst::ArrayAppend {
                    array: a.name.clone(),
                    value: elem,
                });
            }
        }
        other => append_scalar(a, other, state, ssa, block),
    }
    block
}

/// Scalar `var += val` — arithmetic add for int-typed vars,
/// otherwise string concat (`var = var ++ val`).
fn append_scalar(
    a: &Assignment,
    other: SsaValue,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) {
    let old = SsaValue::Var(state.lookup(&a.name));
    let is_int = state.type_map.get(&a.name).copied() == Some(crate::ir::VarType::Int);
    let new_val = if is_int {
        SsaValue::ArithBinOp {
            left: Box::new(old),
            op: ArithOp::Add,
            right: Box::new(other),
        }
    } else {
        SsaValue::Concat(vec![old, other])
    };
    let target = state.define(&a.name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target,
        value: new_val,
    });
}

/// `var = val` — plain assignment. If the value is a `FuncCall`
/// substitution that needs capture, inlines the body
/// (`capture_inline`); otherwise lifts any int-typed function
/// calls and emits a single `Assign`.
fn handle_plain_assign(
    a: &Assignment,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    mut block: BlockId,
) -> BlockId {
    let val = expr_to_value(&a.value, state);
    if let Some(b) = maybe_capture_condition(a, &val, state, ssa, block) {
        return b;
    }
    if let SsaValue::FuncCall { name: fname, .. } = &val {
        // A string function's result is its stdout; inline-capture it.
        // (An `-> int` function is an IntFuncCall here, not a FuncCall — it
        // falls through to lift_int_func_calls, capturing the return value.)
        return capture_inline(a, fname, state, ssa, block);
    }
    // DD#58 / Option C: lift IntFuncCall (from `$(f)` / `$((f))`) into a temp.
    let val = lift_int_func_calls(val, ssa, state, &mut block);
    let target = state.define_replacement(&a.name);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::Assign { target, value: val });
    block
}

/// DD#88 capture-condition dispatch for `handle_plain_assign`. In a boolean
/// context (`if x=$(cmd)` / `while x=$(cmd)`), a single external `$(cmd)` (or
/// external pipeline) RHS is the CONDITION — capture its stdout into `x`
/// (tainted) but branch on the command's exit status (no abort). Returns
/// `Some(block)` when it consumed the assignment. Guarded on a
/// command-substitution source expr so an arithmetic mutation `x=1` inside
/// `(( … ))` — also lowered via `handle_plain_assign` with `in_condition` set —
/// is untouched. A `$(user_fn)` RHS (which `validate` couldn't exclude
/// pre-resolution) is rejected with SB4025; returns `None` so the caller still
/// produces a well-formed `Assign` (compilation fails on the diagnostic).
fn maybe_capture_condition(
    a: &Assignment,
    val: &SsaValue,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> Option<BlockId> {
    if state.in_condition == 0 || !matches!(&a.value.kind, ExprKind::CommandSubstitution(_)) {
        return None;
    }
    if let Some(b) = try_capture_condition(&a.name, val, state, ssa, block) {
        return Some(b);
    }
    reject_nonexternal_capture_condition(a, state);
    None
}

/// Capture-condition lowering (`if x=$(cmd)` / `while x=$(cmd)`): bind the
/// command's captured stdout into `name` (tainted, via the `CmdSubst` value)
/// while making the command's exit status the branch truth value. The capture
/// is SUPPRESSED — a nonzero exit does not abort (the status is consumed by the
/// condition, per DD#55's "binding the status" rule) — and `SetTestFromStatus`
/// syncs `test_result` from `$?`. Returns `None` for a non-`CmdSubst` RHS (a
/// literal/concat/user-fn value binding), which stays a validate-level SB4025.
fn try_capture_condition(
    name: &str,
    val: &SsaValue,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> Option<BlockId> {
    match val {
        // Single external `$(cmd)` → a suppressed capture value, then sync
        // test_result from $?.
        SsaValue::CmdSubst { argv, .. } => {
            let mut after = block;
            let value = super::functions::lift_int_func_calls(
                SsaValue::CmdSubst {
                    argv: argv.clone(),
                    suppressed: true,
                },
                ssa,
                state,
                &mut after,
            );
            let target = state.define_replacement(name);
            ssa.block_mut(after)
                .insts
                .push(SsaInst::Assign { target, value });
            push_set_test_from_status(ssa, after, state.suppressed == 0);
            Some(after)
        }
        // A body capture — a pipeline `$(a | b)`, or any command carrying a
        // redirect, which `single_command_subst` deliberately declines so the
        // redirect is lowered by the ordinary statement machinery. Lower it
        // with `state.suppressed` raised, so the commands it runs emit
        // unchecked: the branch consumes their status, so it must not abort.
        // Captures in their operands are lifted checked (DD#55 2026-10-30).
        SsaValue::CmdSubstBody { body, .. } if !capture_condition_head_is_user_fn(body, state) => {
            capture_condition_body(name, body, state, ssa, block)
        }
        _ => None,
    }
}

fn capture_condition_body(
    name: &str,
    body: &[Statement],
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> Option<BlockId> {
    state.suppressed += 1;
    let after = match super::functions::suppressed_capture_value(body, state) {
        Some(value) => try_capture_condition(name, &value, state, ssa, block),
        None => {
            let after = capture_body_into(name, body, state, ssa, block);
            push_set_test_from_status(ssa, after, state.suppressed == 0);
            Some(after)
        }
    };
    state.suppressed -= 1;
    after
}

/// `test_result = ($? == 0)` — the capture-condition status sync sigil.
fn push_set_test_from_status(ssa: &mut SsaProgram, block: BlockId, checked: bool) {
    ssa.block_mut(block).insts.push(SsaInst::internal(
        BuiltinCmd::SetTestFromStatus,
        vec![],
        checked,
    ));
}

/// DD#88: an assignment-in-condition that `validate` let through as an
/// external-looking `$(…)` but did NOT lower to an external capture — i.e. a
/// `$(user_fn)` capture. A function conveys status via `return`, not a captured
/// value, so this is not a capture-condition. Emit SB4025 (compilation fails via
/// `enforce_transform_diags`).
fn reject_nonexternal_capture_condition(a: &Assignment, state: &mut VersionState) {
    let sp = a.value.span;
    // This rejects the capture outright (`$(- f)` included), so the capture
    // check's advice on the same capture would only lead to another SB4025.
    state.expr_diagnostics.get_mut().retain(|(at, _)| *at != sp);
    state.diagnostics.push(format!(
        "{}:{}: error [SB4025] capturing a function's output is not a condition — a function \
         conveys status via `return` (use it as a predicate: `if f {{ … }}`), or capture then \
         test (`{}=$(f); if [[ … ]]`)",
        sp.line, sp.col, a.name
    ));
}

/// The one question `validate` could not answer about a capture-condition body.
///
/// `validate::is_capture_condition_rhs` has already fixed the *shape* — exactly
/// one statement, peeling to a multi-stage pipeline of externals or a command
/// with an external-looking head — and `run_semantic_checks` exits on its
/// diagnostics before `ir_to_ssa` runs, so nothing else reaches here. Re-testing
/// the shape would be the same question asked of the same AST one pass later:
/// a second copy of a classifier rather than a backstop, and the copies in this
/// file have drifted away from their originals rather than catching them (the
/// `Redirected`-peeling disagreements, journal 495). Specs 88.29-88.31 pin
/// those three rejections on the gate that owns them.
///
/// What `validate` genuinely cannot decide is whether the head names a **user
/// function**: it runs before resolution, so `$(f 2>&1)` arrives looking
/// exactly like an external. A bare `$(f)` never reaches this arm at all — it
/// lowers to `FuncCall` — so without this check the redirected spelling would
/// drift off DD#88's side of the line from its unredirected twin (88.28).
///
/// A pipeline is *not* asked: a user function is legal as a stage, because what
/// the condition tests is the pipeline's status, not the function's output
/// (`if x=$(/bin/echo hi | myfn)` compiles and is correct).
fn capture_condition_head_is_user_fn(body: &[Statement], state: &VersionState) -> bool {
    body.iter()
        .any(|stmt| match peel_capture_wrappers(&stmt.kind) {
            StmtKind::Command(c) => {
                matches!(&c.name.kind, ExprKind::Literal(n)
                if state.function_bodies.contains_key(n.as_str()))
            }
            _ => false,
        })
}

/// Peel `Redirected` / `Suppressed` wrappers off a capture body statement.
/// The transform's counterpart to `validate::peel_capture_wrappers`, which
/// also reports the redirects it stripped; here only the innermost kind
/// matters, because whether those redirects are legal was settled by SB4075.
fn peel_capture_wrappers(kind: &StmtKind) -> &StmtKind {
    let mut kind = kind;
    loop {
        match kind {
            StmtKind::Redirected(r) => kind = &r.body.kind,
            StmtKind::Suppressed(inner) => kind = &inner.kind,
            _ => return kind,
        }
    }
}

/// Inline a function-substitution `$(fname args...)` so its output
/// can be captured into `a.name`. Emits `CaptureStart`, sets up
/// positional params (DD#46 clears unbound slots), inlines the body
/// with `return_target` pointing at the restore block, emits
/// `CaptureEnd`, and restores the saved positional / local params.
fn capture_inline(
    a: &Assignment,
    fname: &str,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> BlockId {
    let func_args = extract_func_args(&a.value);
    // Same COMMAND-ARG lowering as `inline_function_call`: `@a` splats per
    // element (DD#20/#22) and `=name` binds a slot (DD#100). This path kept
    // `expr_to_value` and no split, so `s=$(f @a)` joined the array into one
    // argument and `s=$(f =q)` left the slot unbound.
    let (scalar_args, coll_args) = super::functions::split_call_args(&func_args);
    let arg_values: Vec<SsaValue> = scalar_args.iter().map(|e| arg_to_value(e, state)).collect();
    capture_func_into(&a.name, fname, &arg_values, &coll_args, state, ssa, block)
}

/// Inline the body of function `fname` (with `func_args` bound to its
/// positionals) between `CaptureStart` / `CaptureEnd`, storing the captured
/// stdout into the variable `target_name`. Returns the restore block.
/// A lowered function value requires a registered body. Shared by the
/// assignment path
/// (`s=$(f)`) and the argument-position capture lift
/// (`echo $(f)` — string-func-capture-in-arg-ice).
pub(super) fn capture_func_into(
    target_name: &str,
    fname: &str,
    arg_values: &[SsaValue],
    coll_args: &[String],
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> BlockId {
    let body = state
        .function_bodies
        .get(fname)
        .cloned()
        .unwrap_or_else(|| {
            unreachable!("string capture requires a registered function body: {fname}")
        });
    if super::functions::reject_recursive_call(fname, state) {
        return block;
    }
    // DD#81 Part B: arity-check the `$( )` capture path too (it now carries the
    // call's args), matching the plain-call and pipeline-stage paths.
    super::cmd_handlers::check_callee_arity(fname, &body, arg_values, state);
    let mut block = block;
    let arg_values =
        super::functions::lift_int_func_args(arg_values.to_vec(), ssa, state, &mut block);

    // One site number per capture, reserved before the body is lowered so a
    // nested capture inside the body gets a different one.
    let site = state.alloc_capture_site();
    let buffered = crate::capture_purity::capture_body_is_pure(
        &body,
        &state.function_bodies,
        &state.function_int_typed,
    );
    ssa.block_mut(block)
        .insts
        .push(SsaInst::CaptureStart { site, buffered });
    // DD#81: capture uses the same args-array model as inline_function_call
    // (save_caller_args + setup_callee_args), not the old $1..$9 positional
    // model — so the callee body's `${args[i]}` / `$N` resolve to the passed
    // args (previously the args were dropped and `${args[…]}` hit an
    // undefined array).
    let saved_args = super::functions::save_caller_args(state);
    let saved_slots = super::functions::setup_callee_slots(coll_args, state);
    let saved_ro = super::functions::enter_fn_body_scope(state);

    super::functions::setup_callee_args(&arg_values, ssa, state, block);

    let call_block = ssa.new_checkpoint_block(&format!("cap_{}", fname));
    ssa.block_mut(block).terminator = Terminator::Goto(call_block);
    let restore_block = ssa.new_checkpoint_block(&format!("capret_{}", fname));
    // The captured body's own self tail-calls loop to call_block, inside the
    // capture; `$(f)` inside `f` is recursion, like any other call.
    state.inlining.push(fname.to_string());
    super::functions::lower_inlined_body(fname, &body, call_block, restore_block, ssa, state);
    state.inlining.pop();

    let target = state.define_replacement(target_name);
    ssa.block_mut(restore_block)
        .insts
        .push(SsaInst::CaptureEnd {
            site,
            buffered,
            target,
        });

    super::functions::exit_fn_body_scope(state, saved_ro);
    super::functions::restore_caller_slots(saved_slots, state);
    super::functions::restore_caller_args(saved_args, state);
    restore_block
}

/// Capture a **multi-statement / pipeline** `$(…)` body's stdout into
/// `target_name`. Mirrors `capture_func_into` but for a raw statement list
/// (no positional/func-arg setup — a cmdsub inherits the caller's positionals).
/// The buffer/memfd choice also determines the result's taint (DD#127).
/// Returns the block to continue lowering from.
///
/// The body is an **anonymous function** (DG, 2026-09-28): sash has no
/// subshells, so a write to a global inside the body persists, and only names
/// first introduced *in* the body are local to it. Both properties come free
/// from lowering the statements inline here — the scope rules are DD#86's, and
/// DD#86 gives control flow and grouping no scope of their own.
///
/// This used to snapshot every variable the body assigned and restore it
/// afterwards, described as "subshell isolation". Three things were wrong with
/// that. It contradicted DD#86, which makes a write to a global frictionless
/// from anywhere. It contradicted *itself*: `$(f)` where `f` does `x=two`
/// already let the write survive, because the function path never snapshotted
/// — so the same effect persisted or vanished depending on whether it was
/// spelled inline or behind a call. And the snapshot was not total, so a
/// subscript assignment (`a[0]=v`) or a builtin write escaped it anyway; what
/// survived a capture was decided by which statement form you used.
pub(super) fn capture_body_into(
    target_name: &str,
    body: &[Statement],
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> BlockId {
    let site = state.alloc_capture_site();
    let buffered = crate::capture_purity::capture_body_is_pure(
        body,
        &state.function_bodies,
        &state.function_int_typed,
    );
    ssa.block_mut(block)
        .insts
        .push(SsaInst::CaptureStart { site, buffered });

    let call_block = ssa.new_block("cap_body");
    ssa.block_mut(block).terminator = Terminator::Goto(call_block);
    let restore_block = ssa.new_block("capbody_ret");

    let saved_return_target = state.return_target;
    state.return_target = Some(restore_block);
    let pipe_reader = state.pipe_reader.take();
    // An anonymous function has no name to tail-call, so no tail loop. One
    // inherited from the enclosing function turned `return g …` inside a
    // `$( … )` inside `g` into a back edge out of the open capture; without
    // it the call reaches the recursion check like any other.
    let saved_tail = state.tail_loop.take();
    // Nor does a condition reach inside: a capture in a condition's argument
    // is not itself in condition position (validate resets it for the same
    // reason), so its failing pipeline must abort as it does elsewhere. A
    // capture *condition* consumes the status through `state.suppressed`,
    // which stays in force for the commands the body runs.
    let saved_condition = std::mem::take(&mut state.in_condition);
    let failed_attests = std::mem::take(&mut state.failed_attests);
    let exit_block = super::functions::lower_body(body, ssa, state, call_block);
    state.failed_attests = failed_attests;
    state.in_condition = saved_condition;
    state.tail_loop = saved_tail;
    state.pipe_reader = pipe_reader;
    state.return_target = saved_return_target;
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

    let target = state.define_replacement(target_name);
    ssa.block_mut(restore_block)
        .insts
        .push(SsaInst::CaptureEnd {
            site,
            buffered,
            target,
        });

    restore_block
}

/// Re-extract the call args from the original `$(…)` expression. The
/// args were dropped when `expr_to_value` collapsed the substitution
/// into a `FuncCall(name)`, so we pull them back from the `Expr` here.
///
/// The shape test is `single_command_subst`'s — the one function that
/// decided this was a `FuncCall` at all (`handle_command_substitution` →
/// `resolve_single_cmdsubst` → `func_call_capture`). It used to be
/// duplicated here, and the copy had already drifted: it unwrapped
/// `StmtKind::Redirected`, which the canonical test deliberately *declines*
/// so a redirected capture falls to `CmdSubstBody` and keeps its redirect.
fn extract_func_args(value: &Expr) -> Vec<Expr> {
    let Some((_, _, c)) = super::expr_handlers::single_command_subst(value) else {
        unreachable!(
            "capture_inline runs only when expr_to_value produced a FuncCall, \
             and single_command_subst is the only route to one"
        )
    };
    c.args.clone()
}

/// `attest`, `argv` and `keys` each reach lowering with their shape already
/// fixed — `attest` by `parse_attest` (SB4014), `argv` by SB4045/SB5085,
/// `keys` by SB5073/SB4045 — so each lowering says so loudly rather than
/// returning `None`. `None` is the dangerous answer here: it means "not a
/// builtin", which compiles the statement into an ordinary command and dies
/// at runtime with `sash: bug: bare command passed to sash_exec`. DG's rule
/// for the coverage campaign: an unreachable assertion earns a unit test
/// proving it catches what it claims.
#[cfg(test)]
mod invariant_tests {
    use super::*;
    use std::collections::HashMap;

    fn lit(s: &str) -> Expr {
        Expr {
            kind: ExprKind::Literal(s.to_string()),
            span: Span::new(1, 1),
        }
    }

    fn fixtures() -> (SsaProgram, VersionState, BlockId) {
        let mut ssa = SsaProgram::new();
        let block = ssa.new_block("entry");
        ssa.entry = block;
        (ssa, VersionState::with_types(HashMap::new()), block)
    }

    fn ice_on(name: &str, args: Vec<Expr>, want: &str) {
        let c = Command {
            name: lit(name),
            args,
        };
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (mut ssa, mut state, block) = fixtures();
            match name {
                "[[" => {
                    state.in_condition = 1;
                    let stmt = Statement::command(c.name.clone(), c.args.clone(), Span::new(1, 1));
                    handle_command(&c, &stmt, &mut ssa, &mut state, block)
                }
                "attest" => handle_attest_stmt(&c, &mut ssa, &mut state, block),
                "argv" => handle_argv(&c, &mut ssa, &mut state, block),
                "keys" => handle_keys(&c, &mut ssa, &mut state, block),
                other => unreachable!("no such lowering under test: {other}"),
            }
        }));
        let err = caught.expect_err("this shape must ICE");
        let msg = err
            .downcast_ref::<&'static str>()
            .map(|s| (*s).to_string())
            .or_else(|| err.downcast_ref::<String>().cloned())
            .expect("ICE payload is a string");
        assert!(
            msg.contains(want),
            "the ICE must name what guarantees it: wanted {want:?}, got {msg:?}"
        );
    }

    /// A pipeline stage is always a `Command`: SB4022 rejects a compound one
    /// at *parse*, a pass earlier than the usual `validate` shadow. Measured
    /// over eight spellings — `{ }` first and last, `while`/`if`/`for` heads,
    /// `(( … ))`, a redirected group (SB4021) and `case` (SB4029).
    ///
    /// The arm this replaced was a complete, never-executed lowering. The ICE
    /// is the claim that no such stage exists; if SB4022 is ever relaxed, this
    /// is what stops the untested route from opening in silence.
    #[test]
    #[should_panic(expected = "SB4022 rejects one at parse")]
    fn a_compound_pipeline_stage_is_a_loud_ice() {
        let (mut ssa, mut state, block) = fixtures();
        let mut stages = vec![];
        let stmt = Statement {
            kind: StmtKind::Pipeline(Pipeline {
                commands: vec![],
                negated: false,
            }),
            span: Span::new(1, 1),
        };
        lower_one_pipeline_stage(&stmt, 0, &mut ssa, &mut state, block, &mut stages);
    }

    /// `a | ! while …` is SB4022 at parse (`reject_nonconsuming_final_stage`),
    /// so no program reaches this shape. Were that relaxed,
    /// `consumes_as_statement`'s `!self.negated` term keeps a negated loop off
    /// the reader-loop path, which would lower it with an inverted status that
    /// never joins the pipeline's, so the `!` would compile to nothing. The
    /// loop goes down the regular path instead, and the compound-stage ICE
    /// fires.
    #[test]
    #[should_panic(expected = "SB4022 rejects one at parse")]
    fn a_negated_final_loop_stage_is_not_a_reader_loop() {
        let (mut ssa, mut state, block) = fixtures();
        let span = Span::new(1, 1);
        let reader_loop = Statement {
            kind: StmtKind::While(While {
                condition: vec![],
                body: vec![],
                until: false,
            }),
            span,
        };
        let p = Pipeline {
            commands: vec![
                Statement::command(lit("/bin/echo"), vec![lit("x")], span),
                Statement {
                    kind: StmtKind::Negated(Box::new(reader_loop)),
                    span,
                },
            ],
            negated: false,
        };
        dispatch_multistage_pipeline(&p, &mut ssa, &mut state, block);
    }

    #[test]
    fn an_unclassifiable_attest_is_a_loud_ice() {
        ice_on("attest", vec![], "SB4014");
    }

    #[test]
    fn a_malformed_test_cannot_fall_through_or_drop_arguments() {
        let test = Expr {
            kind: ExprKind::Test(TestExpr::Unary {
                op: "-n".to_string(),
                operand: Box::new(lit("word")),
            }),
            span: Span::new(1, 1),
        };
        for args in [
            vec![],
            vec![lit("word")],
            vec![lit("word"), test.clone()],
            vec![test.clone(), lit("word")],
            vec![test.clone(), test],
        ] {
            ice_on(
                "[[",
                args,
                "a selected [[ command requires exactly one Test operand",
            );
        }
    }

    #[test]
    fn a_zero_argument_argv_is_a_loud_ice() {
        ice_on("argv", vec![], "SB4045");
    }

    #[test]
    fn a_non_integer_argv_index_is_a_loud_ice() {
        ice_on("argv", vec![lit("x"), lit("a")], "SB5085");
    }

    #[test]
    fn a_three_argument_argv_is_a_loud_ice() {
        ice_on("argv", vec![lit("1"), lit("2"), lit("a")], "SB4045");
    }

    #[test]
    fn a_keys_statement_of_the_wrong_arity_is_a_loud_ice() {
        ice_on("keys", vec![lit("m")], "SB5073");
        ice_on("keys", vec![lit("m"), lit("k"), lit("x")], "SB5073");
    }

    /// `extract_func_args` is reached only from `capture_inline`, which
    /// runs only when `expr_to_value` produced an `SsaValue::FuncCall` — and
    /// `single_command_subst` is the only route to one. These are the four
    /// shapes it declines, so none can arrive here. The checks used to be
    /// duplicated inside `extract_func_args`, and the copy had already drifted
    /// from the original over `Redirected`.
    #[test]
    fn extract_func_args_rejects_every_shape_single_command_subst_declines() {
        let cmd = Statement {
            kind: StmtKind::Command(Command {
                name: lit("f"),
                args: vec![],
            }),
            span: Span::new(1, 1),
        };
        let subst = |stmts: Vec<Statement>| Expr {
            kind: ExprKind::CommandSubstitution(stmts),
            span: Span::new(1, 1),
        };
        let redirected = Statement {
            kind: StmtKind::Redirected(crate::ir::Redirected {
                body: Box::new(cmd.clone()),
                redirects: vec![],
            }),
            span: Span::new(1, 1),
        };
        let shapes = [
            // not a command substitution at all
            lit("plain"),
            // zero and two statements
            subst(vec![]),
            subst(vec![cmd.clone(), cmd.clone()]),
            // a redirected capture — declined so the redirect survives
            subst(vec![redirected]),
        ];
        for shape in shapes {
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                extract_func_args(&shape)
            }));
            let err = caught.expect_err("this shape must ICE");
            let msg = err
                .downcast_ref::<&'static str>()
                .map(|s| (*s).to_string())
                .or_else(|| err.downcast_ref::<String>().cloned())
                .expect("ICE payload is a string");
            assert!(
                msg.contains("single_command_subst is the only route"),
                "ICE must name the invariant that guarantees it, got: {msg}"
            );
        }
    }
}
