//! Command-specific handlers extracted from handle_command.

use super::expressions::{arg_to_value, expr_to_string, expr_to_value};
use super::functions::{collect_local_vars, collect_max_positional};
use super::resolution::resolve_pipeline_command;
use super::statements::lower_stmts;
use crate::ir::*;
use crate::ssa::*;

/// Handle `read` command — emit ReadLine/ReadLineMulti instructions.
pub(super) fn handle_read(
    _name: &str,
    c: &Command,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
    let Some(flags) = parse_read_flags(&args, stmt, state) else {
        return block;
    };
    if flags.field_sep.is_some() && flags.var_names.len() <= 1 && flags.array_name.is_none() {
        state.diagnostics.push(format!(
            "{}:{}: error [SB5016] read -f: field separator requires multiple target variables",
            stmt.span.line, stmt.span.col
        ));
        return block;
    }
    emit_read_inst(&flags, ssa, state, block);
    block
}

/// Decoded `read` flags. `var_names` borrows from the original args
/// vector since they're consumed in place to build SSA targets.
struct ReadFlags<'a> {
    delim: Option<u8>,
    field_sep: Option<u8>,
    array_name: Option<String>,
    var_names: Vec<&'a String>,
}

/// Parse `read` flags: `-d DELIM` (record delimiter), `-f DELIM`
/// (field separator), `-a ARR`. Joined forms (`-dDELIM`, `-fSEP`)
/// are accepted. Any unknown flag (including `-r`, `-t`) is rejected
/// with SB5015 — returns `None` so the caller can early-out without
/// emitting an instruction.
fn parse_read_flags<'a>(
    args: &'a [String],
    stmt: &Statement,
    state: &mut VersionState,
) -> Option<ReadFlags<'a>> {
    let mut flags = ReadFlags {
        delim: None,
        field_sep: None,
        array_name: None,
        var_names: Vec::new(),
    };
    let mut skip_next = false;
    for (i, a) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if a == "-d" || a.starts_with("-d") {
            skip_next = consume_byte_flag(a, &mut flags.delim, args, i);
            continue;
        }
        if a == "-f" || a.starts_with("-f") {
            skip_next = consume_byte_flag(a, &mut flags.field_sep, args, i);
            continue;
        }
        if a == "-a" {
            if i + 1 < args.len() {
                flags.array_name = Some(args[i + 1].clone());
                skip_next = true;
            }
            continue;
        }
        if a.starts_with('-') {
            state.diagnostics.push(format!(
                "{}:{}: error [SB5015] read: unknown flag '{}'",
                stmt.span.line, stmt.span.col, a
            ));
            return None;
        }
        flags.var_names.push(a);
    }
    Some(flags)
}

/// Decode a `-X` byte flag in either joined form (`-Xbyte`) or
/// split form (`-X next`). Sets `*slot` to the first byte of the
/// chosen value and returns `true` iff a following arg was consumed
/// (so the caller can advance `skip_next`).
fn consume_byte_flag(arg: &str, slot: &mut Option<u8>, args: &[String], i: usize) -> bool {
    if arg.len() > 2 {
        *slot = Some(arg.as_bytes()[2]);
        false
    } else if i + 1 < args.len() {
        *slot = Some(args[i + 1].as_bytes()[0]);
        true
    } else {
        false
    }
}

/// Emit the right `Read*` SSA instruction for the parsed flags.
/// `-a` → `ReadArray`; one var → `ReadLine`; many vars → `ReadLineMulti`.
fn emit_read_inst(
    flags: &ReadFlags,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    if let Some(arr) = &flags.array_name {
        ssa.block_mut(block).insts.push(SsaInst::ReadArray {
            array: arr.clone(),
            fd: 0,
            delim: flags.delim,
        });
        return;
    }
    match flags.var_names.len() {
        0 => {}
        1 => {
            let target = state.define(flags.var_names[0]);
            ssa.block_mut(block).insts.push(SsaInst::ReadLine {
                target,
                fd: 0,
                delim: flags.delim,
                redirects: vec![],
                clean_pipe: false,
            });
        }
        _ => {
            let targets: Vec<VarRef> = flags.var_names.iter().map(|n| state.define(n)).collect();
            ssa.block_mut(block).insts.push(SsaInst::ReadLineMulti {
                targets,
                fd: 0,
                delim: flags.delim,
                redirects: vec![],
                clean_pipe: false,
                field_sep: flags.field_sep,
            });
        }
    }
}

