//! Extracted handlers for the largest StmtKind match arms.

use super::cmd_handlers::{
    expand_dollar_at_in_body, resolve_function_name, setup_callee_positionals,
};
use super::expressions::{
    arg_to_value, expr_to_string, expr_to_value, subscript_to_value, test_to_ssa,
};
use super::functions::{collect_local_vars, lift_int_func_calls};
use super::pipelines::lower_pipe_while_read;
use super::resolution::resolve_pipeline_command;
use super::statements::{lower_one, lower_stmts};
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

    // var =~ s/pattern/replacement/flags — regex mutation
    // (single-arg commands only; not keyed on a builtin name)
    if let Some(b) = try_regex_substitution(c, &name, state, ssa, block) {
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
        BuiltinCmd::Shift => Some(handle_shift_stmt(c, ssa, state, *block)),
        BuiltinCmd::Break => Some(handle_break_stmt(c, state, ssa, *block)),
        BuiltinCmd::Continue => Some(handle_continue_stmt(c, state, ssa, *block)),
        BuiltinCmd::Return => Some(handle_return_stmt(c, ssa, state, block)),
        BuiltinCmd::Mapfile => try_handle_mapfile(c, ssa, *block),
        BuiltinCmd::Signal => Some(super::cmd_handlers::handle_signal(
            name, c, stmt, ssa, state, *block,
        )),
        BuiltinCmd::Trap => Some(super::cmd_handlers::handle_trap(
            name, c, stmt, ssa, state, *block,
        )),
        BuiltinCmd::Unset => Some(super::cmd_handlers::handle_unset(
            name, c, stmt, ssa, state, *block,
        )),
        BuiltinCmd::Read => Some(super::cmd_handlers::handle_read(
            name, c, stmt, ssa, state, *block,
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
fn try_regex_substitution(
    c: &Command,
    name: &str,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> Option<BlockId> {
    if c.args.len() != 1 {
        return None;
    }
    let arg_s = expr_to_string(&c.args[0]);
    let after_prefix = arg_s.strip_prefix("s/")?;
    let parts: Vec<&str> = after_prefix.splitn(3, '/').collect();
    if parts.len() < 2 {
        return None;
    }
    let pattern = parts[0].to_string();
    let replacement = parts[1].to_string();
    let flags = if parts.len() > 2 { parts[2] } else { "" };

    // DD#39: compile-time regex validation
    if !pattern.contains('$') {
        if let Err(msg) = crate::regex_validate::validate_regex(&pattern) {
            state.diagnostics.push(format!(
                "1:1: error [SB5023] invalid regex in substitution: {}",
                msg
            ));
            return Some(block);
        }
    }

    let global = if flags.contains('g') { "1" } else { "0" };
    let var_ref = state.lookup(name);
    let new_ref = state.define(name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: new_ref,
        value: SsaValue::ParamExpansion {
            var: var_ref,
            operator: "regex_replace".to_string(),
            operand: format!("{}\x00{}\x00{}", pattern, replacement, global),
        },
    });
    Some(block)
}

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
    block: BlockId,
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
    let args: Vec<SsaValue> = c
        .args
        .iter()
        .map(|a| {
            let v = expr_to_value(a, state);
            // DD#55: $(f) for -> int function is a type error
            if let SsaValue::FuncCall(fname) = &v {
                if state.function_int_typed.contains(fname.as_str()) {
                    state.diagnostics.push(format!(
                        "{}:{}: error [SB4018] $({}): cannot capture -> int function with $() — use $(({}))",
                        a.span.line, a.span.col, fname, fname
                    ));
                }
            }
            v
        })
        .collect();
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

fn handle_shift_stmt(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let n = c
        .args
        .first()
        .map(|a| expr_to_string(a).parse::<usize>().unwrap_or(1))
        .unwrap_or(1);
    // $1 ← $(1+n), $2 ← $(2+n), ..., $9 ← $(9+n) or ""
    for i in 1..=9usize {
        let src_idx = i + n;
        let val = if src_idx <= 9 {
            SsaValue::Var(state.lookup(&src_idx.to_string()))
        } else {
            SsaValue::Literal(String::new())
        };
        let target = state.define(&i.to_string());
        ssa.block_mut(block)
            .insts
            .push(SsaInst::Assign { target, value: val });
    }
    // Decrement $#
    let _argc_ref = state.lookup("#");
    let new_argc = state.define("#");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: new_argc,
        value: SsaValue::Literal("__shift__".to_string()), // placeholder
    });
    block
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

/// `mapfile -t arr` / `readarray -t arr` → Mapfile instruction.
/// Returns `None` if no array name is present (caller falls through to
/// general command handling).
fn try_handle_mapfile(c: &Command, ssa: &mut SsaProgram, block: BlockId) -> Option<BlockId> {
    let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
    let mut map_mode = false;
    let mut separator = '=';
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-m" {
            map_mode = true;
        } else if args[i].starts_with("-f") {
            if args[i].len() > 2 {
                separator = args[i].chars().nth(2).unwrap_or('=');
            } else if i + 1 < args.len() {
                separator = args[i + 1].chars().next().unwrap_or('=');
                i += 1;
            }
        }
        i += 1;
    }
    let var_names: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let arr_name = var_names.first()?;
    ssa.block_mut(block).insts.push(SsaInst::Mapfile {
        array: arr_name.to_string(),
        fd: 0,
        map_mode,
        separator,
    });
    Some(block)
}

