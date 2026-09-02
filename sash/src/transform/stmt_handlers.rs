//! Extracted handlers for the largest StmtKind match arms.

use super::cmd_handlers::check_callee_arity;
use super::expressions::{
    arg_to_value, expr_to_string, expr_to_value, subscript_to_value, test_to_ssa,
};
use super::functions::{lift_int_func_args, lift_int_func_calls};
use super::pipelines::lower_pipe_while_read;
use super::resolution::resolve_pipeline_command;
use super::statements::{lower_one, lower_stmts, spill_scalar_redirect_targets};
use crate::ir::*;
use crate::ssa::*;

/// Handle StmtKind::Command — command execution, function inlining, bless, regex.
///
/// This is the largest arm because commands can be:
/// - User-defined function calls (inlined at the call site)
/// - Bless statements (runtime taint validation)
/// - Regex mutation (var =~ s/re/str/flags)
/// - External commands (emitted as SsaInst::Command)
pub(super) fn handle_command(
    c: &Command,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    let name = expr_to_string(&c.name);
    let kind = BuiltinCmd::from_name(&name);

    // DD#70 (B): `match` and a bare `[[ … ]]` are booleans that must be
    // consumed — a bare or `-`-suppressed one is a compile error.
    check_boolean_must_consume(kind, stmt, state);

    // DD#63: the 3-arg out-array form `match S P ARR` (the 2-arg boolean
    // and `$(match …)` value forms fall through).
    if let Some(b) = try_handle_match_into(kind, c, ssa, state, block) {
        return b;
    }

    if let Some(b) = try_builtin(kind, &name, c, stmt, ssa, state, &mut block) {
        return b;
    }

    check_non_void_bare_call(&name, stmt, state);
    super::cmd_handlers::handle_function_call_or_command(&name, c, stmt, ssa, state, block)
}

/// Dispatch a `BuiltinCmd` to its handler. Most builtins always
/// consume the statement (return `Some`); `Test` and `Mapfile` may
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
        BuiltinCmd::Bless => Some(handle_bless_stmt(name, c, ssa, state, *block)),
        BuiltinCmd::Test => try_test_arg_capture(c, ssa, state, *block),
        BuiltinCmd::Break => Some(handle_break_stmt(c, state, ssa, *block)),
        BuiltinCmd::Continue => Some(handle_continue_stmt(c, state, ssa, *block)),
        BuiltinCmd::Return => Some(handle_return_stmt(c, ssa, state, block)),
        BuiltinCmd::Mapfile | BuiltinCmd::Argv => try_fill_array(kind, c, ssa, state, *block),
        BuiltinCmd::Signal => Some(super::cmd_handlers::handle_signal(
            name, c, stmt, ssa, state, *block,
        )),
        BuiltinCmd::Unset => Some(super::cmd_handlers::handle_unset(
            name, c, stmt, ssa, state, *block,
        )),
        BuiltinCmd::Read => Some(super::cmd_handlers::handle_read(
            c, stmt, ssa, state, *block,
        )),
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
    if kind != BuiltinCmd::Match || c.args.len() != 3 {
        return None;
    }
    let subject = expr_to_value(&c.args[0], state);
    let pattern = expr_to_value(&c.args[1], state);
    let array = expr_to_string(&c.args[2]);
    ssa.block_mut(block).insts.push(SsaInst::MatchInto {
        subject,
        pattern,
        array,
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
            let ssa_test = test_to_ssa(test_expr, state);
            ssa.block_mut(block).insts.push(SsaInst::Test(ssa_test));
            return Some(block);
        }
    }
    None
}

/// `var s/pattern/replacement/flags` — bash-style sed substitution
/// against the named variable. DD#39 compile-time regex validation.
/// Single-arg form only. Returns `None` when the arg does not start
/// with `s/` or the substitution body is malformed.
/// DD#55/SB4016: a bare call of a non-void (`-> int`) function in
/// statement position must be handled by `if`/`||` or captured by
/// `$((…))`. Emits a diagnostic when violated; does not affect
/// codegen (the caller still threads the call through).
fn check_non_void_bare_call(name: &str, stmt: &Statement, state: &mut VersionState) {
    if state.function_non_void.contains(name) && state.in_condition == 0 {
        state.diagnostics.push(format!(
            "{}:{}: error [SB4016] function '{}' returns a status — handle with if/|| or capture with $(({}))",
            stmt.span.line, stmt.span.col, name, name
        ));
    }
}

