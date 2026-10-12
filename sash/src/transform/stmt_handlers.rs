//! Extracted handlers for the largest StmtKind match arms.

use super::cmd_handlers::check_callee_arity;
use super::expressions::{
    arg_to_value, expr_to_value, literal_name_of, resolve_args_name, subscript_to_value,
    test_to_ssa,
};
use super::functions::{lift_int_func_args, lift_int_func_calls};
use super::pipelines::lower_pipe_while_read;
use super::resolution::resolve_pipeline_command;
use super::statements::{lower_one, lower_stmts, spill_scalar_redirect_targets};
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

/// Dispatch a `BuiltinCmd` to its handler. Most builtins always
/// consume the statement (return `Some`); `Test` and `Argv` may
/// return `None` to fall through to the regex / general handler.
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
        BuiltinCmd::Test => try_test_arg_capture(c, ssa, state, *block),
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
    if c.args.len() == 3 {
        autodeclare_bulk_target(&array, false, ssa, state, block);
    }
    ssa.block_mut(block).insts.push(SsaInst::MatchInto {
        subject,
        pattern,
        array,
        all: kind == BuiltinCmd::MatchAll,
    });
    Some(block)
}

/// `[[ … ]]` attached as an argument (the parser may emit `test` /
/// `[` commands whose first/only arg carries the `Test` expression
/// kind). The first matching arg wins. Returns `None` when no
/// `ExprKind::Test` is found so the caller falls through.
fn try_test_arg_capture(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    for arg in &c.args {
        if let ExprKind::Test(test_expr) = &arg.kind {
            let mut block = block;
            let ssa_test = test_to_ssa(test_expr, state);
            // A `$(f)` operand must be captured into a temp before the test —
            // the capture may open a new block, so continue in whichever the
            // lift leaves us in.
            let ssa_test = super::functions::lift_test_captures(ssa_test, ssa, state, &mut block);
            ssa.block_mut(block)
                .insts
                .push(SsaInst::Test(ssa_test, TestOrigin::Plain));
            return Some(block);
        }
    }
    None
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

/// DD#55/SB4016: a bare call of a non-void (`-> int`) function in
/// statement position must be handled by `if`/`||` or captured by
/// `$((…))`. Emits a diagnostic when violated; does not affect
/// codegen (the caller still threads the call through).
fn check_non_void_bare_call(name: &str, stmt: &Statement, state: &mut VersionState) {
    // DD#70 addendum: `- f` discards the returned int, like any other result.
    if state.suppressed > 0 {
        return;
    }
    if state.function_non_void.contains(name) && state.in_condition == 0 {
        // DD#74: an `-> int` function is rejected in *every* boolean context
        // (SB4030), so `if`/`||` was not advice the author could act on — it
        // sent them from SB4016 straight into SB4030. This is unconditional,
        // not a special case: `function_non_void` is populated only by
        // `analyze_function_returns`, which returns early for anything that
        // is not `-> int`, so every name reaching here is int-typed.
        state.diagnostics.push(format!(
            "{}:{}: error [SB4016] function '{}' returns an int — capture with $(({})) or discard with `-`",
            stmt.span.line, stmt.span.col, name, name
        ));
    }
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
    // before lowering — a malformed one is SB4014 at parse time, and quoting
    // the name (`"attest" x`) makes it an ordinary command instead. There is
    // no path by which an unclassifiable `attest` Command reaches here.
    let Some((form, pred)) = classify_attest(&c.args) else {
        unreachable!("`attest` shape is fixed by parse_attest before lowering (SB4014)")
    };
    {
        // DD#93 Part D: `attest SOURCE (( TARGET = SOURCE ))`.
        if let (AttestForm::Scalar { var }, AttestPredicate::ArithConvert { target, value }) =
            (&form, &pred)
        {
            let (var, target) = (var.to_string(), target.to_string());
            let value = (*value).clone();
            let span = c.args.last().map(|a| a.span).unwrap_or_default();
            push_attest_arith_convert(ssa, state, block, &var, &target, &value, span);
            return block;
        }
        // Total over `AttestPredicate` (DD#116 property 2): a new predicate
        // variant is a compile error here rather than a silently dropped
        // `attest`. `ArithConvert` is unreachable in this position because
        // `classify_attest_convert` only ever pairs it with a single-name
        // lead, which the arm above has already consumed.
        let test = match pred {
            AttestPredicate::Test(test) => test,
            AttestPredicate::ArithConvert { .. } => {
                unreachable!("ArithConvert is only produced with AttestForm::Scalar")
            }
        };
        let test = test.clone();
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
    // DD#100: `attest v in A` attests the *caller's* collection — the deferral
    // Part E describes. Unresolved, this ICEd on an undefined array `A`.
    let array = &resolve_args_name(array, state);
    let source = state.lookup(array);
    // Define the binders so they receive globally-unique SSA versions,
    // resolve the predicate against them, then restore current versions so
    // the binders stay loop-local (DD#61 shadow semantics). The version
    // counter is not rewound by restore, so the binder versions remain
    // unique and match the slots emit_attest_all creates for them.
    let snap = state.snapshot();
    let binder_refs: Vec<VarRef> = binders.iter().map(|b| state.define(b)).collect();
    let ssa_test = test_to_ssa(&test, state);
    state.restore(&snap);
    ssa.block_mut(block).insts.push(SsaInst::AttestAll {
        source: source.clone(),
        binders: binder_refs,
        test: ssa_test,
        is_map,
        if_form: false,
    });
    let target = state.define(array);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::AttestRetag { target, source });
}

fn handle_break_stmt(
    c: &Command,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> BlockId {
    let n = c
        .args
        .first()
        .map(|a| literal_name_of(a).parse::<usize>().unwrap_or(1))
        .unwrap_or(1);
    let depth = state.loop_stack.len();
    if n > 0 && depth > 0 {
        let level = n.min(depth);
        let (_header, exit) = state.loop_stack[depth - level];
        ssa.block_mut(block).terminator = Terminator::Goto(exit);
        return ssa.new_block("after_break");
    }
    block
}

fn handle_continue_stmt(
    c: &Command,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> BlockId {
    let n = c
        .args
        .first()
        .map(|a| literal_name_of(a).parse::<usize>().unwrap_or(1))
        .unwrap_or(1);
    let depth = state.loop_stack.len();
    if n > 0 && depth > 0 {
        let level = n.min(depth);
        let (header, _exit) = state.loop_stack[depth - level];
        ssa.block_mut(block).terminator = Terminator::Goto(header);
        return ssa.new_block("after_continue");
    }
    block
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
    let es_var = state.define("?");
    ssa.block_mut(*block).insts.push(SsaInst::Assign {
        target: es_var,
        value: exit_val,
    });
    if let Some(target) = state.return_target {
        ssa.block_mut(*block).terminator = Terminator::Goto(target);
    } else {
        ssa.block_mut(*block).terminator = Terminator::Return;
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
    let (tf, header, aname) = state.tail_loop.clone()?;
    if *name != tf {
        return None;
    }
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
    autodeclare_bulk_target(&array, false, ssa, state, block);
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
    autodeclare_bulk_target(&array, false, ssa, state, block);
    // A LATER version than the autodeclare's empty-array Assign, so the SSA
    // taint backstop resolves the array to this fill rather than to the clean
    // empty array the declaration produced. (Both are clean here — see
    // `SsaInst::KeysFill` — but the versioning rule is the same for every
    // bulk fill and diverging from it would be a trap for the next one.)
    let target = state.define(&array);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::KeysFill { source, target });
    block
}

/// Auto-declare a bulk-read target array (mirrors argv, DG 2026-07-08): if
/// `name` isn't already a collection, emit the empty-array/-map Assign that
/// creates the versioned `name_0` slot — so the fill (`argv`/`read`) and any
/// later `len`/iteration/element read resolve without a prior `array
/// name=()`. Without this the codegen `find_array_slot` panics (ICE). `as_map`
/// creates an associative array.
pub(super) fn autodeclare_bulk_target(
    name: &str,
    as_map: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    if state.collections.contains(name) {
        return;
    }
    state.collections.insert(name.to_string());
    let value = if as_map {
        state.assoc_arrays.insert(name.to_string());
        SsaValue::AssocArray(vec![])
    } else {
        SsaValue::IndexedArray(vec![])
    };
    let target = state.define(name);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::Assign { target, value });
}

/// Handle StmtKind::Pipeline — multi-command pipe chains.
pub(super) fn handle_pipeline(
    p: &Pipeline,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#70: `! X` is a (B) must-consume boolean — bare/consequent → SB4018,
    // `-` → SB4019. And `!` *consumes* its operand, so lower it in a boolean
    // context (an inner `[[ … ]]`/`match` must not itself fire — the `!` is
    // its consumer).
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
        true,
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
/// based on its last stage: `while read`/`mapfile`, `lastpipe read`, or a
/// regular fork/exec chain.
fn dispatch_multistage_pipeline(
    p: &Pipeline,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let last = &p.commands[p.commands.len() - 1];
    // Structural on purpose: DD#122/SB4073 has already rejected a `while` final
    // stage with no reader in it, so reaching here means the loop really does
    // consume the pipe. This predicate used to be the *only* thing deciding it,
    // which is how `cmd | while (( n < 1 )); do /bin/cat; done` compiled and
    // silently discarded the pipeline's output.
    let is_while_read = matches!(&last.kind, StmtKind::While(_));
    let is_array_read = is_array_read_last(last);
    if is_while_read || is_array_read {
        return handle_pipe_while_or_array_read(p, last, is_array_read, ssa, state, block);
    }
    if is_lastpipe_read_last(last) {
        handle_lastpipe_read(p, last, ssa, state, block);
    } else {
        handle_regular_pipeline(p, ssa, state, block);
    }
    block
}

/// DD#70 (B): `! X` produces a must-consume boolean (for any operand). A
/// consuming position (if/while/&&/||) is fine; a bare result is SB4018;
/// `-` discards it (DD#70 addendum — see `check_boolean_must_consume`).
fn check_negation_must_consume(stmt: &Statement, state: &mut VersionState) {
    if state.in_condition > 0 || state.suppressed > 0 {
        return;
    }
    state.diagnostics.push(format!(
        "{}:{}: error [SB4018] `!` result is unused — consume it with if/`&&`/`||`, or discard with `-` or `|| :`",
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
fn is_lastpipe_read_last(last: &Statement) -> bool {
    match &last.kind {
        StmtKind::Command(c) => literal_name_of(&c.name) == "readline",
        _ => false,
    }
}

/// Build pipeline stages from the producer commands (all but the last).
/// Used by pipe-while-read and lastpipe-read shapes; both have identical loops.
/// A known-function stage is inlined as a forked compound child (like the
/// regular pipeline), not exec'd as a bare command.
fn build_producer_stages(
    cmds: &[Statement],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Vec<(String, Vec<SsaValue>, Vec<Redirect>)> {
    let mut stages = vec![];
    for (idx, cmd) in cmds.iter().enumerate() {
        let (inner, redirects) = match &cmd.kind {
            StmtKind::Redirected(r) => (&r.body.kind, r.redirects.clone()),
            other => (other, vec![]),
        };
        if let StmtKind::Command(c) = inner {
            push_command_stage(c, redirects, idx, ssa, state, block, &mut stages);
        }
    }
    stages
}

/// Push a `Command` pipeline stage into `stages`: a known function inlines as a
/// forked compound child (`__compound_<id>`, args bound); anything else resolves
/// to an exec stage (DD#23 compile-time PATH). Shared by the regular pipeline
/// and the producer-stage (pipe-while-read / lastpipe) builders.
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
        if let Some(entry) = build_function_pipeline_stage(name, c, ssa, state, idx) {
            stages.push((format!("__compound_{}", entry), vec![], redirects));
            return;
        }
    }
    let args: Vec<SsaValue> = c.args.iter().map(|a| arg_to_value(a, state)).collect();
    // DD#99: a `write` stage is an `echo -n` stage (see rewrite_write_as_echo).
    let (name, args) = super::cmd_handlers::rewrite_write_as_echo(name, args);
    stages.push((resolve_pipeline_command(&name, state), args, redirects));
}

/// `cmd1 | ... | while readline var` or `cmd1 | ... | read arr`.
/// Emit producer stages as a pipeline that returns a pipe fd,
/// then lower the last stage with fd = -1 (pipe-fd sentinel).
fn handle_pipe_while_or_array_read(
    p: &Pipeline,
    last: &Statement,
    is_array_read: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    let stages = build_producer_stages(&p.commands[..p.commands.len() - 1], ssa, state, block);
    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
        stages,
        capture_read_fd: true,
        checked: true,
        suppressed: vec![],
        negated: vec![],
    });
    if is_array_read {
        let mark = super::pipelines::mark_ssa(ssa);
        block = lower_one(last, ssa, state, block);
        // DD#24: the read is clean only if every producer is compiler-computed.
        // Same predicate `handle_lastpipe_read` uses for `readline`, and the
        // same `validate::is_external_source` the front-end taint pass uses.
        let clean_pipe = !p.commands[..p.commands.len() - 1]
            .iter()
            .any(crate::validate::is_external_source);
        rewrite_lastpipe_consumer_fds(ssa, &mark, clean_pipe);
    } else {
        block = lower_pipe_while_read(last, ssa, state, block);
    }
    block
}

/// After lowering an array-read lastpipe consumer, repoint its read fd
/// (currently 0 = stdin) to -1 (pipe sentinel) so codegen reads from
/// the captured pipe fd instead of the inherited stdin, and record whether
/// that pipe is clean so the SSA taint backstop classifies it correctly.
///
/// DD#122: scoped to the instructions the consumer's own lowering produced.
/// This walked every block in the program, so a `read arr` reading fd 0 with no
/// pipe at all was silently re-aimed at a later pipeline's pipe — the same
/// defect `repoint_readlines_to_pipe` had, in the array half of the read family.
fn rewrite_lastpipe_consumer_fds(
    ssa: &mut SsaProgram,
    mark: &std::collections::HashMap<BlockId, usize>,
    clean: bool,
) {
    super::pipelines::for_each_new_inst(ssa, mark, &mut |inst| {
        repoint_stdin_to_pipe_sentinel(inst, clean);
    });
}

fn repoint_stdin_to_pipe_sentinel(inst: &mut SsaInst, clean: bool) {
    if let SsaInst::ReadArray { fd, clean_pipe, .. } = inst {
        if *fd == 0 {
            *fd = -1;
            *clean_pipe = clean;
        }
    }
}

/// DD#20: lastpipe — `cmd | read var` runs `read` in the main shell.
/// Emit producer stages with captured pipe fd, then emit a ReadLine that
/// consumes from that pipe in the main process.
fn handle_lastpipe_read(
    p: &Pipeline,
    last: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    let producers = &p.commands[..p.commands.len() - 1];
    // DD#24: the read targets are clean only if every producer is
    // compiler-computed. This was hardcoded `true`, which disabled the SSA
    // backstop for every lastpipe read — so when the front-end pass also
    // missed a laundering shape, nothing caught it.
    let clean_pipe = !producers.iter().any(crate::validate::is_external_source);
    let stages = build_producer_stages(producers, ssa, state, block);
    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
        stages,
        capture_read_fd: true,
        checked: true,
        suppressed: vec![],
        negated: vec![],
    });
    // An array read is peeled off earlier by `is_array_read_last`, so only
    // `readline` reaches here — one target, no flags (DD#99/SB5057).
    if let StmtKind::Command(c) = &last.kind {
        let target = state.define(&crate::ir::readline_target(&c.args));
        ssa.block_mut(block).insts.push(SsaInst::ReadLine {
            target,
            fd: -1,
            redirects: vec![],
            clean_pipe,
        });
    }
}

/// Regular pipeline: `cmd1 | cmd2 | ...`. Compound commands (while, for, if)
/// become forked children — lowered into their own SSA blocks and recorded
/// in `compound_pipeline_blocks` for codegen to fork and run.
fn handle_regular_pipeline(
    p: &Pipeline,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    let mut stages = vec![];
    let mut suppressed = vec![];
    let mut negated = vec![];
    for (idx, cmd) in p.commands.iter().enumerate() {
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
    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
        stages,
        capture_read_fd: false,
        // DD#55: a `-`-suppressed pipeline (and a capture-condition pipeline,
        // `if x=$(a|b)`) runs unchecked — its aggregate status is consumed, not
        // aborted on. `state.suppressed` is bumped only by `- cmd` lowering and
        // the capture-condition capture.
        checked: state.suppressed == 0,
        suppressed,
        negated,
    });
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
    let (unwrapped, is_suppressed, is_negated) = match &cmd.kind {
        StmtKind::Suppressed(inner) => (inner.as_ref(), true, false),
        StmtKind::Negated(inner) => (inner.as_ref(), false, true),
        _ => (cmd, false, false),
    };
    let stages_before = stages.len();
    lower_one_pipeline_stage(unwrapped, idx, ssa, state, block, stages);
    suppressed.resize(stages.len(), false);
    negated.resize(stages.len(), false);
    suppressed[stages_before..].fill(is_suppressed);
    negated[stages_before..].fill(is_negated);
}

