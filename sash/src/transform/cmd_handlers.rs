//! Command-specific handlers extracted from handle_command.

use super::expressions::{expr_to_string, expr_to_value};
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
    // Parse flags: -d DELIM (record delimiter), -f DELIM (field separator), -a ARR
    let mut delim: Option<u8> = None;
    let mut field_sep: Option<u8> = None;
    let mut array_name: Option<String> = None;
    let mut skip_next = false;
    let mut var_names: Vec<&String> = Vec::new();
    for (i, a) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if a == "-d" || a.starts_with("-d") {
            if a.len() > 2 {
                delim = Some(a.as_bytes()[2]);
            } else if i + 1 < args.len() {
                delim = Some(args[i + 1].as_bytes()[0]);
                skip_next = true;
            }
            continue;
        }
        if a == "-f" || a.starts_with("-f") {
            if a.len() > 2 {
                field_sep = Some(a.as_bytes()[2]);
            } else if i + 1 < args.len() {
                field_sep = Some(args[i + 1].as_bytes()[0]);
                skip_next = true;
            }
            continue;
        }
        if a == "-a" {
            if i + 1 < args.len() {
                array_name = Some(args[i + 1].clone());
                skip_next = true;
            }
            continue;
        }
        if a.starts_with('-') {
            // Reject unknown flags (including -r, -t)
            state.diagnostics.push(format!(
                "{}:{}: error [SB5015] read: unknown flag '{}'",
                stmt.span.line, stmt.span.col, a
            ));
            return block;
        }
        var_names.push(a);
    }
    // Reject -f with single variable
    if field_sep.is_some() && var_names.len() <= 1 && array_name.is_none() {
        state.diagnostics.push(format!(
            "{}:{}: error [SB5016] read -f: field separator requires multiple target variables",
            stmt.span.line, stmt.span.col
        ));
        return block;
    }
    // read -a arr: split line into array
    if let Some(arr) = array_name {
        ssa.block_mut(block).insts.push(SsaInst::ReadArray {
            array: arr,
            fd: 0,
            delim,
        });
        return block;
    }
    if var_names.len() == 1 {
        let target = state.define(var_names[0]);
        ssa.block_mut(block).insts.push(SsaInst::ReadLine {
            target,
            fd: 0,
            delim,
            redirects: vec![],
            clean_pipe: false,
        });
        return block;
    } else if var_names.len() > 1 {
        let targets: Vec<VarRef> = var_names.iter().map(|n| state.define(n)).collect();
        ssa.block_mut(block).insts.push(SsaInst::ReadLineMulti {
            targets,
            fd: 0,
            delim,
            redirects: vec![],
            clean_pipe: false,
            field_sep,
        });
    }
    block
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
            // Parse the trap body as bash source and store for later
            // Strip surrounding quotes if present
            let body_clean = if (body_str.starts_with('\'') && body_str.ends_with('\''))
                || (body_str.starts_with('"') && body_str.ends_with('"'))
            {
                &body_str[1..body_str.len() - 1]
            } else {
                &body_str
            };
            let trap_stmts = match crate::parse::parse(body_clean) {
                Ok((program, _)) => program.statements,
                Err(_) => vec![],
            };
            state.exit_trap = Some(trap_stmts);
            return block;
        } else if body_str.is_empty() || body_str == "''" || body_str == "\"\"" || body_str == "-" {
            // trap '' SIG or trap - SIG → ignore/reset
            let signum = match signal.as_str() {
                "INT" | "SIGINT" | "2" => 2,
                "TERM" | "SIGTERM" | "15" => 15,
                "HUP" | "SIGHUP" | "1" => 1,
                "PIPE" | "SIGPIPE" | "13" => 13,
                "USR1" | "SIGUSR1" | "10" => 10,
                "USR2" | "SIGUSR2" | "12" => 12,
                _ => 0,
            };
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
            return block;
        }
    }
    // Unsupported trap form — emit as regular command
    let args: Vec<SsaValue> = c.args.iter().map(|a| expr_to_value(a, state)).collect();
    let resolved = resolve_pipeline_command(name);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::command(resolved, args, vec![], true));
    block
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
// cyclo-allow: function inlining; linear plumbing
pub(super) fn handle_function_call_or_command(
    name: &str,
    c: &Command,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // Check if this is a known function call → inline expansion
    // DD#18: try local function first (__local__name), only if in scope
    let resolved_name = {
        let local_name = format!("__local__{}", name);
        if state.local_fn_scope.contains(&local_name)
            && state.function_bodies.contains_key(&local_name)
        {
            local_name
        } else {
            name.to_string()
        }
    };
    if let Some(body) = state.function_bodies.get(&resolved_name).cloned() {
        // Collect local variable names from function body

        let locals = collect_local_vars(&body);

        // DD#18: save local function scope before inlining
        let saved_local_fn_scope = state.local_fn_scope.clone();

        // Save caller's positional params
        let saved: Vec<(String, VarRef)> = (1..=9)
            .map(|i| (i.to_string(), state.lookup(&i.to_string())))
            .collect();
        let saved_argc = state.lookup("#");

        // Save caller's versions of local variables to temp names
        // (different base name so they get separate CL Variables)
        let saved_locals: Vec<(String, VarRef)> = locals
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
            .collect();

        // Set positional params from call args
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
        // DD#46: clear unbound positional slots so caller's $K
        // doesn't leak through when the callee reads $K+1..$9.
        // Also: detect out-of-range positional reads in the body
        // and emit a diagnostic if the function reads $K where K > argc.
        let max_positional = collect_max_positional(&body);
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
        let exit_block = lower_stmts(&body, ssa, state, call_block);
        state.return_target = saved_return_target;

        // Pop function-level redirects before leaving
        if !func_redirs.is_empty() {
            ssa.block_mut(exit_block).insts.push(SsaInst::sigil(
                BuiltinCmd::RedirectPop,
                vec![],
                true,
            ));
        }

        // Normal fall-through also goes to restore block
        ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

        for (param_name, saved_ref) in &saved {
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
        for (local_name, saved_ref) in &saved_locals {
            let restore_var = state.define(local_name);
            ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
                target: restore_var,
                value: SsaValue::Var(saved_ref.clone()),
            });
        }

        // DD#18: restore local function scope
        state.local_fn_scope = saved_local_fn_scope;

        return restore_block;
    }

    let args: Vec<SsaValue> = c.args.iter()
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
    ssa.block_mut(block).insts.push(SsaInst::command(
        resolve_pipeline_command(name),
        args,
        Vec::new(),
        true,
    ));
    block
}