/// DD#70 (B): `match` and a bare `[[ … ]]` are booleans that **must be
/// consumed**. In a consuming position (if/while/&&/||) they are fine.
/// Otherwise it is a compile error in the "ignored result" family (adjacent
/// to SB4016): SB4018 for a bare result, SB4019 when `-` is applied (a
/// must-consume result has no runtime verdict to suppress).
fn check_boolean_must_consume(kind: BuiltinCmd, stmt: &Statement, state: &mut VersionState) {
    let construct = match kind {
        BuiltinCmd::Match => "match",
        BuiltinCmd::Test => "[[ … ]]",
        _ => return,
    };
    if state.in_condition > 0 {
        return;
    }
    let diag = if state.suppressed > 0 {
        format!(
            "{}:{}: error [SB4019] `-` cannot suppress `{}` — it is a boolean that must be consumed (if/||) or discarded with `|| :`",
            stmt.span.line, stmt.span.col, construct
        )
    } else {
        format!(
            "{}:{}: error [SB4018] `{}` result is unused — consume it with if/`&&`/`||`, or discard with `|| :`",
            stmt.span.line, stmt.span.col, construct
        )
    };
    state.diagnostics.push(diag);
}

fn handle_bless_stmt(
    name: &str,
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    // DD#24 scalar / DD#61 array & map bless forms.
    if let Some((form, test)) = classify_bless(&c.args) {
        let test = test.clone();
        match form {
            BlessForm::Scalar { var } => {
                let var = var.to_string();
                let source = state.lookup(&var);
                // Resolve the predicate's operands BEFORE defining the target
                // version, so `$var` in the test binds to the source (the
                // value being blessed). emit_bless evaluates the test before
                // the target slot exists. See dispatch.rs::emit_bless.
                let ssa_test = test_to_ssa(&test, state);
                let target = state.define(&var);
                ssa.block_mut(block).insts.push(SsaInst::Bless {
                    target,
                    source,
                    test: ssa_test,
                });
            }
            BlessForm::Indexed { binder, array } => {
                push_bless_all(
                    ssa,
                    state,
                    block,
                    array,
                    vec![binder.to_string()],
                    test,
                    false,
                );
            }
            BlessForm::Map { key, val, map } => {
                let binders = vec![key.to_string(), val.to_string()];
                push_bless_all(ssa, state, block, map, binders, test, true);
            }
        }
        return block;
    }
    // Fallback: no TestExpr — emit as regular command (caught by taint checker)
    let raw: Vec<SsaValue> = c.args.iter().map(|a| expr_to_value(a, state)).collect();
    let args = lift_int_func_args(raw, ssa, state, &mut block);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::command(name.to_string(), args, vec![], true));
    block
}

/// DD#61: emit a statement-form bless-all — `BlessAll` (exits on a failing
/// element) followed by `BlessRetag` (produces the clean version).
fn push_bless_all(
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
    array: &str,
    binders: Vec<String>,
    test: crate::ir::TestExpr,
    is_map: bool,
) {
    let source = state.lookup(array);
    // Define the binders so they receive globally-unique SSA versions,
    // resolve the predicate against them, then restore current versions so
    // the binders stay loop-local (DD#61 shadow semantics). The version
    // counter is not rewound by restore, so the binder versions remain
    // unique and match the slots emit_bless_all creates for them.
    let snap = state.snapshot();
    let binder_refs: Vec<VarRef> = binders.iter().map(|b| state.define(b)).collect();
    let ssa_test = test_to_ssa(&test, state);
    state.restore(&snap);
    ssa.block_mut(block).insts.push(SsaInst::BlessAll {
        source: source.clone(),
        binders: binder_refs,
        test: ssa_test,
        is_map,
        if_form: false,
    });
    let target = state.define(array);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::BlessRetag { target, source });
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
        .map(|a| expr_to_string(a).parse::<usize>().unwrap_or(1))
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
        .map(|a| expr_to_string(a).parse::<usize>().unwrap_or(1))
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
    let SsaValue::IntFuncCall { name, args } = val else {
        return None;
    };
    let (tf, header, aname) = state.tail_loop.clone()?;
    if *name != tf {
        return None;
    }
    let lifted = lift_int_func_args(args.clone(), ssa, state, block);
    Some(super::functions::emit_tail_call(
        header, &aname, lifted, ssa, state, *block,
    ))
}

