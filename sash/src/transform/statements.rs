//! Statement lowering: IR Statement → SSA instructions.
//!
//! `lower_one` is the main dispatch — each StmtKind variant is handled.
//! SECURITY: This is where commands become SsaInst::Command with resolved names.

use super::control_flow::{flatten_elif, lower_case, lower_for, lower_if, lower_while};
use super::expressions::{expr_to_string, expr_to_value};
use super::functions::{
    collect_local_vars, collect_max_positional, collect_returns, lift_int_func_calls,
};
use super::pipelines::lower_pipe_while_read;
use super::resolution::resolve_pipeline_command;
use crate::ir::*;
use crate::ssa::*;

pub(super) fn lower_stmts(
    stmts: &[Statement],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    for stmt in stmts {
        block = lower_one(stmt, ssa, state, block);
    }
    block
}

/// Lower a single statement into SSA.
/// DD#58: Lift IntFuncCall values out of arithmetic expressions.
/// Inlines the function body, captures the return value as an int temp var.
pub(super) fn lower_one(
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            // DD#58: name collision check — variable vs function
            if state.known_functions.contains(a.name.as_str()) {
                state.diagnostics.push(format!(
                    "{}:{}: error [SB4019] variable '{}' collides with function '{}' — rename one",
                    stmt.span.line, stmt.span.col, a.name, a.name
                ));
            }
            if let Some(ref idx) = a.index {
                // Subscript assignment: arr[i]=val or map[key]=val
                let val = expr_to_value(&a.value, state);
                let idx_val = expr_to_value(idx, state);
                let is_int = !state.assoc_arrays.contains(&a.name);
                ssa.block_mut(block).insts.push(SsaInst::SubscriptAssign {
                    array: a.name.clone(),
                    index: idx_val,
                    value: val,
                    is_integer_key: is_int,
                });
            } else if a.op == crate::ir::AssignOp::Append {
                let val = expr_to_value(&a.value, state);
                match val {
                    SsaValue::IndexedArray(elems) => {
                        // arr+=(val1 val2 ...) — array append
                        for elem in elems {
                            ssa.block_mut(block).insts.push(SsaInst::ArrayAppend {
                                array: a.name.clone(),
                                value: elem,
                            });
                        }
                    }
                    other => {
                        // Check if this is an array variable
                        if state.assoc_arrays.contains(&a.name) {
                            ssa.block_mut(block).insts.push(SsaInst::ArrayAppend {
                                array: a.name.clone(),
                                value: other,
                            });
                        } else {
                            // String concatenation: var = var ++ val
                            // Check if integer-typed: += does arithmetic addition
                            let is_int = state.type_map.get(&a.name).copied()
                                == Some(crate::ir::VarType::Int);
                            if is_int {
                                let old = SsaValue::Var(state.lookup(&a.name));
                                let new_val = SsaValue::ArithBinOp {
                                    left: Box::new(old),
                                    op: ArithOp::Add,
                                    right: Box::new(other),
                                };
                                let target = state.define(&a.name);
                                ssa.block_mut(block).insts.push(SsaInst::Assign {
                                    target,
                                    value: new_val,
                                });
                            } else {
                                let old = SsaValue::Var(state.lookup(&a.name));
                                let concat = SsaValue::Concat(vec![old, other]);
                                let target = state.define(&a.name);
                                ssa.block_mut(block).insts.push(SsaInst::Assign {
                                    target,
                                    value: concat,
                                });
                            }
                        }
                    }
                }
            } else {
                let val = expr_to_value(&a.value, state);
                // Check if the value is a FuncCall that needs capture
                if let SsaValue::FuncCall(fname) = &val {
                    // DD#55: $(f) for -> int function is a type error
                    if state.function_int_typed.contains(fname.as_str()) {
                        state.diagnostics.push(format!(
                            "{}:{}: error [SB4018] $({}): cannot capture -> int function with $() — use $(({}))",
                            stmt.span.line, stmt.span.col, fname, fname
                        ));
                        return block;
                    }
                    // Non-constant function call: emit CaptureStart, inline body, CaptureEnd
                    if let Some(body) = state.function_bodies.get(fname).cloned() {
                        let locals = collect_local_vars(&body);
                        // CaptureStart
                        ssa.block_mut(block).insts.push(SsaInst::CaptureStart);
                        // Save/set positional params (same as direct call)
                        let saved: Vec<(String, VarRef)> = (1..=9)
                            .map(|i| (i.to_string(), state.lookup(&i.to_string())))
                            .collect();
                        let saved_argc = state.lookup("#");
                        let saved_locals: Vec<(String, VarRef)> = locals
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
                            .collect();
                        // We need to get the args from the original CommandSubstitution
                        // The args were lost when expr_to_value returned FuncCall(name)
                        // Re-extract them from the expression
                        let func_args = match &a.value.kind {
                            ExprKind::CommandSubstitution(stmts) if stmts.len() == 1 => {
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
                            _ => vec![],
                        };
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
                        // DD#46: clear unbound positional slots so caller's
                        // $K doesn't leak through inside the inlined body.
                        for i in (func_args.len() + 1)..=9 {
                            let param_var = state.define(&i.to_string());
                            ssa.block_mut(block).insts.push(SsaInst::Assign {
                                target: param_var,
                                value: SsaValue::Literal(String::new()),
                            });
                        }
                        // Inline function body
                        let call_block = ssa.new_block(&format!("cap_{}", fname));
                        ssa.block_mut(block).terminator = Terminator::Goto(call_block);
                        let restore_block = ssa.new_block(&format!("capret_{}", fname));
                        let saved_return_target = state.return_target;
                        state.return_target = Some(restore_block);
                        let exit_block = lower_stmts(&body, ssa, state, call_block);
                        state.return_target = saved_return_target;
                        ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);
                        // CaptureEnd — assign captured output to target
                        let target = state.define(&a.name);
                        ssa.block_mut(restore_block)
                            .insts
                            .push(SsaInst::CaptureEnd { target });
                        // Restore params
                        for (param_name, saved_ref) in &saved {
                            let rv = state.define(param_name);
                            ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
                                target: rv,
                                value: SsaValue::Var(saved_ref.clone()),
                            });
                        }
                        let ra = state.define("#");
                        ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
                            target: ra,
                            value: SsaValue::Var(saved_argc),
                        });
                        for (local_name, saved_ref) in &saved_locals {
                            let rv = state.define(local_name);
                            ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
                                target: rv,
                                value: SsaValue::Var(saved_ref.clone()),
                            });
                        }
                        return restore_block;
                    }
                }
                // DD#58: lift IntFuncCall out of arithmetic expressions
                let val = lift_int_func_calls(val, ssa, state, &mut block);
                let target = state.define(&a.name);
                ssa.block_mut(block)
                    .insts
                    .push(SsaInst::Assign { target, value: val });
            }
            block
        }

        StmtKind::Declaration(d) => {
            let is_array = d.flags.contains(&"-a".to_string());
            let is_assoc = d.flags.contains(&"-A".to_string());
            for a in &d.assignments {
                // Bare `declare var` (no =value) — don't reset existing variable
                let is_bare = matches!(&a.value.kind, ExprKind::Literal(s) if s.is_empty())
                    && !is_array
                    && !is_assoc;
                if is_bare && state.lookup(&a.name).version > 0 {
                    // Variable already defined — bare declare is a no-op
                    continue;
                }
                let val =
                    if a.value.kind == ExprKind::Literal(String::new()) && (is_array || is_assoc) {
                        // Bare declaration like `array arr` or `map m` — empty table
                        if is_assoc {
                            state.assoc_arrays.insert(a.name.clone());
                            state.collections.insert(a.name.clone());
                            SsaValue::AssocArray(vec![])
                        } else {
                            state.collections.insert(a.name.clone());
                            SsaValue::IndexedArray(vec![])
                        }
                    } else {
                        let val = expr_to_value(&a.value, state);
                        let val = lift_int_func_calls(val, ssa, state, &mut block);
                        if matches!(&val, SsaValue::AssocArray(_)) {
                            state.assoc_arrays.insert(a.name.clone());
                        }
                        if matches!(&val, SsaValue::AssocArray(_) | SsaValue::IndexedArray(_)) {
                            state.collections.insert(a.name.clone());
                        }
                        val
                    };
                let target = state.define(&a.name);
                ssa.block_mut(block).insts.push(SsaInst::Assign {
                    target: target.clone(),
                    value: val.clone(),
                });

                if d.keyword == crate::ir::DeclKeyword::Readonly {
                    state.readonly_vars.insert(a.name.clone());
                }
            }
            block
        }

        StmtKind::Command(c) => {
            let name = expr_to_string(&c.name);

            // bless VAR [[ test_expr ]] — runtime validation
            if name == "bless" {
                // First arg is variable name, remaining args should contain [[ test_expr ]]
                if c.args.len() >= 2 {
                    let var_name = expr_to_string(&c.args[0]);
                    // Look for a TestExpr in the remaining args
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
                // If we couldn't find a TestExpr, emit as regular command (will be caught by taint checker)
                let args: Vec<SsaValue> = c.args.iter().map(|a| {
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
                }).collect();
                ssa.block_mut(block).insts.push(SsaInst::Command {
                    name: name.clone(),
                    args,
                    redirects: vec![],
                    checked: true,
                });
                return block;
            }

            // If the command is [[ ]], preserve the TestExpr for codegen
            if name == "[[" {
                for arg in &c.args {
                    if let ExprKind::Test(test_expr) = &arg.kind {
                        ssa.block_mut(block)
                            .insts
                            .push(SsaInst::Test(test_expr.clone()));
                        return block;
                    }
                }
            }

            // var =~ s/pattern/replacement/flags — regex mutation
            if c.args.len() == 1 {
                let arg_s = expr_to_string(&c.args[0]);
                if let Some(after_prefix) = arg_s.strip_prefix("s/") {
                    // Parse s/pattern/replacement/flags
                    let parts: Vec<&str> = after_prefix.splitn(3, '/').collect();
                    if parts.len() >= 2 {
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
                                return block;
                            }
                        }

                        let global = if flags.contains('g') { "1" } else { "0" };
                        let var_ref = state.lookup(&name);
                        let new_ref = state.define(&name);
                        ssa.block_mut(block).insts.push(SsaInst::Assign {
                            target: new_ref,
                            value: SsaValue::ParamExpansion {
                                var: var_ref,
                                operator: "regex_replace".to_string(),
                                operand: format!("{}\x00{}\x00{}", pattern, replacement, global),
                            },
                        });
                        return block;
                    }
                }
            }

            // DD#48: shift — remap positional parameters
            if name == "shift" {
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
                return block;
            }

            // break → Goto(exit_block), continue → Goto(header_block)
            if name == "break" {
                let n = c
                    .args
                    .first()
                    .map(|a| expr_to_string(a).parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let depth = state.loop_stack.len();
                if n > 0 && depth > 0 {
                    let level = n.min(depth); // clamp to available depth
                    let (_header, exit) = state.loop_stack[depth - level];
                    ssa.block_mut(block).terminator = Terminator::Goto(exit);
                    let dead = ssa.new_block("after_break");
                    return dead;
                }
                return block;
            }
            if name == "continue" {
                let n = c
                    .args
                    .first()
                    .map(|a| expr_to_string(a).parse::<usize>().unwrap_or(1))
                    .unwrap_or(1);
                let depth = state.loop_stack.len();
                if n > 0 && depth > 0 {
                    let level = n.min(depth); // clamp to available depth
                    let (header, _exit) = state.loop_stack[depth - level];
                    ssa.block_mut(block).terminator = Terminator::Goto(header);
                    let dead = ssa.new_block("after_continue");
                    return dead;
                }
                return block;
            }
            if name == "return" {
                // return N — set $? to N (default 0), jump to function exit
                let exit_val = if let Some(arg) = c.args.first() {
                    let val = expr_to_value(arg, state);
                    lift_int_func_calls(val, ssa, state, &mut block)
                } else {
                    SsaValue::Literal("0".to_string())
                };
                // Set exit status variable
                let es_var = state.define("?");
                ssa.block_mut(block).insts.push(SsaInst::Assign {
                    target: es_var,
                    value: exit_val,
                });
                if let Some(target) = state.return_target {
                    ssa.block_mut(block).terminator = Terminator::Goto(target);
                } else {
                    // Top-level return acts like exit
                    ssa.block_mut(block).terminator = Terminator::Return;
                }
                let dead = ssa.new_block("after_return");
                return dead;
            }

            // `mapfile -t arr` / `readarray -t arr` → Mapfile instruction
            // `mapfile -m [-f SEP] map` → Mapfile in map mode
            if name == "mapfile" || name == "readarray" {
                let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
                let mut map_mode = false;
                let mut separator = '=';
                let mut i = 0;
                while i < args.len() {
                    if args[i] == "-m" {
                        map_mode = true;
                    } else if args[i].starts_with("-f") {
                        // -f: or -f = or -f<char>
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
                if let Some(arr_name) = var_names.first() {
                    ssa.block_mut(block).insts.push(SsaInst::Mapfile {
                        array: arr_name.to_string(),
                        fd: 0,
                        map_mode,
                        separator,
                    });
                    return block;
                }
            }

            // `read -r var` → ReadLine instruction (sets test_result for while-read)
            if name == "signal" {
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
                                ssa.block_mut(block).insts.push(SsaInst::Command {
                                    name: "__trap_signal".to_string(),
                                    args: vec![
                                        SsaValue::Literal(signum.to_string()),
                                        SsaValue::Literal("1".to_string()),
                                    ],
                                    redirects: vec![],
                                    checked: true,
                                });
                            }
                            "DFL" => {
                                // SIG_DFL = 0
                                ssa.block_mut(block).insts.push(SsaInst::Command {
                                    name: "__trap_signal".to_string(),
                                    args: vec![
                                        SsaValue::Literal(signum.to_string()),
                                        SsaValue::Literal("0".to_string()),
                                    ],
                                    redirects: vec![],
                                    checked: true,
                                });
                            }
                            "FLAG" => {
                                ssa.block_mut(block).insts.push(SsaInst::Command {
                                    name: "__signal_flag".to_string(),
                                    args: vec![SsaValue::Literal(signum.to_string())],
                                    redirects: vec![],
                                    checked: true,
                                });
                            }
                            "EXIT" => {
                                // Register FLAG handler + mark as exiting signal
                                ssa.block_mut(block).insts.push(SsaInst::Command {
                                    name: "__signal_flag".to_string(),
                                    args: vec![SsaValue::Literal(signum.to_string())],
                                    redirects: vec![],
                                    checked: true,
                                });
                                state.exiting_signals.insert(signum);
                            }
                            _ => {}
                        }
                    }
                }
                return block;
            }

            if name == "trap" {
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
                        let mut parser = tree_sitter::Parser::new();
                        parser
                            .set_language(&tree_sitter_bash::LANGUAGE.into())
                            .unwrap();
                        let tree = parser.parse(body_clean.as_bytes(), None).unwrap();
                        let trap_stmts =
                            crate::lower::lower_body(tree.root_node(), body_clean.as_bytes());
                        state.exit_trap = Some(trap_stmts);
                        return block;
                    } else if body_str.is_empty()
                        || body_str == "''"
                        || body_str == "\"\""
                        || body_str == "-"
                    {
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
                            ssa.block_mut(block).insts.push(SsaInst::Command {
                                name: "__trap_signal".to_string(),
                                args: vec![
                                    SsaValue::Literal(signum.to_string()),
                                    SsaValue::Literal(handler.to_string()),
                                ],
                                redirects: vec![],
                                checked: true,
                            });
                        }
                        return block;
                    }
                }
                // Unsupported trap form — emit as regular command
                let args: Vec<SsaValue> = c.args.iter().map(|a| expr_to_value(a, state)).collect();
                let resolved = resolve_pipeline_command(&name);
                ssa.block_mut(block).insts.push(SsaInst::Command {
                    name: resolved,
                    args,
                    redirects: vec![],
                    checked: true,
                });
                return block;
            }

            if name == "let" {
                // let "expr" → evaluate arithmetic, set exit status
                // Exit status: 0 if result != 0, 1 if result == 0
                let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
                for arg in &args {
                    // Strip surrounding quotes if present
                    let expr_str = arg.trim_matches('"').trim_matches('\'').to_string();

                    // If it's an assignment (e.g. "x=10"), define the variable
                    if let Some(eq_pos) = expr_str.find('=') {
                        let var_name = &expr_str[..eq_pos];
                        let val_str = &expr_str[eq_pos + 1..];
                        if !var_name.is_empty()
                            && var_name.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            let val_expr = crate::ir::Expr {
                                kind: ExprKind::ArithmeticExpansion(val_str.to_string()),
                                span: Span { line: 0, col: 0 },
                            };
                            let val = expr_to_value(&val_expr, state);
                            let val = lift_int_func_calls(val, ssa, state, &mut block);
                            let target = state.define(var_name);
                            ssa.block_mut(block)
                                .insts
                                .push(SsaInst::Assign { target, value: val });
                            continue;
                        }
                    }

                    let expr = crate::ir::Expr {
                        kind: ExprKind::ArithmeticExpansion(expr_str),
                        span: Span { line: 0, col: 0 },
                    };
                    let val = expr_to_value(&expr, state);
                    let val = lift_int_func_calls(val, ssa, state, &mut block);
                    // We need to evaluate this and set exit status based on result
                    // For now, emit as a command that sets test_result
                    let target = state.define("__let_result");
                    let target_ref = target.clone();
                    ssa.block_mut(block)
                        .insts
                        .push(SsaInst::Assign { target, value: val });
                    // Set exit status: result != 0 → exit 0, result == 0 → exit 1
                    ssa.block_mut(block).insts.push(SsaInst::Command {
                        name: "__let_exit".to_string(),
                        args: vec![SsaValue::Var(target_ref)],
                        redirects: vec![],
                        checked: true,
                    });
                }
                return block;
            }

            if name == "unset" {
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
                        ssa.block_mut(block).insts.push(SsaInst::Command {
                            name: "__table_delete".to_string(),
                            args: vec![SsaValue::Var(arr_ref), key_val],
                            redirects: vec![],
                            checked: true,
                        });
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
                return block;
            }

            if name == "read" {
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
                    return block;
                }
            }

            // DD#55/SB4016: bare call of non-void (-> int) function is error
            if state.function_non_void.contains(&name) && state.in_condition == 0 {
                state.diagnostics.push(format!(
                    "{}:{}: error [SB4016] function '{}' returns a status — handle with if/|| or capture with $(({}))",
                    stmt.span.line, stmt.span.col, name, name
                ));
            }

            // Check if this is a known function call → inline expansion
            // DD#18: try local function first (__local__name), only if in scope
            let resolved_name = {
                let local_name = format!("__local__{}", name);
                if state.local_fn_scope.contains(&local_name)
                    && state.function_bodies.contains_key(&local_name)
                {
                    local_name
                } else {
                    name.clone()
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
                        (lname.clone(), save_ref)
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
                    .get(&name)
                    .cloned()
                    .unwrap_or_default();
                if !func_redirs.is_empty() {
                    ssa.block_mut(call_block).insts.push(SsaInst::Command {
                        name: "__redirect_push".to_string(),
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
                    ssa.block_mut(exit_block).insts.push(SsaInst::Command {
                        name: "__redirect_pop".to_string(),
                        args: vec![],
                        redirects: vec![],
                        checked: true,
                    });
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
            ssa.block_mut(block).insts.push(SsaInst::Command {
                name,
                args,
                redirects: Vec::new(),
                checked: true,
            });
            block
        }

        StmtKind::If(i) => {
            let flattened = flatten_elif(i);
            lower_if(&flattened, ssa, state, block)
        }

        StmtKind::While(w) => {
            if w.until {
                // Desugar: until cond → while ! cond
                // lower_while will emit a Test instruction; we wrap it with Not
                // by lowering normally then patching the last Test in the header
                let mut desugared = w.clone();
                desugared.until = false;
                let exit = lower_while(&desugared, ssa, state, block);
                // Find the header block (first block after current) and negate its Test
                // The header is the Goto target from current block
                if let Terminator::Goto(header_id) = &ssa.block(block).terminator {
                    let header_id = *header_id;
                    let insts = &mut ssa.block_mut(header_id).insts;
                    for inst in insts.iter_mut() {
                        if let SsaInst::Test(ref mut test) = inst {
                            *test = TestExpr::Not(Box::new(test.clone()));
                        }
                    }
                }
                exit
            } else {
                lower_while(w, ssa, state, block)
            }
        }

        StmtKind::For(f) => lower_for(f, ssa, state, block),

        StmtKind::Pipeline(p) => {
            if p.commands.len() > 1 {
                // Check if last stage is a while-read loop or mapfile
                let last = &p.commands[p.commands.len() - 1];
                let is_while_read = matches!(&last.kind, StmtKind::While(_));
                let is_mapfile = match &last.kind {
                    StmtKind::Command(c) => {
                        let n = expr_to_string(&c.name);
                        let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
                        n == "mapfile"
                            || n == "readarray"
                            || (n == "read" && args.iter().any(|a| a == "-a"))
                    }
                    _ => false,
                };
                // DD#20: lastpipe — `cmd | read -r var` runs read in main shell
                let is_lastpipe_read = match &last.kind {
                    StmtKind::Command(c) => {
                        let n = expr_to_string(&c.name);
                        let args: Vec<String> = c.args.iter().map(expr_to_string).collect();
                        n == "read" && !args.iter().any(|a| a == "-a")
                    }
                    _ => false,
                };

                if is_while_read || is_mapfile {
                    // pipe-while-read pattern: cmd1 | ... | while read -r var
                    // Emit producer stages as a pipeline that returns pipe fd
                    let mut stages = vec![];
                    for cmd in &p.commands[..p.commands.len() - 1] {
                        if let StmtKind::Command(c) = &cmd.kind {
                            let name = expr_to_string(&c.name);
                            let args: Vec<SsaValue> =
                                c.args.iter().map(|a| expr_to_value(a, state)).collect();
                            stages.push((resolve_pipeline_command(&name), args, vec![]));
                        } else if let StmtKind::Redirected(r) = &cmd.kind {
                            if let StmtKind::Command(c) = &r.body.kind {
                                let name = expr_to_string(&c.name);
                                let args: Vec<SsaValue> =
                                    c.args.iter().map(|a| expr_to_value(a, state)).collect();
                                stages.push((
                                    resolve_pipeline_command(&name),
                                    args,
                                    r.redirects.clone(),
                                ));
                            }
                        }
                    }
                    // Emit PipeIntoWhile: pipeline stages + while body
                    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
                        stages,
                        capture_read_fd: true,
                        checked: true,
                    });
                    if is_mapfile {
                        // Lower the mapfile command, then patch fd to -1
                        block = lower_one(last, ssa, state, block);
                        for bid in ssa.blocks.keys().cloned().collect::<Vec<_>>() {
                            for inst in &mut ssa.block_mut(bid).insts {
                                if let SsaInst::Mapfile { fd, .. } = inst {
                                    if *fd == 0 {
                                        *fd = -1;
                                    }
                                }
                                if let SsaInst::ReadArray { fd, .. } = inst {
                                    if *fd == 0 {
                                        *fd = -1;
                                    }
                                }
                            }
                        }
                    } else {
                        // Now lower the while-read with fd = -1 (sentinel: use pipe fd)
                        block = lower_pipe_while_read(last, ssa, state, block);
                    }
                } else if is_lastpipe_read {
                    // DD#20: lastpipe — `cmd | read -r x` runs read in main shell
                    // Emit producer stages as a pipeline with captured pipe fd,
                    // then emit ReadLine to read from the pipe in the main process.
                    let mut stages = vec![];
                    for cmd in &p.commands[..p.commands.len() - 1] {
                        if let StmtKind::Command(c) = &cmd.kind {
                            let name = expr_to_string(&c.name);
                            let args: Vec<SsaValue> =
                                c.args.iter().map(|a| expr_to_value(a, state)).collect();
                            stages.push((resolve_pipeline_command(&name), args, vec![]));
                        } else if let StmtKind::Redirected(r) = &cmd.kind {
                            if let StmtKind::Command(c) = &r.body.kind {
                                let name = expr_to_string(&c.name);
                                let args: Vec<SsaValue> =
                                    c.args.iter().map(|a| expr_to_value(a, state)).collect();
                                stages.push((
                                    resolve_pipeline_command(&name),
                                    args,
                                    r.redirects.clone(),
                                ));
                            }
                        }
                    }
                    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
                        stages,
                        capture_read_fd: true,
                        checked: true,
                    });
                    // Lower the read command with fd=-1 (pipe fd sentinel)
                    // and clean_pipe=true (producer stages are clean)
                    if let StmtKind::Command(c) = &last.kind {
                        let mut var_names = vec![];
                        let mut delim: Option<u8> = None;
                        let mut field_sep: Option<u8> = None;
                        let mut skip_next = false;
                        for arg in &c.args {
                            let s = expr_to_string(arg);
                            if skip_next {
                                // This arg is the value for the previous flag
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
                            if s.starts_with("-d") {
                                delim = Some(s.as_bytes().get(2).copied().unwrap_or(0));
                                continue;
                            }
                            if s == "-f" {
                                skip_next = true;
                                field_sep = Some(0);
                                continue;
                            }
                            if s.starts_with("-f") {
                                field_sep = Some(s.as_bytes().get(2).copied().unwrap_or(0));
                                continue;
                            }
                            if s.starts_with('-') {
                                continue;
                            }
                            var_names.push(s);
                        }
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
                            let targets: Vec<VarRef> =
                                var_names.iter().map(|n| state.define(n)).collect();
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
                } else {
                    // Regular pipeline: cmd1 | cmd2 | ...
                    // Compound commands (while, for, if) become forked children
                    let mut stages = vec![];
                    let _compound_producers: Vec<(usize, usize)> = vec![];
                    for (idx, cmd) in p.commands.iter().enumerate() {
                        // Unwrap Suppressed/Redirected to get inner command
                        let (inner, redirects) = match &cmd.kind {
                            StmtKind::Suppressed(_s) => {
                                // - prefix in pipeline: include but don't count toward pipefail
                                // For now, just skip suppressed stages entirely
                                continue;
                            }
                            StmtKind::Redirected(r) => (&r.body.kind, r.redirects.clone()),
                            other => (other, vec![]),
                        };
                        if let StmtKind::Command(c) = inner {
                            let name = expr_to_string(&c.name);
                            let args: Vec<SsaValue> =
                                c.args.iter().map(|a| expr_to_value(a, state)).collect();
                            stages.push((resolve_pipeline_command(&name), args, redirects));
                        } else {
                            // Compound command as pipeline stage:
                            // Lower into SSA blocks, record the entry block for codegen
                            let compound_entry = ssa.new_block(&format!("pipe_compound_{}", idx));
                            let compound_exit = lower_one(cmd, ssa, state, compound_entry);
                            ssa.block_mut(compound_exit).terminator = Terminator::Return;
                            ssa.compound_pipeline_blocks.push(compound_entry);
                            // Push placeholder stage — codegen will fork+run the SSA blocks
                            stages.push((format!("__compound_{}", compound_entry), vec![], vec![]));
                        }
                    }
                    ssa.block_mut(block).insts.push(SsaInst::Pipeline {
                        stages,
                        capture_read_fd: false,
                        checked: true,
                    });
                }
            } else {
                for cmd in &p.commands {
                    block = lower_one(cmd, ssa, state, block);
                }
            }
            if p.negated {
                ssa.block_mut(block).insts.push(SsaInst::Command {
                    name: "__negate_test_result".to_string(),
                    args: vec![],
                    redirects: vec![],
                    checked: true,
                });
            }
            block
        }

        StmtKind::List(l) => {
            match l.op {
                ListOp::And => {
                    // left && right: run left, skip right if left failed
                    state.in_condition += 1;
                    block = lower_one(&l.left, ssa, state, block);
                    state.in_condition -= 1;
                    let right_block = ssa.new_block("and.right");
                    let merge_block = ssa.new_block("and.merge");
                    ssa.block_mut(block).terminator = Terminator::Branch {
                        condition: block.to_string(),
                        true_block: right_block,
                        false_block: merge_block,
                    };
                    let right_end = lower_one(&l.right, ssa, state, right_block);
                    ssa.block_mut(right_end).terminator = Terminator::Goto(merge_block);
                    merge_block
                }
                ListOp::Or => {
                    // left || right: run left, skip right if left succeeded
                    state.in_condition += 1;
                    block = lower_one(&l.left, ssa, state, block);
                    state.in_condition -= 1;
                    let right_block = ssa.new_block("or.right");
                    let merge_block = ssa.new_block("or.merge");
                    ssa.block_mut(block).terminator = Terminator::Branch {
                        condition: block.to_string(),
                        true_block: merge_block,  // success → skip right
                        false_block: right_block, // failure → run right
                    };
                    let right_end = lower_one(&l.right, ssa, state, right_block);
                    // DD#55: if the right side completes, the || list succeeded;
                    // reset exit status to 0 so the program doesn't exit with
                    // the left side's failure status.
                    let zero_ref = state.define("?");
                    ssa.block_mut(right_end).insts.push(SsaInst::Assign {
                        target: zero_ref,
                        value: SsaValue::Literal("0".to_string()),
                    });
                    ssa.block_mut(right_end).terminator = Terminator::Goto(merge_block);
                    merge_block
                }
                ListOp::Background => {
                    // cmd & — fork, child runs cmd, parent continues
                    let child_block = ssa.new_block("bg.child");
                    let parent_block = ssa.new_block("bg.parent");

                    // Fork
                    ssa.block_mut(block).insts.push(SsaInst::Command {
                        name: "__subshell".to_string(),
                        args: vec![],
                        redirects: vec![],
                        checked: true,
                    });
                    ssa.block_mut(block).terminator = Terminator::Branch {
                        condition: "__fork_pid".to_string(),
                        true_block: parent_block, // parent continues
                        false_block: child_block, // child runs command
                    };

                    // Store fork pid in $! variable
                    let bg_target = state.define("!");
                    ssa.block_mut(parent_block).insts.push(SsaInst::Command {
                        name: "__store_bg_pid".to_string(),
                        args: vec![SsaValue::Var(bg_target)],
                        redirects: vec![],
                        checked: true,
                    });

                    // Child: run the command then _exit
                    let child_end = lower_one(&l.left, ssa, state, child_block);
                    ssa.block_mut(child_end).terminator = Terminator::Return;

                    parent_block
                }
            }
        }

        StmtKind::Function(f) => {
            // DD#56: END block — store body for end_block, don't emit as function
            if f.name == "__sash_end__" {
                state.exit_trap = Some(f.body.clone());
                return block;
            }

            // Store function body for inline expansion at call sites.
            state.function_bodies.insert(f.name.clone(), f.body.clone());

            // DD#18: track local function scope
            if f.name.starts_with("__local__") {
                state.local_fn_scope.insert(f.name.clone());
            }

            // Track -> int functions for DD#55 type error on $(f)
            if f.return_type == crate::ir::VarType::Int {
                state.function_int_typed.insert(f.name.clone());
            }

            // DD#55: non-void function detection
            let returns = collect_returns(&f.body);
            if !returns.is_empty() {
                // SB4017: -> int function where all literal returns are 0
                if f.return_type == crate::ir::VarType::Int {
                    let all_literal_zero = returns
                        .iter()
                        .all(|v| matches!(v, Some(s) if s.is_empty() || s == "0"));
                    if all_literal_zero {
                        state.diagnostics.push(format!(
                            "{}:{}: error [SB4017] function '{}' only returns 0 — omit the return (void function) or return a meaningful status",
                            stmt.span.line, stmt.span.col, f.name
                        ));
                    }
                }
                // -> int function with any nonzero or computed returns is non-void
                let has_nonzero = returns.iter().any(|v| match v {
                    Some(s) => !s.is_empty() && s != "0",
                    None => true, // computed value — assume non-void
                });
                if has_nonzero && f.return_type == crate::ir::VarType::Int {
                    state.function_non_void.insert(f.name.clone());
                }
            }

            let fn_entry = ssa.new_block(&format!("fn_{}", f.name));
            // DD#18: save/restore local_fn_scope around function body compilation
            let saved_scope = state.local_fn_scope.clone();
            let fn_exit = lower_stmts(&f.body, ssa, state, fn_entry);
            state.local_fn_scope = saved_scope;
            ssa.block_mut(fn_exit).terminator = Terminator::Return;
            block
        }

        StmtKind::Redirected(r) => {
            // For compound commands (for/while/if/case/group/function), the redirect
            // applies to the entire body — emit push/pop around it.
            let is_compound = matches!(
                r.body.kind,
                StmtKind::While(_)
                    | StmtKind::If(_)
                    | StmtKind::For(_)
                    | StmtKind::Case(_)
                    | StmtKind::Group(_)
                    | StmtKind::Subshell(_)
                    | StmtKind::Function { .. }
            );

            // Special case: redirected function definition — store redirects
            // for application at call sites
            if let StmtKind::Function(ref f) = r.body.kind {
                if !r.redirects.is_empty() {
                    state
                        .function_redirects
                        .insert(f.name.clone(), r.redirects.clone());
                }
                // Still lower the function def normally
                return lower_one(&r.body, ssa, state, block);
            }

            if is_compound && !r.redirects.is_empty() {
                // Emit __redirect_push with redirect metadata
                ssa.block_mut(block).insts.push(SsaInst::Command {
                    name: "__redirect_push".to_string(),
                    args: vec![],
                    redirects: r.redirects.clone(),
                    checked: true,
                });
                let result_block = lower_one(&r.body, ssa, state, block);
                ssa.block_mut(result_block).insts.push(SsaInst::Command {
                    name: "__redirect_pop".to_string(),
                    args: vec![],
                    redirects: r.redirects.clone(),
                    checked: true,
                });
                result_block
            } else {
                // Simple command — attach redirects to the instruction
                let result_block = lower_one(&r.body, ssa, state, block);
                if !r.redirects.is_empty() {
                    let blocks_to_check = if result_block == block {
                        vec![block]
                    } else {
                        vec![block, result_block]
                    };
                    for blk_id in blocks_to_check {
                        let blk = ssa.block_mut(blk_id);
                        if let Some(inst) = blk.insts.last_mut() {
                            match inst {
                                SsaInst::Command { redirects, .. } => {
                                    *redirects = r.redirects.clone();
                                }
                                SsaInst::Pipeline { stages, .. } => {
                                    if let Some(last) = stages.last_mut() {
                                        last.2 = r.redirects.clone();
                                    }
                                }
                                SsaInst::ReadLine { redirects, .. } => {
                                    *redirects = r.redirects.clone();
                                }
                                SsaInst::ReadLineMulti { redirects, .. } => {
                                    *redirects = r.redirects.clone();
                                }
                                _ => {}
                            }
                        }
                    }
                }
                result_block
            }
        }

        StmtKind::Subshell(s) => {
            // Subshell: fork child, run body in child, wait in parent.
            // Child gets its own memory space — modifications don't propagate.
            let child_block = ssa.new_block("subshell");
            let after_block = ssa.new_block("subshell_done");

            // Emit fork — codegen stores pid in test_result_var
            ssa.block_mut(block).insts.push(SsaInst::Command {
                name: "__subshell".to_string(),
                args: vec![],
                redirects: vec![],
                checked: true,
            });
            // Branch: parent (pid>0) → after, child (pid==0) → body
            ssa.block_mut(block).terminator = Terminator::Branch {
                condition: "__fork_pid".to_string(),
                true_block: after_block,
                false_block: child_block,
            };

            // Parent: wait for child at start of after_block
            ssa.block_mut(after_block).insts.push(SsaInst::Command {
                name: "__subshell_wait".to_string(),
                args: vec![],
                redirects: vec![],
                checked: true,
            });

            // Snapshot SSA state — child modifications don't affect parent
            let snap = state.snapshot();

            // Child: run body then _exit(0)
            let body_exit = lower_stmts(&s.body, ssa, state, child_block);
            ssa.block_mut(body_exit).terminator = Terminator::Return;

            // Restore parent's SSA versions
            state.restore(&snap);

            after_block
        }

        StmtKind::Group(g) => lower_stmts(&g.body, ssa, state, block),

        StmtKind::Case(c) => lower_case(c, ssa, state, block),

        StmtKind::Comment(_) => block,

        StmtKind::Suppressed(inner) => {
            // DD#55: lower the inner statement, then mark the last
            // Command/Pipeline instruction as unchecked.
            let pre_len = ssa.block(block).insts.len();
            let result_block = lower_one(inner, ssa, state, block);
            // Walk backwards through emitted instructions to uncheck commands/pipelines
            fn uncheck_block(ssa: &mut SsaProgram, block_id: BlockId, from: usize) {
                let insts = &mut ssa.block_mut(block_id).insts;
                for inst in insts[from..].iter_mut() {
                    match inst {
                        SsaInst::Command { checked, .. } => {
                            *checked = false;
                        }
                        SsaInst::Pipeline { checked, .. } => {
                            *checked = false;
                        }
                        _ => {}
                    }
                }
            }
            uncheck_block(ssa, block, pre_len);
            // If lowering created new blocks, uncheck those too
            if result_block != block {
                for b in (block + 1)..=result_block {
                    uncheck_block(ssa, b, 0);
                }
            }
            result_block
        }
    }
}
