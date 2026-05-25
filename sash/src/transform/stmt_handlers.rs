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
        return super::cmd_handlers::handle_signal(&name, c, stmt, ssa, state, block);
    }
    if name == "trap" {
        return super::cmd_handlers::handle_trap(&name, c, stmt, ssa, state, block);
    }

    if name == "unset" {
        return super::cmd_handlers::handle_unset(&name, c, stmt, ssa, state, block);
    }
    if name == "read" {
        return super::cmd_handlers::handle_read(&name, c, stmt, ssa, state, block);
    }
    // DD#55/SB4016: bare call of non-void (-> int) function is error
    if state.function_non_void.contains(&name) && state.in_condition == 0 {
        state.diagnostics.push(format!(
            "{}:{}: error [SB4016] function '{}' returns a status — handle with if/|| or capture with $(({}))",
            stmt.span.line, stmt.span.col, name, name
        ));
    }
    super::cmd_handlers::handle_function_call_or_command(&name, c, stmt, ssa, state, block)
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
                        stages.push((resolve_pipeline_command(&name), args, r.redirects.clone()));
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
                        stages.push((resolve_pipeline_command(&name), args, r.redirects.clone()));
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

/// Handle StmtKind::Assignment — variable assignment with optional function collision check.
pub(super) fn handle_assignment(
    a: &Assignment,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
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
                    let is_int =
                        state.type_map.get(&a.name).copied() == Some(crate::ir::VarType::Int);
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