/// `mapfile -t arr` / `readarray -t arr` → Mapfile instruction.
/// Returns `None` if no array name is present (caller falls through to
/// general command handling).
/// Array out-param fills: `mapfile`/`readarray` and DD#60 `argv [N] a`.
fn try_fill_array(
    kind: BuiltinCmd,
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    match kind {
        BuiltinCmd::Mapfile => try_handle_mapfile(c, ssa, state, block),
        BuiltinCmd::Argv => try_handle_argv(c, ssa, state, block),
        _ => None,
    }
}

/// DD#60: `argv [N] a` out-param builtin. The trailing positional is the
/// destination array; an optional leading non-negative integer is the
/// slice start (`argv a` = whole argv, `argv N a` = argv[N:]). Emits an
/// `ArgvFill` instruction (auto-declares + taints the array, like
/// mapfile / read -a). Returns `None` for any other shape.
fn try_handle_argv(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
    let array = args.last()?.clone();
    let start = match args.len() {
        1 => 0,
        2 => args[0].parse::<i64>().ok().filter(|n| *n >= 0)?,
        _ => return None,
    };
    // Auto-declare (DG 2026-07-08): if `a` isn't already an array, create
    // the versioned slot the same way `array a=()` does, so the fill and later
    // len/iteration/element reads agree with no prior `array a=()`.
    autodeclare_bulk_target(&array, false, ssa, state, block);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::ArgvFill { array, start });
    Some(block)
}

/// Auto-declare a bulk-read target array (mirrors argv, DG 2026-07-08): if
/// `name` isn't already a collection, emit the empty-array/-map Assign that
/// creates the versioned `name_0` slot — so the fill (argv/mapfile/read -a) and
/// any later `len`/iteration/element read resolve without a prior `array
/// name=()`. Without this the codegen `find_array_slot` panics (ICE). `as_map`
/// creates an associative array (`mapfile -m`).
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

fn try_handle_mapfile(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
    let (map_mode, separator) = parse_mapfile_flags(&args);
    let var_names: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let arr_name = var_names.first()?;
    // Auto-declare the target so `find_array_slot` resolves it with no prior
    // `array a=()` / `map a=()` (else a codegen ICE). `-m` → an assoc array.
    autodeclare_bulk_target(arr_name, map_mode, ssa, state, block);
    ssa.block_mut(block).insts.push(SsaInst::Mapfile {
        array: arr_name.to_string(),
        fd: 0,
        map_mode,
        separator,
    });
    Some(block)
}

/// Parse `mapfile` flags: `-m` (map mode) and `-f<c>` / `-f <c>` (the map
/// key/value separator, default `=`).
fn parse_mapfile_flags(args: &[String]) -> (bool, char) {
    let map_mode = args.iter().any(|a| a == "-m");
    (map_mode, mapfile_separator(args).unwrap_or('='))
}

/// The `-f<c>` (inline) or `-f <c>` (next arg) map separator, if present.
fn mapfile_separator(args: &[String]) -> Option<char> {
    let pos = args.iter().position(|a| a.starts_with("-f"))?;
    args[pos]
        .strip_prefix("-f")
        .and_then(|rest| rest.chars().next())
        .or_else(|| args.get(pos + 1).and_then(|next| next.chars().next()))
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
    ssa.block_mut(block)
        .insts
        .push(SsaInst::sigil(BuiltinCmd::NegateTestResult, vec![], true));
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
    let is_while_read = matches!(&last.kind, StmtKind::While(_));
    let is_mapfile = is_mapfile_last(last);
    if is_while_read || is_mapfile {
        return handle_pipe_while_or_mapfile(p, last, is_mapfile, ssa, state, block);
    }
    if is_lastpipe_read_last(last) {
        handle_lastpipe_read(p, last, ssa, state, block);
    } else {
        handle_regular_pipeline(p, ssa, state, block);
    }
    block
}

/// DD#70 (B): `! X` produces a must-consume boolean (for any operand). In a
/// consuming position (if/while/&&/||) it is fine; a bare result is SB4018,
/// and `-` applied to it is SB4019.
fn check_negation_must_consume(stmt: &Statement, state: &mut VersionState) {
    if state.in_condition > 0 {
        return;
    }
    let diag = if state.suppressed > 0 {
        format!(
            "{}:{}: error [SB4019] `-` cannot suppress `!` — it is a boolean that must be consumed (if/||) or discarded with `|| :`",
            stmt.span.line, stmt.span.col
        )
    } else {
        format!(
            "{}:{}: error [SB4018] `!` result is unused — consume it with if/`&&`/`||`, or discard with `|| :`",
            stmt.span.line, stmt.span.col
        )
    };
    state.diagnostics.push(diag);
}