/// Handle `trap` command — convert to signal instructions.
pub(super) fn handle_trap(
    name: &str,
    c: &Command,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // trap 'body' SIGNAL
    // For EXIT: record body, emit before all Return terminators
    // For signals: compile body inline, register with sash_trap
    if c.args.len() >= 2 {
        let body_str = expr_to_string(&c.args[0]);
        let signal = expr_to_string(&c.args[1]);
        if signal == "EXIT" || signal == "0" {
            handle_trap_exit(&body_str, state);
            return block;
        } else if is_ignore_reset_body(&body_str) {
            // trap '' SIG or trap - SIG → ignore/reset
            handle_trap_ignore_reset(&body_str, &signal, ssa, block);
            return block;
        }
    }
    // Unsupported trap form — emit as regular command
    let args: Vec<SsaValue> = c.args.iter().map(|a| arg_to_value(a, state)).collect();
    let resolved = resolve_pipeline_command(name);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::command(resolved, args, vec![], true));
    block
}

/// Map a signal name or numeric string to the kernel signal number.
/// Returns 0 for unsupported names.
fn signal_name_to_num(signal: &str) -> i64 {
    match signal {
        "INT" | "SIGINT" | "2" => 2,
        "TERM" | "SIGTERM" | "15" => 15,
        "HUP" | "SIGHUP" | "1" => 1,
        "PIPE" | "SIGPIPE" | "13" => 13,
        "USR1" | "SIGUSR1" | "10" => 10,
        "USR2" | "SIGUSR2" | "12" => 12,
        _ => 0,
    }
}

/// Detect the trap-body forms that mean "ignore" (`''`, `""`, empty) or
/// "reset to SIG_DFL" (`-`).
fn is_ignore_reset_body(body_str: &str) -> bool {
    body_str.is_empty() || body_str == "''" || body_str == "\"\"" || body_str == "-"
}

