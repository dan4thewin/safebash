//! Command-specific handlers extracted from handle_command.

use super::expressions::{arg_to_value, expr_to_string, expr_to_value};
use super::functions::{collect_local_vars, lift_int_func_args};
use super::resolution::resolve_pipeline_command;
use super::statements::lower_stmts;
use crate::ir::*;
use crate::ssa::*;

/// Handle `read` command — emit ReadLine/ReadLineMulti instructions.
pub(super) fn handle_read(
    name: &str,
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
    // DD#86: `readc` is the cooked read (honors `\` escapes); `read` is raw.
    let cooked = name == "readc";
    emit_read_inst(&flags, cooked, ssa, state, block);
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
/// `cooked` (DD#86 `readc`) honors backslash escapes in the value/split.
fn emit_read_inst(
    flags: &ReadFlags,
    cooked: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    if let Some(arr) = &flags.array_name {
        // Auto-declare (mirrors argv/mapfile) so a later len/element read on an
        // undeclared `read -a a` target resolves instead of an ICE.
        super::stmt_handlers::autodeclare_bulk_target(arr, false, ssa, state, block);
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
                cooked,
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
                cooked,
            });
        }
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
    // DD#81: this bareword resolved to an external command (no function defined
    // so far) — record it so a later `fn NAME` is flagged as a collision. Skip
    // records made while lowering a dead `fn_` template (not real execution).
    if !state.lowering_fn_template {
        state.externally_used.borrow_mut().insert(name.to_string());
    }
    emit_external_command(name, c, ssa, state, block)
}

/// DD#18: prefer a local function (`__local__name`) when in scope and defined;
/// fall back to the global `name`.
pub(super) fn resolve_function_name(name: &str, state: &VersionState) -> String {
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
    // DD#27/§5: reject a call to a function already being inlined (recursion) —
    // unbounded inlining would overflow the compiler. Tail self-calls are
    // handled before reaching here. Emit the diagnostic and skip the inline.
    if state.inlining.iter().any(|n| n == name) {
        state
            .diagnostics
            .push(super::functions::recursion_diagnostic(name));
        return block;
    }
    state.inlining.push(name.to_string());

    let locals = collect_local_vars(body);

    // DD#18: save local function scope before inlining
    let saved_local_fn_scope = state.local_fn_scope.clone();

    // Lower the call arguments ONCE in the caller's scope (before `args` is
    // redefined), so a side-effecting `$( … )` arg is not executed twice and
    // args reading the caller's positionals see the caller's values.
    let arg_values: Vec<SsaValue> = c.args.iter().map(|a| expr_to_value(a, state)).collect();

    check_callee_arity(name, body, arg_values.len(), state);
    let saved_args = super::functions::save_caller_args(state);
    let saved_locals = save_caller_locals(name, &locals, ssa, state, block);
    super::functions::setup_callee_args(&arg_values, ssa, state, block);

    // Inline the function body
    let call_block = ssa.new_block(&format!("call_{}", name));
    ssa.block_mut(block).terminator = Terminator::Goto(call_block);

    // Set return target so `return` jumps to restore block
    let restore_block = ssa.new_block(&format!("ret_{}", name));
    let saved_return_target = state.return_target;
    state.return_target = Some(restore_block);
    // DD#27/§5: a self tail-call (`return F …`) in the body loops to call_block.
    let args_name = state.current_args_name.clone();
    let saved_tail = state.tail_loop.take();
    state.tail_loop = Some((name.to_string(), call_block, args_name));
    let exit_block = lower_stmts(body, ssa, state, call_block);
    state.tail_loop = saved_tail;
    state.return_target = saved_return_target;

    // Normal fall-through also goes to restore block
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

    restore_caller_state(&saved_locals, ssa, state, restore_block);
    super::functions::restore_caller_args(saved_args, state);

    // DD#18: restore local function scope
    state.local_fn_scope = saved_local_fn_scope;

    state.inlining.pop();
    restore_block
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

/// DD#46/#48: emit a diagnostic if the callee reads `$K` (accounting for
/// `shift`) where K exceeds the number of arguments actually passed. The
/// positional values themselves live in the `args` array (setup_callee_args).
pub(super) fn check_callee_arity(
    name: &str,
    body: &[Statement],
    call_argc: usize,
    state: &mut VersionState,
) {
    let (min, max) = match super::functions::analyze_arity(body).0 {
        super::functions::Arity::Fixed { min, max } => (min, Some(max)),
        super::functions::Arity::Variadic { floor } => (floor, None),
    };
    if call_argc < min {
        state
            .diagnostics
            .push(arity_too_few_msg(name, min, max.is_none(), call_argc));
    } else if max.is_some_and(|m| call_argc > m) {
        state
            .diagnostics
            .push(arity_too_many_msg(name, min, call_argc));
    }
}

fn arity_plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn arity_too_few_msg(name: &str, min: usize, variadic: bool, got: usize) -> String {
    let atleast = if variadic { "at least " } else { "" };
    format!(
        "1:1: error [SB5042] function '{}' expects {}{} argument{} but was called with {}",
        name,
        atleast,
        min,
        arity_plural(min),
        got
    )
}

fn arity_too_many_msg(name: &str, n: usize, got: usize) -> String {
    format!(
        "1:1: error [SB5043] function '{}' takes {} argument{} but was called with {} — use @args to accept a variable number",
        name, n, arity_plural(n), got
    )
}

fn restore_caller_state(
    saved_locals: &[(String, VarRef)],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    restore_block: BlockId,
) {
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
    mut block: BlockId,
) -> BlockId {
    let raw: Vec<SsaValue> = c.args.iter().map(|a| arg_to_value(a, state)).collect();
    // Option C: `$(f)` / `$((f))` on an `-> int` function is an IntFuncCall;
    // materialize it into a temp so the argv is scalar (unlifted → codegen ICE).
    let args = lift_int_func_args(raw, ssa, state, &mut block);
    // DD#14: `exec CMD …` replaces the running program — resolve the target
    // (args[0]) to its external absolute path so codegen's `sash_exec` (which
    // rejects bare names) gets a real path. Unlike a normal command head, exec
    // must not keep `echo`/`:` inline; it runs the external program. The
    // fd-only form (`exec > file`) has no args and is unaffected.
    let args = if name == "exec" {
        resolve_exec_target_arg(args, state)
    } else {
        args
    };
    ssa.block_mut(block).insts.push(SsaInst::command(
        resolve_pipeline_command(name, state),
        args,
        Vec::new(),
        true,
    ));
    block
}

/// Rewrite an `exec` target (args[0]) to its resolved external absolute path
/// and record it as an external dependency (for `--manifest`). A target that
/// already contains `/` or is not a literal (the latter is rejected at
/// validation) is left untouched.
fn resolve_exec_target_arg(mut args: Vec<SsaValue>, state: &VersionState) -> Vec<SsaValue> {
    let target = match args.first() {
        Some(SsaValue::Literal(s)) if !s.contains('/') => s.clone(),
        _ => return args,
    };
    if let Some(path) = crate::resolve::resolve_command_path(&target) {
        state.external_deps.borrow_mut().insert(path.clone());
        args[0] = SsaValue::Literal(path);
    }
    args
}