fn is_mapfile_last(last: &Statement) -> bool {
    match &last.kind {
        StmtKind::Command(c) => {
            let n = expr_to_string(&c.name);
            let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
            n == "mapfile" || n == "readarray" || (n == "read" && args.iter().any(|a| a == "-a"))
        }
        _ => false,
    }
}

fn is_lastpipe_read_last(last: &Statement) -> bool {
    match &last.kind {
        StmtKind::Command(c) => {
            let n = expr_to_string(&c.name);
            let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
            n == "read" && !args.iter().any(|a| a == "-a")
        }
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
    let name = expr_to_string(&c.name);
    if state.known_functions.contains(&name) {
        if let Some(entry) = build_function_pipeline_stage(&name, c, ssa, state, idx) {
            stages.push((format!("__compound_{}", entry), vec![], redirects));
            return;
        }
    }
    let args: Vec<SsaValue> = c.args.iter().map(|a| arg_to_value(a, state)).collect();
    stages.push((resolve_pipeline_command(&name, state), args, redirects));
}

/// `cmd1 | ... | while read -r var` or `cmd1 | ... | mapfile`.
/// Emit producer stages as a pipeline that returns a pipe fd,
/// then lower the last stage with fd = -1 (pipe-fd sentinel).
fn handle_pipe_while_or_mapfile(
    p: &Pipeline,
    last: &Statement,
    is_mapfile: bool,
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
    if is_mapfile {
        block = lower_one(last, ssa, state, block);
        rewrite_lastpipe_consumer_fds(ssa);
    } else {
        block = lower_pipe_while_read(last, ssa, state, block);
    }
    block
}

/// After lowering a mapfile lastpipe consumer, repoint its read fd
/// (currently 0 = stdin) to -1 (pipe sentinel) so codegen reads from
/// the captured pipe fd instead of the inherited stdin.
fn rewrite_lastpipe_consumer_fds(ssa: &mut SsaProgram) {
    for bid in ssa.blocks.keys().cloned().collect::<Vec<_>>() {
        for inst in &mut ssa.block_mut(bid).insts {
            repoint_stdin_to_pipe_sentinel(inst);
        }
    }
}

fn repoint_stdin_to_pipe_sentinel(inst: &mut SsaInst) {
    match inst {
        SsaInst::Mapfile { fd, .. } if *fd == 0 => *fd = -1,
        SsaInst::ReadArray { fd, .. } if *fd == 0 => *fd = -1,
        _ => {}
    }
}

/// DD#20: lastpipe — `cmd | read -r var` runs `read` in the main shell.
/// Emit producer stages with captured pipe fd, then emit a ReadLine that
/// consumes from that pipe in the main process.
fn handle_lastpipe_read(
    p: &Pipeline,
    last: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    let stages = build_producer_stages(&p.commands[..p.commands.len() - 1], ssa, state, block);
    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
        stages,
        capture_read_fd: true,
        checked: true,
        suppressed: vec![],
        negated: vec![],
    });
    if let StmtKind::Command(c) = &last.kind {
        let flags = crate::ir::classify_read_flags(&c.args);
        if let Some(bad) = &flags.unknown_flag {
            state.diagnostics.push(format!(
                "{}:{}: error [SB5015] read: unknown flag '{}'",
                last.span.line, last.span.col, bad
            ));
            return;
        }
        // An array read (`-a`) in a pipeline is peeled off earlier by
        // `is_mapfile_last`, so only scalar reads reach here.
        if flags.var_names.len() == 1 {
            let target = state.define(&flags.var_names[0]);
            ssa.block_mut(block).insts.push(SsaInst::ReadLine {
                target,
                fd: -1,
                delim: flags.delim,
                redirects: vec![],
                clean_pipe: true,
            });
        } else if flags.var_names.len() > 1 {
            let targets: Vec<VarRef> = flags.var_names.iter().map(|n| state.define(n)).collect();
            ssa.block_mut(block).insts.push(SsaInst::ReadLineMulti {
                targets,
                fd: -1,
                delim: flags.delim,
                redirects: vec![],
                clean_pipe: true,
                field_sep: flags.field_sep,
            });
        }
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
        // DD#55: a `-`-suppressed segment still RUNS (real pipe stage); only
        // its exit status is excluded from the aggregate. DD#70: a `!`-negated
        // stage also runs, but its status is inverted before the aggregate.
        // Peel the wrapper and record the mask, rather than dropping the stage.
        let (unwrapped, is_suppressed, is_negated) = match &cmd.kind {
            StmtKind::Suppressed(inner) => (inner.as_ref(), true, false),
            StmtKind::Negated(inner) => (inner.as_ref(), false, true),
            _ => (cmd, false, false),
        };
        let stages_before = stages.len();
        lower_one_pipeline_stage(unwrapped, idx, ssa, state, block, &mut stages);
        // push_command_stage may add >1 stage (function inlining); mark each.
        suppressed.resize(stages.len(), false);
        negated.resize(stages.len(), false);
        if is_suppressed {
            suppressed[stages_before..].fill(true);
        }
        if is_negated {
            negated[stages_before..].fill(true);
        }
    }
    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
        stages,
        capture_read_fd: false,
        checked: true,
        suppressed,
        negated,
    });
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
    if let StmtKind::Command(c) = inner {
        push_command_stage(c, redirects, idx, ssa, state, block, stages);
    } else {
        let compound_entry = ssa.new_block(&format!("pipe_compound_{}", idx));
        let compound_exit = lower_one(stmt, ssa, state, compound_entry);
        ssa.block_mut(compound_exit).terminator = Terminator::Return;
        ssa.compound_pipeline_blocks.push(compound_entry);
        stages.push((format!("__compound_{}", compound_entry), vec![], vec![]));
    }
}

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
    let arg_values: Vec<SsaValue> = c.args.iter().map(|a| expr_to_value(a, state)).collect();
    check_callee_arity(name, &body, arg_values.len(), state);
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
    if let Some(ref idx) = a.index {
        return handle_subscript_assign(a, idx, state, ssa, block);
    }
    if a.op == crate::ir::AssignOp::Append {
        return handle_append_assign(a, state, ssa, block);
    }
    handle_plain_assign(a, state, ssa, block)
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
    block: BlockId,
) -> BlockId {
    let rhs = expr_to_value(&a.value, state);
    let is_int_key = !state.assoc_arrays.contains(&a.name);
    let is_int_val = state.type_map.get(&a.name).copied() == Some(crate::ir::VarType::Int);
    let val = build_subscript_rhs(a, idx, rhs, is_int_val, state);
    let idx_val = expr_to_value(idx, state);
    ssa.block_mut(block).insts.push(SsaInst::SubscriptAssign {
        array: a.name.clone(),
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
    a: &Assignment,
    idx: &Expr,
    rhs: SsaValue,
    is_int_val: bool,
    state: &VersionState,
) -> SsaValue {
    let append = a.op == crate::ir::AssignOp::Append;
    match (append, is_int_val) {
        (true, true) => {
            let current = subscript_to_value(&a.name, idx, state);
            SsaValue::IntCell(Box::new(SsaValue::ArithBinOp {
                left: Box::new(current),
                op: ArithOp::Add,
                right: Box::new(rhs),
            }))
        }
        (true, false) => {
            let current = subscript_to_value(&a.name, idx, state);
            SsaValue::Concat(vec![current, rhs])
        }
        (false, true) => SsaValue::IntCell(Box::new(rhs)),
        (false, false) => rhs,
    }
}

/// `var += val` — append form. Dispatches on the value shape:
/// IndexedArray literal → element-by-element ArrayAppend; assoc
/// array → ArrayAppend; otherwise string-concat (or arithmetic add
/// for int-typed vars).
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
        other => {
            if state.assoc_arrays.contains(&a.name) {
                ssa.block_mut(block).insts.push(SsaInst::ArrayAppend {
                    array: a.name.clone(),
                    value: other,
                });
            } else {
                append_scalar(a, other, state, ssa, block);
            }
        }
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
    let arg_values: Vec<SsaValue> = func_args.iter().map(|e| expr_to_value(e, state)).collect();
    capture_func_into(&a.name, fname, &arg_values, state, ssa, block)
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
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> Option<BlockId> {
    let body = state.function_bodies.get(fname).cloned()?;
    // DD#81 Part B: arity-check the `$( )` capture path too (it now carries the
    // call's args), matching the plain-call and pipeline-stage paths.
    super::cmd_handlers::check_callee_arity(fname, &body, arg_values.len(), state);

    ssa.block_mut(block).insts.push(SsaInst::CaptureStart);
    // DD#81: capture uses the same args-array model as inline_function_call
    // (save_caller_args + setup_callee_args), not the old $1..$9 positional
    // model — so the callee body's `${args[i]}` / `$N` resolve to the passed
    // args (previously the args were dropped and `${args[…]}` hit an
    // undefined array).
    let saved_args = super::functions::save_caller_args(state);
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
            target,
            tainted: false,
        });

    super::functions::exit_fn_body_scope(state, saved_ro);
    super::functions::restore_caller_args(saved_args, state);
    Some(restore_block)
}