/// Strip a single pair of matching surrounding quotes from `s`, if any.
fn strip_outer_quotes(s: &str) -> &str {
    if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// `trap 'body' EXIT` — parse the body and store on the version state
/// for emission before every Return terminator.
fn handle_trap_exit(body_str: &str, state: &mut VersionState) {
    let body_clean = strip_outer_quotes(body_str);
    let trap_stmts = match crate::parse::parse(body_clean) {
        Ok((program, _)) => program.statements,
        Err(_) => vec![],
    };
    state.exit_trap = Some(trap_stmts);
}

/// `trap '' SIG` / `trap - SIG` — emit a TrapSignal sigil with SIG_IGN
/// (handler=1) or SIG_DFL (handler=0).
fn handle_trap_ignore_reset(body_str: &str, signal: &str, ssa: &mut SsaProgram, block: BlockId) {
    let signum = signal_name_to_num(signal);
    if signum > 0 {
        let handler = if body_str == "-" { 0i64 } else { 1i64 }; // SIG_DFL=0, SIG_IGN=1
        ssa.block_mut(block).insts.push(SsaInst::sigil(
            BuiltinCmd::TrapSignal,
            vec![
                SsaValue::Literal(signum.to_string()),
                SsaValue::Literal(handler.to_string()),
            ],
            true,
        ));
    }
}

/// Handle `signal` command — emit signal configuration instructions.
pub(super) fn handle_signal(
    _name: &str,
    c: &Command,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // signal N DISPOSITION
    // N is a signal number (int literal)
    // DISPOSITION: IGN, DFL, FLAG, EXIT
    if c.args.len() >= 2 {
        let sig_str = expr_to_string(&c.args[0]);
        let disposition = expr_to_string(&c.args[1]);
        let signum: i64 = sig_str.parse().unwrap_or(0);
        if signum > 0 {
            match disposition.as_str() {
                "IGN" => {
                    // SIG_IGN = 1
                    ssa.block_mut(block).insts.push(SsaInst::sigil(
                        BuiltinCmd::TrapSignal,
                        vec![
                            SsaValue::Literal(signum.to_string()),
                            SsaValue::Literal("1".to_string()),
                        ],
                        true,
                    ));
                }
                "DFL" => {
                    // SIG_DFL = 0
                    ssa.block_mut(block).insts.push(SsaInst::sigil(
                        BuiltinCmd::TrapSignal,
                        vec![
                            SsaValue::Literal(signum.to_string()),
                            SsaValue::Literal("0".to_string()),
                        ],
                        true,
                    ));
                }
                "FLAG" => {
                    ssa.block_mut(block).insts.push(SsaInst::sigil(
                        BuiltinCmd::SignalFlag,
                        vec![SsaValue::Literal(signum.to_string())],
                        true,
                    ));
                }
                "EXIT" => {
                    // Register FLAG handler + mark as exiting signal
                    ssa.block_mut(block).insts.push(SsaInst::sigil(
                        BuiltinCmd::SignalFlag,
                        vec![SsaValue::Literal(signum.to_string())],
                        true,
                    ));
                    state.exiting_signals.insert(signum);
                }
                _ => {}
            }
        }
    }
    block
}

/// Handle `unset` command — clear variable or array.
pub(super) fn handle_unset(
    _name: &str,
    c: &Command,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // unset map[key] → sash_table_delete
    // unset var → assign empty string (simulates unset)
    // unset -f func → ignore (function unset not supported)
    // Args may be split: ["m", "[b]"] or single "m[b]"
    let args_str: Vec<String> = c.args.iter().map(expr_to_string).collect();
    // Filter out flags like -v, -f, -n
    let var_args: Vec<String> = args_str
        .iter()
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect();
    // Join consecutive args to handle split "m" "[b]" → "m[b]"
    let joined = var_args.join("");
    // Split on whitespace-before-word boundaries that aren't inside brackets
    let mut items: Vec<String> = Vec::new();
    if joined.contains('[') {
        // Single item with bracket
        items.push(joined);
    } else {
        // Multiple plain vars
        items = var_args;
    }
    for full in &items {
        if let Some(bracket) = full.find('[') {
            let arr_name = &full[..bracket];
            let key = full[bracket + 1..full.len() - 1].to_string();
            let arr_ref = state.lookup(arr_name);
            let key_val = SsaValue::Literal(key);
            ssa.block_mut(block).insts.push(SsaInst::sigil(
                BuiltinCmd::TableDelete,
                vec![SsaValue::Var(arr_ref), key_val],
                true,
            ));
        } else {
            // Plain variable unset
            if state.collections.contains(full) {
                // Array/map: assign new empty table
                let target = state.define(full);
                ssa.block_mut(block).insts.push(SsaInst::Assign {
                    target,
                    value: SsaValue::Literal("__new_table__".to_string()),
                });
            } else {
                // Scalar: assign empty string
                let target = state.define(full);
                ssa.block_mut(block).insts.push(SsaInst::Assign {
                    target,
                    value: SsaValue::Literal(String::new()),
                });
            }
        }
    }
    block
}

/// DD#60: inside a function, $@/$* expand to the caller's args ($1..$N),
/// which are statically known at the inline site. The (cloned) body is
/// rewritten so $@/"$@"/$*/"$*" in command args and for-words become the
/// explicit positional list, then lowered normally.
pub(super) fn expand_dollar_at_in_body(body: &mut [Statement], argc: usize) {
    for stmt in body.iter_mut() {
        expand_dollar_at_in_stmt(stmt, argc);
    }
}

fn expand_dollar_at_in_if(i: &mut If, argc: usize) {
    expand_dollar_at_in_body(&mut i.condition, argc);
    expand_dollar_at_in_body(&mut i.then_body, argc);
    for elif in &mut i.elif_clauses {
        expand_dollar_at_in_body(&mut elif.condition, argc);
        expand_dollar_at_in_body(&mut elif.body, argc);
    }
    if let Some(eb) = &mut i.else_body {
        expand_dollar_at_in_body(eb, argc);
    }
}

fn expand_dollar_at_in_stmt(stmt: &mut Statement, argc: usize) {
    match &mut stmt.kind {
        StmtKind::Command(c) => {
            c.args = expand_dollar_at_words(std::mem::take(&mut c.args), argc);
        }
        StmtKind::For(f) => {
            f.words = expand_dollar_at_words(std::mem::take(&mut f.words), argc);
            expand_dollar_at_in_body(&mut f.body, argc);
        }
        StmtKind::If(i) => expand_dollar_at_in_if(i, argc),
        StmtKind::While(w) => {
            expand_dollar_at_in_body(&mut w.condition, argc);
            expand_dollar_at_in_body(&mut w.body, argc);
        }
        StmtKind::Case(cs) => {
            for item in &mut cs.items {
                expand_dollar_at_in_body(&mut item.body, argc);
            }
        }
        StmtKind::Pipeline(p) => expand_dollar_at_in_body(&mut p.commands, argc),
        StmtKind::Redirected(r) => expand_dollar_at_in_stmt(&mut r.body, argc),
        StmtKind::List(l) => {
            expand_dollar_at_in_stmt(&mut l.left, argc);
            expand_dollar_at_in_stmt(&mut l.right, argc);
        }
        StmtKind::Group(g) => expand_dollar_at_in_body(&mut g.body, argc),
        StmtKind::Suppressed(inner) => expand_dollar_at_in_stmt(inner, argc),
        // Nested function definitions keep their own $@; everything else
        // has no $@-splat position.
        _ => {}
    }
}

/// Replace each `$@`/`$*` (bare or single double-quoted) word with the
/// explicit positional list `$1..$argc`. Other words pass through.
fn expand_dollar_at_words(words: Vec<Expr>, argc: usize) -> Vec<Expr> {
    let mut out = Vec::with_capacity(words.len());
    for w in words {
        if is_dollar_at_word(&w) {
            for i in 1..=argc {
                out.push(Expr {
                    kind: ExprKind::SimpleExpansion(i.to_string()),
                    span: w.span,
                });
            }
        } else {
            out.push(w);
        }
    }
    out
}

fn is_dollar_at_word(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::SimpleExpansion(v) => v == "@" || v == "*",
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => is_dollar_at_word(&parts[0]),
        _ => false,
    }
}

