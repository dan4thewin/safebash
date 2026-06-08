//! Extracted handlers for the largest StmtKind match arms.

use super::expressions::{expr_to_string, expr_to_value};
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
        BuiltinCmd::Test => try_test_arg_capture(c, ssa, *block),
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

/// `[[ … ]]` attached as an argument (the parser may emit `test` /
/// `[` commands whose first/only arg carries the `Test` expression
/// kind). The first matching arg wins. Returns `None` when no
/// `ExprKind::Test` is found so the caller falls through.
fn try_test_arg_capture(c: &Command, ssa: &mut SsaProgram, block: BlockId) -> Option<BlockId> {
    for arg in &c.args {
        if let ExprKind::Test(test_expr) = &arg.kind {
            ssa.block_mut(block)
                .insts
                .push(SsaInst::Test(test_expr.clone()));
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

fn handle_bless_stmt(
    name: &str,
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // First arg is variable name, remaining args should contain [[ test_expr ]]
    if c.args.len() >= 2 {
        let var_name = expr_to_string(&c.args[0]);
        for arg in &c.args[1..] {
            if let ExprKind::Test(test_expr) = &arg.kind {
                let source_ref = state.lookup(&var_name);
                let target_ref = state.define(&var_name);
                ssa.block_mut(block).insts.push(SsaInst::Bless {
                    target: target_ref,
                    source: source_ref,
                    test: test_expr.clone(),
                });
                return block;
            }
        }
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
fn build_producer_stages(
    cmds: &[Statement],
    state: &mut VersionState,
) -> Vec<(String, Vec<SsaValue>, Vec<Redirect>)> {
    let mut stages = vec![];
    for cmd in cmds {
        if let StmtKind::Command(c) = &cmd.kind {
            let name = expr_to_string(&c.name);
            let args: Vec<SsaValue> = c.args.iter().map(|a| expr_to_value(a, state)).collect();
            stages.push((resolve_pipeline_command(&name), args, vec![]));
        } else if let StmtKind::Redirected(r) = &cmd.kind {
            if let StmtKind::Command(c) = &r.body.kind {
                let name = expr_to_string(&c.name);
                let args: Vec<SsaValue> = c.args.iter().map(|a| expr_to_value(a, state)).collect();
                stages.push((resolve_pipeline_command(&name), args, r.redirects.clone()));
            }
        }
    }
    stages
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
    let stages = build_producer_stages(&p.commands[..p.commands.len() - 1], state);
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
    let stages = build_producer_stages(&p.commands[..p.commands.len() - 1], state);
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
            let name = expr_to_string(&c.name);
            let args: Vec<SsaValue> = c.args.iter().map(|a| expr_to_value(a, state)).collect();
            stages.push((resolve_pipeline_command(&name), args, redirects));
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
    let val = expr_to_value(&a.value, state);
    let idx_val = expr_to_value(idx, state);
    let is_int = !state.assoc_arrays.contains(&a.name);
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