/// Handle StmtKind::Pipeline — multi-command pipe chains.
pub(super) fn handle_pipeline(
    p: &Pipeline,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    if p.commands.len() > 1 {
        let last = &p.commands[p.commands.len() - 1];
        let is_while_read = matches!(&last.kind, StmtKind::While(_));
        let is_mapfile = is_mapfile_last(last);
        let is_lastpipe_read = is_lastpipe_read_last(last);

        if is_while_read || is_mapfile {
            block = handle_pipe_while_or_mapfile(p, last, is_mapfile, ssa, state, block);
        } else if is_lastpipe_read {
            handle_lastpipe_read(p, last, ssa, state, block);
        } else {
            handle_regular_pipeline(p, ssa, state, block);
        }
    } else {
        for cmd in &p.commands {
            block = lower_one(cmd, ssa, state, block);
        }
    }
    if p.negated {
        ssa.block_mut(block)
            .insts
            .push(SsaInst::sigil(BuiltinCmd::NegateTestResult, vec![], true));
    }
    block
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
) -> Vec<(String, Vec<SsaValue>, Vec<Redirect>)> {
    let mut stages = vec![];
    for (idx, cmd) in cmds.iter().enumerate() {
        let (inner, redirects) = match &cmd.kind {
            StmtKind::Redirected(r) => (&r.body.kind, r.redirects.clone()),
            other => (other, vec![]),
        };
        if let StmtKind::Command(c) = inner {
            push_command_stage(c, redirects, idx, ssa, state, &mut stages);
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
    stages: &mut Vec<(String, Vec<SsaValue>, Vec<Redirect>)>,
) {
    let name = expr_to_string(&c.name);
    if state.known_functions.contains(&name) {
        if let Some(entry) = build_function_pipeline_stage(&name, c, ssa, state, idx) {
            stages.push((format!("__compound_{}", entry), vec![], redirects));
            return;
        }
    }
    let args: Vec<SsaValue> = c.args.iter().map(|a| arg_to_value(a, state)).collect();
    stages.push((resolve_pipeline_command(&name), args, redirects));
}

/// Parse `read` flags (-d <c>, -d<c>, -f <c>, -f<c>) and collect var names.
/// Returns (var_names, delim, field_sep).
fn parse_read_flags(args: &[Expr]) -> (Vec<String>, Option<u8>, Option<u8>) {
    let mut var_names = vec![];
    let mut delim: Option<u8> = None;
    let mut field_sep: Option<u8> = None;
    let mut skip_next = false;
    for arg in args {
        let s = expr_to_string(arg);
        if skip_next {
            if field_sep == Some(0) {
                field_sep = Some(s.as_bytes().first().copied().unwrap_or(0));
            }
            skip_next = false;
            continue;
        }
        if s == "-d" {
            skip_next = true;
            delim = Some(0);
            continue;
        }
        if let Some(rest) = s.strip_prefix("-d") {
            delim = Some(rest.as_bytes().first().copied().unwrap_or(0));
            continue;
        }
        if s == "-f" {
            skip_next = true;
            field_sep = Some(0);
            continue;
        }
        if let Some(rest) = s.strip_prefix("-f") {
            field_sep = Some(rest.as_bytes().first().copied().unwrap_or(0));
            continue;
        }
        if s.starts_with('-') {
            continue;
        }
        var_names.push(s);
    }
    (var_names, delim, field_sep)
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
    let stages = build_producer_stages(&p.commands[..p.commands.len() - 1], ssa, state);
    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
        stages,
        capture_read_fd: true,
        checked: true,
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
    let stages = build_producer_stages(&p.commands[..p.commands.len() - 1], ssa, state);
    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
        stages,
        capture_read_fd: true,
        checked: true,
    });
    if let StmtKind::Command(c) = &last.kind {
        let (var_names, delim, field_sep) = parse_read_flags(&c.args);
        if var_names.len() == 1 {
            let target = state.define(&var_names[0]);
            ssa.block_mut(block).insts.push(SsaInst::ReadLine {
                target,
                fd: -1,
                delim,
                redirects: vec![],
                clean_pipe: true,
            });
        } else if var_names.len() > 1 {
            let targets: Vec<VarRef> = var_names.iter().map(|n| state.define(n)).collect();
            ssa.block_mut(block).insts.push(SsaInst::ReadLineMulti {
                targets,
                fd: -1,
                delim,
                redirects: vec![],
                clean_pipe: true,
                field_sep,
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
    for (idx, cmd) in p.commands.iter().enumerate() {
        let (inner, redirects) = match &cmd.kind {
            StmtKind::Suppressed(_s) => {
                // - prefix in pipeline: skip suppressed stages entirely
                continue;
            }
            StmtKind::Redirected(r) => (&r.body.kind, r.redirects.clone()),
            other => (other, vec![]),
        };
        if let StmtKind::Command(c) = inner {
            push_command_stage(c, redirects, idx, ssa, state, &mut stages);
        } else {
            // Compound stage: lower into its own SSA blocks, record entry for codegen
            let compound_entry = ssa.new_block(&format!("pipe_compound_{}", idx));
            let compound_exit = lower_one(cmd, ssa, state, compound_entry);
            ssa.block_mut(compound_exit).terminator = Terminator::Return;
            ssa.compound_pipeline_blocks.push(compound_entry);
            stages.push((format!("__compound_{}", compound_entry), vec![], vec![]));
        }
    }
    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
        stages,
        capture_read_fd: false,
        checked: true,
    });
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
    let resolved = resolve_function_name(name, state);
    let mut body = state.function_bodies.get(&resolved).cloned()?;
    // DD#60: expand $@/"$@" to the explicit positional list before lowering.
    expand_dollar_at_in_body(&mut body, c.args.len());

    let saved = state.snapshot();
    let saved_return = state.return_target;

    let entry = ssa.new_block(&format!("pipe_fn_{}_{}", name, idx));
    setup_callee_positionals(name, &body, c, ssa, state, entry);

    // `return` inside the body jumps to ret_block, which ends the forked child.
    let ret_block = ssa.new_block(&format!("pipe_fn_ret_{}_{}", name, idx));
    state.return_target = Some(ret_block);
    let exit = lower_stmts(&body, ssa, state, entry);
    ssa.block_mut(exit).terminator = Terminator::Goto(ret_block);
    ssa.block_mut(ret_block).terminator = Terminator::Return;

    state.return_target = saved_return;
    state.restore(&saved);

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
    handle_plain_assign(a, stmt, state, ssa, block)
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
    let is_int = !state.assoc_arrays.contains(&a.name);
    // `a[i] += v` is read-modify-write: a[i] = ${a[i]} ++ v. (Plain
    // `a[i] = v` discards the current element, which is correct.)
    let val = if a.op == crate::ir::AssignOp::Append {
        let current = subscript_to_value(&a.name, idx, state);
        SsaValue::Concat(vec![current, rhs])
    } else {
        rhs
    };
    let idx_val = expr_to_value(idx, state);
    ssa.block_mut(block).insts.push(SsaInst::SubscriptAssign {
        array: a.name.clone(),
        index: idx_val,
        value: val,
        is_integer_key: is_int,
    });
    block
}

/// `var += val` — append form. Dispatches on the value shape:
/// IndexedArray literal → element-by-element ArrayAppend; assoc
/// array → ArrayAppend; otherwise string-concat (or arithmetic add
/// for int-typed vars).
fn handle_append_assign(
    a: &Assignment,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) -> BlockId {
    let val = expr_to_value(&a.value, state);
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
    stmt: &Statement,
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    mut block: BlockId,
) -> BlockId {
    let val = expr_to_value(&a.value, state);
    if let SsaValue::FuncCall(fname) = &val {
        // DD#55: $(f) for -> int function is a type error
        if state.function_int_typed.contains(fname.as_str()) {
            state.diagnostics.push(format!(
                "{}:{}: error [SB4018] $({}): cannot capture -> int function with $() — use $(({}))",
                stmt.span.line, stmt.span.col, fname, fname
            ));
            return block;
        }
        if let Some(b) = try_capture_inline(a, fname, state, ssa, block) {
            return b;
        }
    }
    // DD#58: lift IntFuncCall out of arithmetic expressions
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
    let body = state.function_bodies.get(fname).cloned()?;
    let locals = collect_local_vars(&body);

    ssa.block_mut(block).insts.push(SsaInst::CaptureStart);
    let (saved, saved_argc) = save_positionals(state);
    let saved_locals = save_locals(fname, &locals, state, ssa, block);

    let func_args = extract_func_args(&a.value);
    setup_func_args(&func_args, state, ssa, block);

    let call_block = ssa.new_block(&format!("cap_{}", fname));
    ssa.block_mut(block).terminator = Terminator::Goto(call_block);
    let restore_block = ssa.new_block(&format!("capret_{}", fname));
    let saved_return_target = state.return_target;
    state.return_target = Some(restore_block);
    let exit_block = lower_stmts(&body, ssa, state, call_block);
    state.return_target = saved_return_target;
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

    let target = state.define(&a.name);
    ssa.block_mut(restore_block)
        .insts
        .push(SsaInst::CaptureEnd { target });

    restore_positionals(&saved, saved_argc, &saved_locals, state, ssa, restore_block);
    Some(restore_block)
}

/// Snapshot the current `$1..$9` and `$#` versions so they can be
/// rolled back after the inlined call returns. No instructions are
/// emitted; the caller saves these as `Var` references.
fn save_positionals(state: &mut VersionState) -> (Vec<(String, VarRef)>, VarRef) {
    let saved: Vec<(String, VarRef)> = (1..=9)
        .map(|i| (i.to_string(), state.lookup(&i.to_string())))
        .collect();
    let saved_argc = state.lookup("#");
    (saved, saved_argc)
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

/// Install `$#` and `$1..$N` in `block`, then clear `$(N+1)..$9`
/// (DD#46: keeps the caller's higher slots from leaking through).
fn setup_func_args(
    func_args: &[Expr],
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) {
    let argc_val = SsaValue::Literal(func_args.len().to_string());
    let argc_var = state.define("#");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: argc_var,
        value: argc_val,
    });
    for (i, arg) in func_args.iter().enumerate() {
        let v = expr_to_value(arg, state);
        let param_var = state.define(&(i + 1).to_string());
        ssa.block_mut(block).insts.push(SsaInst::Assign {
            target: param_var,
            value: v,
        });
    }
    for i in (func_args.len() + 1)..=9 {
        let param_var = state.define(&i.to_string());
        ssa.block_mut(block).insts.push(SsaInst::Assign {
            target: param_var,
            value: SsaValue::Literal(String::new()),
        });
    }
}

/// Roll the saved positionals, `$#`, and locals back into `block`.
fn restore_positionals(
    saved: &[(String, VarRef)],
    saved_argc: VarRef,
    saved_locals: &[(String, VarRef)],
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    block: BlockId,
) {
    for (param_name, saved_ref) in saved {
        let rv = state.define(param_name);
        ssa.block_mut(block).insts.push(SsaInst::Assign {
            target: rv,
            value: SsaValue::Var(saved_ref.clone()),
        });
    }
    let ra = state.define("#");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: ra,
        value: SsaValue::Var(saved_argc),
    });
    for (local_name, saved_ref) in saved_locals {
        let rv = state.define(local_name);
        ssa.block_mut(block).insts.push(SsaInst::Assign {
            target: rv,
            value: SsaValue::Var(saved_ref.clone()),
        });
    }
}