/// Handle function call (inline expansion) or external command emit.
///
/// If the command name matches a known function, the function body is
/// inlined at the call site with positional params remapped.
/// Otherwise, emits an SsaInst::Command.
pub(super) fn handle_function_call_or_command(
    name: &str,
    c: &Command,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let resolved_name = resolve_function_name(name, state);
    if let Some(body) = state.function_bodies.get(&resolved_name).cloned() {
        return inline_function_call(name, &body, c, ssa, state, block);
    }
    emit_external_command(name, c, ssa, state, block)
}

/// DD#18: prefer a local function (`__local__name`) when in scope and defined;
/// fall back to the global `name`.
fn resolve_function_name(name: &str, state: &VersionState) -> String {
    let local_name = format!("__local__{}", name);
    if state.local_fn_scope.contains(&local_name) && state.function_bodies.contains_key(&local_name)
    {
        local_name
    } else {
        name.to_string()
    }
}

/// Inline a known function body into the current block.
/// Saves caller's positional params + named locals, sets up the
/// callee's $1..$9 and $#, lowers the body into a fresh call block,
/// then emits a restore block that re-defines the caller's variables.
fn inline_function_call(
    name: &str,
    body: &[Statement],
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#60: inside a function, $@/$* are the caller's args, which are
    // statically known here. Rewrite the (cloned) body so $@/"$@" expand
    // to the explicit positional list $1..$N before lowering.
    let mut owned_body = body.to_vec();
    expand_dollar_at_in_body(&mut owned_body, c.args.len());
    let body = owned_body.as_slice();

    let locals = collect_local_vars(body);

    // DD#18: save local function scope before inlining
    let saved_local_fn_scope = state.local_fn_scope.clone();

    let (saved, saved_argc) = save_caller_positionals(state);
    let saved_locals = save_caller_locals(name, &locals, ssa, state, block);
    setup_callee_positionals(name, body, c, ssa, state, block);

    // Inline the function body
    let call_block = ssa.new_block(&format!("call_{}", name));
    ssa.block_mut(block).terminator = Terminator::Goto(call_block);

    // Apply function-level redirects if defined with `f() { ... } > file`
    let func_redirs = state
        .function_redirects
        .get(name)
        .cloned()
        .unwrap_or_default();
    if !func_redirs.is_empty() {
        ssa.block_mut(call_block).insts.push(SsaInst::Command {
            name: "__redirect_push".to_string(),
            kind: BuiltinCmd::RedirectPush,
            args: vec![],
            redirects: func_redirs.clone(),
            checked: true,
        });
    }

    // Set return target so `return` jumps to restore block
    let restore_block = ssa.new_block(&format!("ret_{}", name));
    let saved_return_target = state.return_target;
    state.return_target = Some(restore_block);
    let exit_block = lower_stmts(body, ssa, state, call_block);
    state.return_target = saved_return_target;

    // Pop function-level redirects before leaving
    if !func_redirs.is_empty() {
        ssa.block_mut(exit_block)
            .insts
            .push(SsaInst::sigil(BuiltinCmd::RedirectPop, vec![], true));
    }

    // Normal fall-through also goes to restore block
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

    restore_caller_state(&saved, saved_argc, &saved_locals, ssa, state, restore_block);

    // DD#18: restore local function scope
    state.local_fn_scope = saved_local_fn_scope;

    restore_block
}