/// Lower a single (already Suppressed-peeled) pipeline stage into `stages`:
/// a `Command` becomes a direct stage (possibly several, via function
/// inlining); any compound is lowered into its own SSA blocks and recorded
/// as a `__compound_<id>` forked child.
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
/// each **SB4022**; a redirected group is SB4021; `case` is SB4029.
///
/// The arm this replaced was not a wrong answer — it was a complete, plausible
/// lowering (fork a compound child into `compound_pipeline_blocks` as
/// `__compound_<id>`) that had never once run. That is the dangerous shape:
/// relax SB4022 and the route opens *in silence*, with no spec having ever
/// exercised it. The sibling machinery is still live and still tested —
/// `build_function_pipeline_stage` takes the same path for a function-call
/// stage — so nothing is lost by making this entry say what it knows.
///
/// Contrast `build_function_pipeline_stage`'s recursion guard below, which is
/// also unreachable and is deliberately *kept* as a real diagnostic: that one
/// states an invariant the site is uniquely able to state (DD#116). This one
/// states nothing; it just duplicates a lowering for a form the language does
/// not have.
const COMPOUND_STAGE_IS_REJECTED_AT_PARSE: &str =
    "ICE: a compound command reached a pipeline stage — SB4022 rejects one at \
     parse, so a stage is always a Command; the compound-stage lowering was \
     never exercised and must be written knowingly";