/// Capture a **multi-statement / pipeline** `$(…)` body's stdout into
/// `target_name`. Mirrors `capture_func_into` but for a raw statement list
/// (no positional/func-arg setup — a cmdsub inherits the caller's positionals).
/// `CaptureEnd` is marked `tainted` (external output). Returns the block to
/// continue lowering from. Variable-assignment isolation (subshell semantics)
/// is handled by save/restore of the vars the body assigns.
pub(super) fn capture_body_into(
    target_name: &str,
    body: &[Statement],
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> BlockId {
    ssa.block_mut(block).insts.push(SsaInst::CaptureStart);

    // Subshell isolation: snapshot every var the body assigns, restore after.
    let assigned = collect_assigned_vars(body);
    let saved_vars = save_locals("bodycap", &assigned, state, ssa, block);

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
            target,
            // Match the CST decision exactly (else the SSA-taint backstop ICEs): the
            // capture is tainted iff the body runs an external / getenv / arg / argv.
            tainted: crate::validate::cmdsub_is_taint_source(body),
        });

    restore_locals(&saved_vars, state, ssa, restore_block);
    restore_block
}

/// Collect every variable a statement sequence assigns (plain `x=…`,
/// `int x=…`/`local x=…` declarations) — recursively through nested bodies.
/// Used to snapshot+restore for cmdsub subshell isolation.
fn collect_assigned_vars(stmts: &[Statement]) -> Vec<String> {
    let mut vars = Vec::new();
    collect_assigned_into(stmts, &mut vars);
    vars
}