fn save_caller_positionals(state: &mut VersionState) -> (Vec<(String, VarRef)>, VarRef) {
    let saved: Vec<(String, VarRef)> = (1..=9)
        .map(|i| (i.to_string(), state.lookup(&i.to_string())))
        .collect();
    let saved_argc = state.lookup("#");
    (saved, saved_argc)
}

/// Snapshot the caller's versions of each local variable into
/// `__save_<fn>_<local>` slots so the inlined body can clobber them
/// freely; restore_caller_state copies the saves back.
fn save_caller_locals(
    name: &str,
    locals: &[String],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Vec<(String, VarRef)> {
    locals
        .iter()
        .map(|lname| {
            let save_name = format!("__save_{}_{}", name, lname);
            let src = state.lookup(lname);
            let save_ref = state.define(&save_name);
            ssa.block_mut(block).insts.push(SsaInst::Assign {
                target: save_ref.clone(),
                value: SsaValue::Var(src),
            });
            (lname.to_string(), save_ref)
        })
        .collect()
}

/// Assign $1..$N from call args, clear $N+1..$9 (DD#46), and emit a
/// diagnostic if the callee reads $K where K > argc.
fn setup_callee_positionals(
    name: &str,
    body: &[Statement],
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    let argc_val = SsaValue::Literal(c.args.len().to_string());
    let argc_var = state.define("#");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: argc_var,
        value: argc_val,
    });
    for (i, arg) in c.args.iter().enumerate() {
        let val = expr_to_value(arg, state);
        let param_var = state.define(&(i + 1).to_string());
        ssa.block_mut(block).insts.push(SsaInst::Assign {
            target: param_var,
            value: val,
        });
    }
    let max_positional = collect_max_positional(body);
    let call_argc = c.args.len();
    if max_positional > call_argc {
        state.diagnostics.push(format!(
            "1:1: error: function '{}' reads ${}  but called with only {} argument{}",
            name,
            max_positional,
            call_argc,
            if call_argc == 1 { "" } else { "s" }
        ));
    }
    for i in (c.args.len() + 1)..=9 {
        let param_var = state.define(&i.to_string());
        ssa.block_mut(block).insts.push(SsaInst::Assign {
            target: param_var,
            value: SsaValue::Literal(String::new()),
        });
    }
}

fn restore_caller_state(
    saved: &[(String, VarRef)],
    saved_argc: VarRef,
    saved_locals: &[(String, VarRef)],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    restore_block: BlockId,
) {
    for (param_name, saved_ref) in saved {
        let restore_var = state.define(param_name);
        ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
            target: restore_var,
            value: SsaValue::Var(saved_ref.clone()),
        });
    }
    let restore_argc = state.define("#");
    ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
        target: restore_argc,
        value: SsaValue::Var(saved_argc),
    });
    for (local_name, saved_ref) in saved_locals {
        let restore_var = state.define(local_name);
        ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
            target: restore_var,
            value: SsaValue::Var(saved_ref.clone()),
        });
    }
}

fn emit_external_command(
    name: &str,
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let args: Vec<SsaValue> = c
        .args
        .iter()
        .map(|a| eval_arg_and_check_int_capture(a, state))
        .collect();
    ssa.block_mut(block).insts.push(SsaInst::command(
        resolve_pipeline_command(name),
        args,
        Vec::new(),
        true,
    ));
    block
}

/// Lower an argument expression and emit SB4018 if it captures an
/// `-> int` function with `$( )` (DD#55).
fn eval_arg_and_check_int_capture(a: &Expr, state: &mut VersionState) -> SsaValue {
    let v = arg_to_value(a, state);
    if let SsaValue::FuncCall(fname) = &v {
        if state.function_int_typed.contains(fname.as_str()) {
            state.diagnostics.push(format!(
                "{}:{}: error [SB4018] $({}): cannot capture -> int function with $() — use $(({}))",
                a.span.line, a.span.col, fname, fname
            ));
        }
    }
    v
}