/// A `Command` pipeline stage that names a known function: inline the function
/// body into its own SSA blocks (a forked compound child, recorded in
/// `compound_pipeline_blocks` and named `__compound_<id>`), binding the call
/// args as $1..$N. Mirrors the `{ }`-group compound stage, plus positional
/// setup and a return target. Subshell semantics: the stage's variable
/// assignments must not leak to the parent, so the version map is
/// snapshotted/restored around the inline (only the forked child sees them).
/// Returns the entry block id, or `None` if the body isn't registered.
fn build_function_pipeline_stage(
    name: &str,
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    idx: usize,
) -> Option<usize> {
    let resolved = name.to_string();
    let body = state.function_bodies.get(&resolved).cloned()?;

    let saved = state.snapshot();
    let saved_return = state.return_target;
    let saved_args = super::functions::save_caller_args(state);

    // DD#27/§5: reject recursion into a function already being inlined.
    // DD#116: a *specification fidelity* backstop. SB5037 rejects recursion in
    // `validate`, so this cannot fire on a program that gets this far — but
    // inlining is the pass whose termination depends on it, and the rule is
    // "no function is inlined into itself", which is a statement this site is
    // the only one able to make. Deleting it would make the pass's termination
    // an unstated consequence of a check two passes away.
    if state.inlining.iter().any(|n| n == &resolved) {
        state
            .diagnostics
            .push(super::functions::recursion_diagnostic(&resolved));
        super::functions::restore_caller_args(saved_args, state);
        return None;
    }
    state.inlining.push(resolved.clone());
    let saved_ro = super::functions::enter_fn_body_scope(state);

    let entry = ssa.new_block(&format!("pipe_fn_{}_{}", name, idx));
    // `split_call_args` + `arg_to_value`, exactly as `inline_function_call`: a
    // pipeline stage is still a call, so `@a` is a COMMAND-ARG position and
    // splats one value per element (DD#20/#22), and `=name` binds a slot rather
    // than contributing to `args` (DD#100). This path kept `expr_to_value` when
    // the plain-call and array-literal paths were corrected, so `f @a | g`
    // silently passed the whole array as one joined argument — losing an
    // element that contains a space — and `f =q | g` left the slot unbound.
    let (scalar_args, coll_args) = super::functions::split_call_args(&c.args);
    let arg_values: Vec<SsaValue> = scalar_args.iter().map(|a| arg_to_value(a, state)).collect();
    check_callee_arity(name, &body, &arg_values, state);
    let saved_slots = super::functions::setup_callee_slots(&coll_args, state);
    super::functions::setup_callee_args(&arg_values, ssa, state, entry);

    // `return` inside the body jumps to ret_block, which ends the forked child.
    let ret_block = ssa.new_block(&format!("pipe_fn_ret_{}_{}", name, idx));
    state.return_target = Some(ret_block);
    let exit = lower_stmts(&body, ssa, state, entry);
    ssa.block_mut(exit).terminator = Terminator::Goto(ret_block);
    ssa.block_mut(ret_block).terminator = Terminator::Return;

    state.return_target = saved_return;
    super::functions::exit_fn_body_scope(state, saved_ro);
    state.restore(&saved);
    super::functions::restore_caller_slots(saved_slots, state);
    super::functions::restore_caller_args(saved_args, state);
    state.inlining.pop();

    ssa.compound_pipeline_blocks.push(entry);
    Some(entry)
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
/// (`try_capture_inline`); otherwise lifts any int-typed function
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
        if let Some(b) = try_capture_inline(a, fname, state, ssa, block) {
            return b;
        }
    }
    // DD#58 / Option C: lift IntFuncCall (from `$(f)` / `$((f))`) into a temp.
    let val = lift_int_func_calls(val, ssa, state, &mut block);
    let target = state.define(&a.name);
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
            let target = state.define(name);
            ssa.block_mut(block).insts.push(SsaInst::Assign {
                target,
                value: SsaValue::CmdSubst {
                    argv: argv.clone(),
                    suppressed: true,
                },
            });
            push_set_test_from_status(ssa, block);
            Some(block)
        }
        // A body capture — a pipeline `$(a | b)`, or any command carrying a
        // redirect, which `single_command_subst` deliberately declines so the
        // redirect is lowered by the ordinary statement machinery. Lower it
        // with `state.suppressed` bumped (an inner pipeline emits unchecked)
        // and uncheck the rest afterwards, since a plain Command does not
        // consult `state.suppressed` at emission time. Either way the status
        // is consumed by the branch, so it must not abort.
        SsaValue::CmdSubstBody { body, .. } if is_capture_condition_body(body, state) => {
            let pre_len = ssa.block(block).insts.len();
            state.suppressed += 1;
            let after = capture_body_into(name, body, state, ssa, block);
            state.suppressed -= 1;
            super::statements::uncheck_emitted_since(ssa, block, pre_len, after);
            push_set_test_from_status(ssa, after);
            Some(after)
        }
        _ => None,
    }
}