fn collect_assigned_into(stmts: &[Statement], vars: &mut Vec<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assignment(a) => push_unique(vars, &a.name),
            StmtKind::Declaration(d) => d
                .assignments
                .iter()
                .for_each(|a| push_unique(vars, &a.name)),
            _ => {}
        }
        stmt.child_bodies()
            .iter()
            .for_each(|b| collect_assigned_into(b, vars));
    }
}

fn push_unique(vars: &mut Vec<String>, name: &str) {
    if !vars.iter().any(|v| v == name) {
        vars.push(name.to_string());
    }
}

/// Restore saved vars (from `save_locals`) to their pre-capture versions.
fn restore_locals(
    saved: &[(String, VarRef)],
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) {
    for (name, save_ref) in saved {
        let target = state.define(name);
        ssa.block_mut(block).insts.push(SsaInst::Assign {
            target,
            value: SsaValue::Var(save_ref.clone()),
        });
    }
}

/// Save each of `locals` into a fresh `__save_<fname>_<lname>`
/// variable in `block`. Returns the (name, save-ref) pairs for the
/// later restore.
fn save_locals(
    fname: &str,
    locals: &[String],
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> Vec<(String, VarRef)> {
    locals
        .iter()
        .map(|lname| {
            let save_name = format!("__save_{}_{}", fname, lname);
            let src = state.lookup(lname);
            let save_ref = state.define(&save_name);
            ssa.block_mut(block).insts.push(SsaInst::Assign {
                target: save_ref.clone(),
                value: SsaValue::Var(src),
            });
            (lname.clone(), save_ref)
        })
        .collect()
}

/// Re-extract the call args from the original `$(…)` expression. The
/// args were dropped when `expr_to_value` collapsed the substitution
/// into a `FuncCall(name)`, so we pull them back from the `Expr` here.
fn extract_func_args(value: &Expr) -> Vec<Expr> {
    let ExprKind::CommandSubstitution(stmts) = &value.kind else {
        return vec![];
    };
    if stmts.len() != 1 {
        return vec![];
    }
    let inner = match &stmts[0].kind {
        StmtKind::Redirected(r) => &r.body.kind,
        other => other,
    };
    if let StmtKind::Command(c) = inner {
        c.args.clone()
    } else {
        vec![]
    }
}