/// `test_result = ($? == 0)` — the capture-condition status sync sigil.
fn push_set_test_from_status(ssa: &mut SsaProgram, block: BlockId) {
    ssa.block_mut(block).insts.push(SsaInst::internal(
        BuiltinCmd::SetTestFromStatus,
        vec![],
        true,
    ));
}

/// DD#88: an assignment-in-condition that `validate` let through as an
/// external-looking `$(…)` but did NOT lower to an external capture — i.e. a
/// `$(user_fn)` capture. A function conveys status via `return`, not a captured
/// value, so this is not a capture-condition. Emit SB4025 (compilation fails via
/// `enforce_transform_diags`).
fn reject_nonexternal_capture_condition(a: &Assignment, state: &mut VersionState) {
    let sp = a.value.span;
    state.diagnostics.push(format!(
        "{}:{}: error [SB4025] capturing a function's output is not a condition — a function \
         conveys status via `return` (use it as a predicate: `if f {{ … }}`), or capture then \
         test (`{}=$(f); if [[ … ]]`)",
        sp.line, sp.col, a.name
    ));
}

/// True when a `$(…)` body is a capture-condition body: exactly one statement
/// that is a pipeline (`$(a | b)`), or a command carrying a redirect
/// (`$(cmd 2>/dev/null)`). A multi-statement `$(a; b)` is not a condition, and
/// `validate` has already rejected everything else that reaches here.
///
/// The command case must still exclude a **user function**. `validate` runs
/// before resolution and cannot consult the function table, so `$(f 2>&1)`
/// arrives looking exactly like an external — and a bare `$(f)` never reaches
/// this arm at all, because it lowers to `FuncCall` rather than a body
/// capture. Checking the table here keeps the redirected spelling on DD#88's
/// side of the line with its unredirected twin.
fn is_capture_condition_body(body: &[Statement], state: &VersionState) -> bool {
    let [stmt] = body else {
        return false;
    };
    match peel_capture_wrappers(&stmt.kind) {
        StmtKind::Pipeline(_) => true,
        StmtKind::Command(c) => match &c.name.kind {
            ExprKind::Literal(n) => !state.function_bodies.contains_key(n.as_str()),
            _ => false,
        },
        _ => false,
    }
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
/// Returns `None` when no body is registered for `fname`.
fn try_capture_inline(
    a: &Assignment,
    fname: &str,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> Option<BlockId> {
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
/// stdout into the variable `target_name`. Returns the restore block, or
/// `None` when no body is registered. Shared by the assignment path
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
) -> Option<BlockId> {
    let body = state.function_bodies.get(fname).cloned()?;
    // DD#81 Part B: arity-check the `$( )` capture path too (it now carries the
    // call's args), matching the plain-call and pipeline-stage paths.
    super::cmd_handlers::check_callee_arity(fname, &body, arg_values, state);

    // One site number per capture, reserved before the body is lowered so a
    // nested capture inside the body gets a different one.
    let site = state.alloc_capture_site();
    let buffered = super::capture_purity::capture_body_is_pure(&body, &state.function_bodies);
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

    super::functions::setup_callee_args(arg_values, ssa, state, block);

    let call_block = ssa.new_block(&format!("cap_{}", fname));
    ssa.block_mut(block).terminator = Terminator::Goto(call_block);
    let restore_block = ssa.new_block(&format!("capret_{}", fname));
    let saved_return_target = state.return_target;
    state.return_target = Some(restore_block);
    let exit_block = lower_stmts(&body, ssa, state, call_block);
    state.return_target = saved_return_target;
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

    let target = state.define(target_name);
    ssa.block_mut(restore_block)
        .insts
        .push(SsaInst::CaptureEnd {
            site,
            buffered,
            target,
            tainted: false,
        });

    super::functions::exit_fn_body_scope(state, saved_ro);
    super::functions::restore_caller_slots(saved_slots, state);
    super::functions::restore_caller_args(saved_args, state);
    Some(restore_block)
}

/// Capture a **multi-statement / pipeline** `$(…)` body's stdout into
/// `target_name`. Mirrors `capture_func_into` but for a raw statement list
/// (no positional/func-arg setup — a cmdsub inherits the caller's positionals).
/// `CaptureEnd` is marked `tainted` (external output). Returns the block to
/// continue lowering from.
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
    let buffered = super::capture_purity::capture_body_is_pure(body, &state.function_bodies);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::CaptureStart { site, buffered });

    let call_block = ssa.new_block("cap_body");
    ssa.block_mut(block).terminator = Terminator::Goto(call_block);
    let restore_block = ssa.new_block("capbody_ret");

    let saved_return_target = state.return_target;
    state.return_target = Some(restore_block);
    let exit_block = lower_stmts(body, ssa, state, call_block);
    state.return_target = saved_return_target;
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

    let target = state.define(target_name);
    ssa.block_mut(restore_block)
        .insts
        .push(SsaInst::CaptureEnd {
            site,
            buffered,
            target,
            // Match the CST decision exactly (else the SSA-taint backstop ICEs): the
            // capture is tainted iff the body runs an external / getenv / arg / argv.
            tainted: crate::validate::cmdsub_is_taint_source(body),
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
            "try_capture_inline runs only when expr_to_value produced a FuncCall, \
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
///
/// `handle_attest_stmt`'s second guard — the `AttestPredicate::ArithConvert`
/// arm after the scalar form is consumed — is deliberately *not* tested: it
/// is a DD#116 property-2 totality arm, unreachable by construction in the
/// same function rather than by an earlier pass, so nothing can call it.
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

    #[test]
    fn an_unclassifiable_attest_is_a_loud_ice() {
        ice_on("attest", vec![], "SB4014");
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

    /// `extract_func_args` is reached only from `try_capture_inline`, which
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
