/// IR → SSA transformation.
///
/// Converts the safebash IR into SSA form:
///   1. Build basic blocks from IR statements
///   2. Version variable assignments
///   3. Insert phi nodes at if/else and loop merge points

use crate::ir::*;
use crate::ssa::*;
use std::collections::HashMap;

/// Transform an IR Program into SSA form.
pub fn ir_to_ssa(program: &Program) -> (SsaProgram, Vec<String>) {
    ir_to_ssa_typed(program, HashMap::new())
}

/// Transform an IR Program into SSA form with type information.
pub fn ir_to_ssa_typed(program: &Program, type_map: HashMap<String, VarType>) -> (SsaProgram, Vec<String>) {
    let mut ssa = SsaProgram::new();
    let mut state = VersionState::with_types(type_map);

    // First pass: collect function names so CommandSubstitution
    // can detect elidable function calls.
    let mut known_functions: std::collections::HashSet<String> = std::collections::HashSet::new();
    for stmt in &program.statements {
        if let StmtKind::Function(f) = &stmt.kind {
            known_functions.insert(f.name.clone());
        }
    }
    state.known_functions = known_functions;

    let entry = ssa.new_block("entry");
    ssa.entry = entry;

    let exit = lower_stmts(&program.statements, &mut ssa, &mut state, entry);

    // Ensure exit block terminates with Return before END rewrite
    ssa.block_mut(exit).terminator = Terminator::Return;

    // DD#56: Create a dedicated end_block for the END body.
    // All exit paths (normal, abort, explicit exit) goto end_block.
    // This ensures END always fires regardless of how the program exits.
    if let Some(trap_stmts) = state.exit_trap.take() {
        let end_block = ssa.new_block("__end__");
        let end_exit = lower_stmts(&trap_stmts, &mut ssa, &mut state, end_block);
        ssa.block_mut(end_exit).terminator = Terminator::Return;

        // Redirect all non-function Return terminators to goto end_block
        let return_blocks: Vec<usize> = ssa.blocks.iter()
            .filter(|(_, b)| b.terminator == Terminator::Return && !b.label.starts_with("fn_")
                    && !b.label.starts_with("__end__"))
            .map(|(&id, _)| id)
            .collect();
        for block_id in return_blocks {
            if block_id != end_block && block_id != end_exit {
                ssa.block_mut(block_id).terminator = Terminator::Goto(end_block);
            }
        }

        // Store end_block id for codegen abort paths
        ssa.end_block = Some(end_block);
    }

    // Copy exiting signals to program for codegen check point insertion
    ssa.exiting_signals = state.exiting_signals.clone();

    // DD#55: Mark commands/pipelines in condition position as unchecked.
    for block in ssa.blocks.values_mut() {
        if matches!(block.terminator, Terminator::Branch { .. }) {
            for inst in block.insts.iter_mut().rev() {
                match inst {
                    SsaInst::Command { checked, .. } => { *checked = false; break; }
                    SsaInst::Pipeline { checked, .. } => { *checked = false; break; }
                    _ => {}
                }
            }
        }
        // DD#56: Commands in END block are unchecked (cleanup must not abort)
        if block.label.starts_with("__end__") || block.label.contains("__end__") {
            for inst in block.insts.iter_mut() {
                match inst {
                    SsaInst::Command { checked, .. } => { *checked = false; }
                    SsaInst::Pipeline { checked, .. } => { *checked = false; }
                    _ => {}
                }
            }
        }
    }


    let diags = state.diagnostics.clone();
    (ssa, diags)
}

/// Lower a sequence of statements into SSA, starting from `block`.
/// Returns the block ID where execution continues after the sequence.
fn lower_stmts(
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
fn lift_int_func_calls(
    val: SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    match val {
        SsaValue::IntFuncCall { name, args } => {
            // Inline the function and capture return value
            let lifted_args: Vec<SsaValue> = args.into_iter()
                .map(|a| lift_int_func_calls(a, ssa, state, block))
                .collect();
            inline_int_func(&name, &lifted_args, ssa, state, block)
        }
        SsaValue::ArithBinOp { left, op, right } => {
            let l = lift_int_func_calls(*left, ssa, state, block);
            let r = lift_int_func_calls(*right, ssa, state, block);
            SsaValue::ArithBinOp { left: Box::new(l), op, right: Box::new(r) }
        }
        SsaValue::Ternary { cond, true_val, false_val } => {
            let c = lift_int_func_calls(*cond, ssa, state, block);
            let t = lift_int_func_calls(*true_val, ssa, state, block);
            let f = lift_int_func_calls(*false_val, ssa, state, block);
            SsaValue::Ternary { cond: Box::new(c), true_val: Box::new(t), false_val: Box::new(f) }
        }
        other => other, // no IntFuncCall inside
    }
}

/// Inline a -> int function call, returning an SsaValue::Var pointing
/// to the temp variable holding the return value.
fn inline_int_func(
    name: &str,
    args: &[SsaValue],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    let body = match state.function_bodies.get(name).cloned() {
        Some(b) => b,
        None => return SsaValue::Expr("0".to_string()),
    };

    let locals = collect_local_vars(&body);

    // Save positional params
    let saved: Vec<(String, VarRef)> = (1..=9)
        .map(|i| (i.to_string(), state.lookup(&i.to_string())))
        .collect();
    let saved_argc = state.lookup("#");

    // Save local vars
    let saved_locals: Vec<(String, VarRef)> = locals.iter()
        .map(|lname| {
            let save_ref = state.lookup(lname);
            (lname.clone(), save_ref)
        })
        .collect();

    // Set argc
    let argc_val = SsaValue::Literal(args.len().to_string());
    let argc_var = state.define("#");
    ssa.block_mut(*block).insts.push(SsaInst::Assign { target: argc_var, value: argc_val });

    // Set positional params from args
    for (i, arg) in args.iter().enumerate() {
        let param_var = state.define(&(i + 1).to_string());
        ssa.block_mut(*block).insts.push(SsaInst::Assign { target: param_var, value: arg.clone() });
    }
    // Clear unbound positionals
    for i in (args.len() + 1)..=9 {
        let param_var = state.define(&i.to_string());
        ssa.block_mut(*block).insts.push(SsaInst::Assign {
            target: param_var,
            value: SsaValue::Literal(String::new()),
        });
    }

    // Save exit status
    let saved_status = state.lookup("?");

    // Inline function body
    let call_block = ssa.new_block(&format!("intcall_{}", name));
    ssa.block_mut(*block).terminator = Terminator::Goto(call_block);
    let restore_block = ssa.new_block(&format!("intret_{}", name));
    let saved_return_target = state.return_target;
    state.return_target = Some(restore_block);
    let exit_block = lower_stmts(&body, ssa, state, call_block);
    state.return_target = saved_return_target;
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

    // The return value is in "?" (set by `return N`)
    let result_ref = state.lookup("?");
    let temp_name = format!("__intret_{}", name);
    let temp_var = state.define(&temp_name);
    ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
        target: temp_var.clone(),
        value: SsaValue::Var(result_ref),
    });

    // Restore positional params
    for (pname, saved_ref) in &saved {
        let rv = state.define(pname);
        ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
            target: rv,
            value: SsaValue::Var(saved_ref.clone()),
        });
    }
    let argc_restore = state.define("#");
    ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
        target: argc_restore,
        value: SsaValue::Var(saved_argc),
    });

    // Restore locals
    for (lname, saved_ref) in &saved_locals {
        let rv = state.define(lname);
        ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
            target: rv,
            value: SsaValue::Var(saved_ref.clone()),
        });
    }

    // Restore exit status
    let status_restore = state.define("?");
    ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
        target: status_restore,
        value: SsaValue::Var(saved_status),
    });

    *block = restore_block;
    SsaValue::Var(temp_var)
}

fn lower_one(
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
                            let is_int = state.type_map.get(&a.name).copied() == Some(crate::ir::VarType::Int);
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
                        let saved_locals: Vec<(String, VarRef)> = locals.iter()
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
                                } else { vec![] }
                            }
                            _ => vec![],
                        };
                        let argc_val = SsaValue::Literal(func_args.len().to_string());
                        let argc_var = state.define("#");
                        ssa.block_mut(block).insts.push(SsaInst::Assign { target: argc_var, value: argc_val });
                        for (i, arg) in func_args.iter().enumerate() {
                            let v = expr_to_value(arg, state);
                            let param_var = state.define(&(i + 1).to_string());
                            ssa.block_mut(block).insts.push(SsaInst::Assign { target: param_var, value: v });
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
                        ssa.block_mut(restore_block).insts.push(SsaInst::CaptureEnd { target });
                        // Restore params
                        for (param_name, saved_ref) in &saved {
                            let rv = state.define(param_name);
                            ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
                                target: rv, value: SsaValue::Var(saved_ref.clone()),
                            });
                        }
                        let ra = state.define("#");
                        ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
                            target: ra, value: SsaValue::Var(saved_argc),
                        });
                        for (local_name, saved_ref) in &saved_locals {
                            let rv = state.define(local_name);
                            ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
                                target: rv, value: SsaValue::Var(saved_ref.clone()),
                            });
                        }
                        return restore_block;
                    }
                }
                // DD#58: lift IntFuncCall out of arithmetic expressions
                let val = lift_int_func_calls(val, ssa, state, &mut block);
                let target = state.define(&a.name);
                ssa.block_mut(block).insts.push(SsaInst::Assign {
                    target,
                    value: val,
                });
            }
            block
        }

        StmtKind::Declaration(d) => {
            let is_array = d.flags.contains(&"-a".to_string());
            let is_assoc = d.flags.contains(&"-A".to_string());
            for a in &d.assignments {
                // Bare `declare var` (no =value) — don't reset existing variable
                let is_bare = matches!(&a.value.kind, ExprKind::Literal(s) if s.is_empty())
                    && !is_array && !is_assoc;
                if is_bare && state.lookup(&a.name).version > 0 {
                    // Variable already defined — bare declare is a no-op
                    continue;
                }
                let val = if a.value.kind == ExprKind::Literal(String::new()) && (is_array || is_assoc) {
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
                        ssa.block_mut(block).insts.push(SsaInst::Test(test_expr.clone()));
                        return block;
                    }
                }
            }

            // var =~ s/pattern/replacement/flags — regex mutation
            if c.args.len() == 1 {
                let arg_s = expr_to_string(&c.args[0]);
                if arg_s.starts_with("s/") {
                    // Parse s/pattern/replacement/flags
                    let parts: Vec<&str> = arg_s[2..].splitn(3, '/').collect();
                    if parts.len() >= 2 {
                        let pattern = parts[0].to_string();
                        let replacement = parts[1].to_string();
                        let flags = if parts.len() > 2 { parts[2] } else { "" };

                        // DD#39: compile-time regex validation
                        if !pattern.contains('$') {
                            if let Err(msg) = crate::regex_validate::validate_regex(&pattern) {
                                state.diagnostics.push(format!(
                                    "1:1: error [SB5023] invalid regex in substitution: {}", msg
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
                let n = c.args.first()
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
                    ssa.block_mut(block).insts.push(SsaInst::Assign {
                        target,
                        value: val,
                    });
                }
                // Decrement $#
                let argc_ref = state.lookup("#");
                let new_argc = state.define("#");
                ssa.block_mut(block).insts.push(SsaInst::Assign {
                    target: new_argc,
                    value: SsaValue::Literal("__shift__".to_string()), // placeholder
                });
                return block;
            }

            // break → Goto(exit_block), continue → Goto(header_block)
            if name == "break" {
                let n = c.args.first()
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
                let n = c.args.first()
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
                let args: Vec<String> = c.args.iter().map(|a| expr_to_string(a)).collect();
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
                                    args: vec![
                                        SsaValue::Literal(signum.to_string()),
                                    ],
                                    redirects: vec![],
                                    checked: true,
                                });
                            }
                            "EXIT" => {
                                // Register FLAG handler + mark as exiting signal
                                ssa.block_mut(block).insts.push(SsaInst::Command {
                                    name: "__signal_flag".to_string(),
                                    args: vec![
                                        SsaValue::Literal(signum.to_string()),
                                    ],
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
                            || (body_str.starts_with('"') && body_str.ends_with('"')) {
                            &body_str[1..body_str.len()-1]
                        } else {
                            &body_str
                        };
                        let mut parser = tree_sitter::Parser::new();
                        parser.set_language(&tree_sitter_bash::LANGUAGE.into()).unwrap();
                        let tree = parser.parse(body_clean.as_bytes(), None).unwrap();
                        let trap_stmts = crate::lower::lower_body(tree.root_node(), body_clean.as_bytes());
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
                let args: Vec<String> = c.args.iter().map(|a| expr_to_string(a)).collect();
                for arg in &args {
                    // Strip surrounding quotes if present
                    let expr_str = arg.trim_matches('"').trim_matches('\'').to_string();

                    // If it's an assignment (e.g. "x=10"), define the variable
                    if let Some(eq_pos) = expr_str.find('=') {
                        let var_name = &expr_str[..eq_pos];
                        let val_str = &expr_str[eq_pos + 1..];
                        if !var_name.is_empty() && var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            let val_expr = crate::ir::Expr {
                                kind: ExprKind::ArithmeticExpansion(val_str.to_string()),
                                span: Span { line: 0, col: 0 },
                            };
                            let val = expr_to_value(&val_expr, state);
                            let val = lift_int_func_calls(val, ssa, state, &mut block);
                            let target = state.define(var_name);
                            ssa.block_mut(block).insts.push(SsaInst::Assign {
                                target,
                                value: val,
                            });
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
                    ssa.block_mut(block).insts.push(SsaInst::Assign {
                        target,
                        value: val,
                    });
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
                let args_str: Vec<String> = c.args.iter().map(|a| expr_to_string(a)).collect();
                // Filter out flags like -v, -f, -n
                let var_args: Vec<String> = args_str.iter().filter(|a| !a.starts_with('-')).cloned().collect();
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
                        let key = full[bracket+1..full.len()-1].to_string();
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
                let args: Vec<String> = c.args.iter().map(|a| expr_to_string(a)).collect();
                // Parse flags: -d DELIM (record delimiter), -f DELIM (field separator), -a ARR
                let mut delim: Option<u8> = None;
                let mut field_sep: Option<u8> = None;
                let mut array_name: Option<String> = None;
                let mut skip_next = false;
                let mut var_names: Vec<&String> = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    if skip_next { skip_next = false; continue; }
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
                if state.local_fn_scope.contains(&local_name) && state.function_bodies.contains_key(&local_name) {
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
                let saved_locals: Vec<(String, VarRef)> = locals.iter()
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
                        name, max_positional, call_argc,
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
                let func_redirs = state.function_redirects.get(&name).cloned().unwrap_or_default();
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

        StmtKind::For(f) => {
            lower_for(f, ssa, state, block)
        }

        StmtKind::Pipeline(p) => {
            if p.commands.len() > 1 {
                // Check if last stage is a while-read loop or mapfile
                let last = &p.commands[p.commands.len() - 1];
                let is_while_read = matches!(&last.kind, StmtKind::While(_));
                let is_mapfile = match &last.kind {
                    StmtKind::Command(c) => {
                        let n = expr_to_string(&c.name);
                        let args: Vec<String> = c.args.iter().map(|a| expr_to_string(a)).collect();
                        n == "mapfile" || n == "readarray" || (n == "read" && args.iter().any(|a| a == "-a"))
                    }
                    _ => false,
                };
                // DD#20: lastpipe — `cmd | read -r var` runs read in main shell
                let is_lastpipe_read = match &last.kind {
                    StmtKind::Command(c) => {
                        let n = expr_to_string(&c.name);
                        let args: Vec<String> = c.args.iter().map(|a| expr_to_string(a)).collect();
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
                            let args: Vec<SsaValue> = c.args.iter()
                                .map(|a| expr_to_value(a, state))
                                .collect();
                            stages.push((resolve_pipeline_command(&name), args, vec![]));
                        } else if let StmtKind::Redirected(r) = &cmd.kind {
                            if let StmtKind::Command(c) = &r.body.kind {
                                let name = expr_to_string(&c.name);
                                let args: Vec<SsaValue> = c.args.iter()
                                    .map(|a| expr_to_value(a, state))
                                    .collect();
                                stages.push((resolve_pipeline_command(&name), args, r.redirects.clone()));
                            }
                        }
                    }
                    // Emit PipeIntoWhile: pipeline stages + while body
                    ssa.block_mut(block).insts.push(SsaInst::Pipeline { stages, capture_read_fd: true, checked: true });
                    if is_mapfile {
                        // Lower the mapfile command, then patch fd to -1
                        block = lower_one(last, ssa, state, block);
                        for bid in ssa.blocks.keys().cloned().collect::<Vec<_>>() {
                            for inst in &mut ssa.block_mut(bid).insts {
                                if let SsaInst::Mapfile { fd, .. } = inst {
                                    if *fd == 0 { *fd = -1; }
                                }
                                if let SsaInst::ReadArray { fd, .. } = inst {
                                    if *fd == 0 { *fd = -1; }
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
                            let args: Vec<SsaValue> = c.args.iter()
                                .map(|a| expr_to_value(a, state))
                                .collect();
                            stages.push((resolve_pipeline_command(&name), args, vec![]));
                        } else if let StmtKind::Redirected(r) = &cmd.kind {
                            if let StmtKind::Command(c) = &r.body.kind {
                                let name = expr_to_string(&c.name);
                                let args: Vec<SsaValue> = c.args.iter()
                                    .map(|a| expr_to_value(a, state))
                                    .collect();
                                stages.push((resolve_pipeline_command(&name), args, r.redirects.clone()));
                            }
                        }
                    }
                    ssa.block_mut(block).insts.push(SsaInst::Pipeline { stages, capture_read_fd: true, checked: true });
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
                                if field_sep == Some(0) { field_sep = Some(s.as_bytes().get(0).copied().unwrap_or(0)); }
                                skip_next = false; continue;
                            }
                            if s == "-d" { skip_next = true; delim = Some(0); continue; }
                            if s.starts_with("-d") { delim = Some(s.as_bytes().get(2).copied().unwrap_or(0)); continue; }
                            if s == "-f" { skip_next = true; field_sep = Some(0); continue; }
                            if s.starts_with("-f") { field_sep = Some(s.as_bytes().get(2).copied().unwrap_or(0)); continue; }
                            if s.starts_with('-') { continue; }
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
                    let mut compound_producers: Vec<(usize, usize)> = vec![];
                    for (idx, cmd) in p.commands.iter().enumerate() {
                        // Unwrap Suppressed/Redirected to get inner command
                        let (inner, redirects) = match &cmd.kind {
                            StmtKind::Suppressed(s) => {
                                // - prefix in pipeline: include but don't count toward pipefail
                                // For now, just skip suppressed stages entirely
                                continue;
                            }
                            StmtKind::Redirected(r) => (&r.body.kind, r.redirects.clone()),
                            other => (other, vec![]),
                        };
                        if let StmtKind::Command(c) = inner {
                            let name = expr_to_string(&c.name);
                            let args: Vec<SsaValue> = c.args.iter()
                                .map(|a| expr_to_value(a, state))
                                .collect();
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
                    ssa.block_mut(block).insts.push(SsaInst::Pipeline { stages, capture_read_fd: false, checked: true });
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
                        true_block: merge_block,   // success → skip right
                        false_block: right_block,   // failure → run right
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
                        true_block: parent_block,  // parent continues
                        false_block: child_block,   // child runs command
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
                    let all_literal_zero = returns.iter().all(|v| matches!(v, Some(s) if s.is_empty() || s == "0"));
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
            let is_compound = matches!(r.body.kind,
                StmtKind::While(_) | StmtKind::If(_) | StmtKind::For(_)
                | StmtKind::Case(_) | StmtKind::Group(_) | StmtKind::Subshell(_)
                | StmtKind::Function { .. }
            );

            // Special case: redirected function definition — store redirects
            // for application at call sites
            if let StmtKind::Function(ref f) = r.body.kind {
                if !r.redirects.is_empty() {
                    state.function_redirects.insert(f.name.clone(), r.redirects.clone());
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

        StmtKind::Group(g) => {
            lower_stmts(&g.body, ssa, state, block)
        }

        StmtKind::Case(c) => {
            lower_case(c, ssa, state, block)
        }

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
                        SsaInst::Command { checked, .. } => { *checked = false; }
                        SsaInst::Pipeline { checked, .. } => { *checked = false; }
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

// ── If lowering ───────────────────────────────────────────────

/// Desugar elif chains into nested if/else.
/// `if A then X elif B then Y else Z fi` becomes
/// `if A then X else { if B then Y else Z fi } fi`
fn flatten_elif(i: &If) -> If {
    if i.elif_clauses.is_empty() {
        return i.clone();
    }
    // Build from the tail: start with the else body (or None)
    let mut else_body = i.else_body.clone();
    for elif in i.elif_clauses.iter().rev() {
        let nested = If {
            condition: elif.condition.clone(),
            then_body: elif.body.clone(),
            elif_clauses: vec![],
            else_body,
        };
        else_body = Some(vec![Statement {
            kind: StmtKind::If(nested),
            span: Span::default(),
        }]);
    }
    If {
        condition: i.condition.clone(),
        then_body: i.then_body.clone(),
        elif_clauses: vec![],
        else_body,
    }
}

/// DD#24: if bless VAR [[ test ]]; then ... fi
/// Non-fatal bless: test branch gets clean var, else branch keeps tainted.
fn lower_if_bless(
    i: &If,
    c: &crate::ir::Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let var_name = expr_to_string(&c.args[0]);
    // Find test expression
    let test_expr = c.args[1..].iter()
        .find_map(|a| if let crate::ir::ExprKind::Test(t) = &a.kind { Some(t.clone()) } else { None });
    let test_expr = match test_expr {
        Some(t) => t,
        None => return block,
    };

    // Emit the test as a condition (non-fatal)
    let test_expr_copy = test_expr.clone();
    ssa.block_mut(block).insts.push(SsaInst::Test(test_expr));

    let then_block = ssa.new_block("if_bless.then");
    let merge_block = ssa.new_block("if_bless.merge");

    let pre_state = state.snapshot();

    // Branch on test result
    ssa.block_mut(block).terminator = Terminator::Branch {
        true_block: then_block,
        false_block: merge_block,
        condition: "bless".to_string(),
    };

    // Then branch: bless var (copy source to target, marking clean)
    let source_ref = state.lookup(&var_name);
    let target_ref = state.define(&var_name);
    // Use a trivial always-true test for the SSA taint verifier
    ssa.block_mut(then_block).insts.push(SsaInst::Bless {
        target: target_ref,
        source: source_ref,
        test: test_expr_copy,
    });
    let then_exit = lower_stmts(&i.then_body, ssa, state, then_block);
    ssa.block_mut(then_exit).terminator = Terminator::Goto(merge_block);
    let then_snap = state.snapshot();

    if let Some(else_body) = &i.else_body {
        let else_block = ssa.new_block("if_bless.else");
        if let Terminator::Branch { false_block, .. } = &mut ssa.block_mut(block).terminator {
            *false_block = else_block;
        }
        state.restore(&pre_state);
        let else_exit = lower_stmts(else_body, ssa, state, else_block);
        ssa.block_mut(else_exit).terminator = Terminator::Goto(merge_block);
    } else {
        state.restore(&pre_state);
    }

    // Merge: use then_state versions for variables modified in then
    // (simplified: just restore to pre-state, let Cranelift SSA handle merge)

    merge_block
}

fn lower_if(
    i: &If,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#24: detect if-bless conditional form
    if i.condition.len() == 1 {
        if let StmtKind::Command(c) = &i.condition[0].kind {
            if expr_to_string(&c.name) == "bless" && c.args.len() >= 2 {
                return lower_if_bless(i, c, ssa, state, block);
            }
        }
    }

    // Lower condition into current block
    state.in_condition += 1;
    let cond_block = lower_stmts(&i.condition, ssa, state, block);
    state.in_condition -= 1;
    let cond_str = "condition".to_string(); // placeholder

    let then_block = ssa.new_block("if.then");
    let merge_block = ssa.new_block("if.merge");

    // Snapshot state before branches
    let pre_state = state.snapshot();

    // Lower then branch
    let then_exit = lower_stmts(&i.then_body, ssa, state, then_block);
    ssa.block_mut(then_exit).terminator = Terminator::Goto(merge_block);
    let then_state = state.snapshot();

    if let Some(else_body) = &i.else_body {
        let else_block = ssa.new_block("if.else");

        // Restore to pre-branch state for else
        state.restore(&pre_state);
        let else_exit = lower_stmts(else_body, ssa, state, else_block);
        ssa.block_mut(else_exit).terminator = Terminator::Goto(merge_block);
        let else_state = state.snapshot();

        // Set branch terminator
        ssa.block_mut(cond_block).terminator = Terminator::Branch {
            condition: cond_str,
            true_block: then_block,
            false_block: else_block,
        };

        // Insert phi nodes at merge point for variables modified in either branch
        insert_phis_two_branch(
            state, ssa, merge_block,
            then_exit, &then_state,
            else_exit, &else_state,
        );
    } else {
        // No else — branch to then or skip to merge
        ssa.block_mut(cond_block).terminator = Terminator::Branch {
            condition: cond_str,
            true_block: then_block,
            false_block: merge_block,
        };

        // Phi nodes: then-branch version vs pre-branch version
        insert_phis_two_branch(
            state, ssa, merge_block,
            then_exit, &then_state,
            cond_block, &pre_state,
        );
    }

    // Handle elif clauses by chaining — each elif is a nested if
    // For simplicity in this implementation, elif clauses were already
    // flattened into the IR's if/elif structure. We handle the first
    // else and leave elif for future refinement.

    merge_block
}

// ── While lowering ────────────────────────────────────────────

fn lower_while(
    w: &While,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let header = ssa.new_block("while.header");
    let body_block = ssa.new_block("while.body");
    let exit_block = ssa.new_block("while.exit");

    // Wire entry → header
    ssa.block_mut(block).terminator = Terminator::Goto(header);

    // Snapshot state before loop
    let pre_state = state.snapshot();

    // === Two-pass loop lowering ===
    // Pass 1: Lower body to discover which variables are modified
    let _state_at_header = state.snapshot();
    let body_exit_1 = lower_stmts(&w.body, ssa, state, body_block);
    let body_state_1 = state.snapshot();

    // Insert phi nodes at header for loop-carried variables
    // This creates new versions (e.g., x_3) for modified variables
    insert_phis_two_branch(
        state, ssa, header,
        block, &pre_state,
        body_exit_1, &body_state_1,
    );

    // Pass 2: Re-lower condition and body using phi-merged versions
    // Clear the body block's instructions (we're re-lowering)
    ssa.block_mut(body_block).insts.clear();
    ssa.block_mut(header).insts.clear();

    // Lower condition in header using post-phi state
    state.in_condition += 1;
    let cond_exit = lower_stmts(&w.condition, ssa, state, header);
    state.in_condition -= 1;

    // Set loop terminator
    ssa.block_mut(cond_exit).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // Push loop context for break/continue
    state.loop_stack.push((header, exit_block));

    // Re-lower body with phi-merged versions
    let body_exit = lower_stmts(&w.body, ssa, state, body_block);
    ssa.block_mut(body_exit).terminator = Terminator::Goto(header);

    // Pop loop context
    state.loop_stack.pop();

    // Update phi sources to use the correct versions from pass 2
    let body_state_2 = state.snapshot();
    let header_phis = &mut ssa.block_mut(header).phis;
    for phi in header_phis.iter_mut() {
        let var_name = &phi.target.name;
        // Update the body source (index 1) with the new version
        if let Some(ver) = body_state_2.get(var_name) {
            phi.sources[1].1 = VarRef::new(var_name, *ver);
        }
    }

    exit_block
}

// ── For lowering ──────────────────────────────────────────────

fn lower_for(
    f: &For,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // Check if this is a dynamic for loop (e.g., `for x in "$@"`)
    let is_dynamic = f.words.iter().any(|w| !matches!(&w.kind, ExprKind::Literal(_)));

    if is_dynamic && f.words.len() == 1 {
        // Check for "$@" — may be SimpleExpansion("@") or DoubleQuoted([SimpleExpansion("@")])
        let is_dollar_at = match &f.words[0].kind {
            ExprKind::SimpleExpansion(v) if v == "@" => true,
            ExprKind::DoubleQuoted(parts) if parts.len() == 1 => {
                matches!(&parts[0].kind, ExprKind::SimpleExpansion(v) if v == "@")
            }
            _ => false,
        };
        if is_dollar_at {
            return lower_for_dollar_at(f, ssa, state, block);
        }

        // Check for "${arr[@]}" or "${!arr[@]}" — array iteration
        let arr_info = match &f.words[0].kind {
            ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => Some((pe.name.clone(), false)),
            ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrkeys") => Some((pe.name.clone(), true)),
            ExprKind::DoubleQuoted(parts) if parts.len() == 1 => {
                match &parts[0].kind {
                    ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => Some((pe.name.clone(), false)),
                    ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrkeys") => Some((pe.name.clone(), true)),
                    _ => None,
                }
            }
            // Sash-native: $(keys arr) in for-loop → array key iteration (DD#41)
            ExprKind::CommandSubstitution(stmts) if stmts.len() == 1 => {
                let inner = match &stmts[0].kind {
                    StmtKind::Redirected(r) => &r.body.kind,
                    other => other,
                };
                if let StmtKind::Command(c) = inner {
                    if let ExprKind::Literal(cmd_name) = &c.name.kind {
                        if cmd_name == "keys" {
                            c.args.first().map(|a| (expr_to_string(a), true))
                        } else {
                            None
                        }
                    } else { None }
                } else { None }
            }
            _ => None,
        };
        if let Some((name, use_keys)) = arr_info {
            return lower_for_array(f, &name, use_keys, ssa, state, block);
        }

        // Check for $(cmd) — command substitution in for-loop
        // DD#28/DD#40: silently transform to pipe-while-read
        // `for x in $(cmd args)` → `cmd args | while read -r x; do body; done`
        let is_cmd_subst = matches!(&f.words[0].kind, ExprKind::CommandSubstitution(_));
        if is_cmd_subst {
            if let ExprKind::CommandSubstitution(stmts) = &f.words[0].kind {
                // Build pipeline stages from the command substitution
                let mut stages = vec![];
                for stmt in stmts {
                    let cmd_stmt = match &stmt.kind {
                        StmtKind::Redirected(r) => &r.body,
                        _ => stmt,
                    };
                    if let StmtKind::Command(c) = &cmd_stmt.kind {
                        let name = expr_to_string(&c.name);
                        let args: Vec<SsaValue> = c.args.iter()
                            .map(|a| expr_to_value(a, state))
                            .collect();
                        let redirs = match &stmt.kind {
                            StmtKind::Redirected(r) => r.redirects.clone(),
                            _ => vec![],
                        };
                        stages.push((resolve_pipeline_command(&name), args, redirs));
                    }
                }
                // Emit pipeline with pipe capture
                ssa.block_mut(block).insts.push(SsaInst::Pipeline { stages, capture_read_fd: true, checked: true });

                // Build a while-read loop over the pipe
                let loop_var = &f.variable;
                let header = ssa.new_block("for_cmd.hdr");
                let body_block = ssa.new_block("for_cmd.body");
                let exit = ssa.new_block("for_cmd.exit");

                ssa.block_mut(block).terminator = Terminator::Goto(header);

                // Header: read -r var from pipe (fd = -1 sentinel)
                let target = state.define(loop_var);
                ssa.block_mut(header).insts.push(SsaInst::ReadLine {
                    target,
                    fd: -1,
                    delim: None,
                    redirects: vec![],
                    clean_pipe: false,
                });
                ssa.block_mut(header).terminator = Terminator::Branch {
                    condition: "__test_result".to_string(),
                    true_block: body_block,
                    false_block: exit,
                };

                // Body: lower the for-loop body statements
                state.loop_stack.push((header, exit));
                let body_end = lower_stmts(&f.body, ssa, state, body_block);
                state.loop_stack.pop();
                ssa.block_mut(body_end).terminator = Terminator::Goto(header);

                return exit;
            }
        }
    }

    // Static word list — unroll
    let exit_block = ssa.new_block("for.exit");
    let mut current = block;

    // Pre-create iteration blocks so continue can jump to the next one
    let iter_blocks: Vec<BlockId> = f.words.iter()
        .map(|_| ssa.new_block("for.iter"))
        .collect();

    for (i, word) in f.words.iter().enumerate() {
        let iter_block = iter_blocks[i];
        ssa.block_mut(current).terminator = Terminator::Goto(iter_block);
        current = iter_block;

        // continue → next iteration (or exit if last)
        let continue_target = if i + 1 < iter_blocks.len() { iter_blocks[i + 1] } else { exit_block };
        state.loop_stack.push((continue_target, exit_block));

        let val = expr_to_value(word, state);
        let loop_var = state.define(&f.variable);
        ssa.block_mut(current).insts.push(SsaInst::Assign {
            target: loop_var,
            value: val,
        });
        current = lower_stmts(&f.body, ssa, state, current);

        state.loop_stack.pop();
    }

    ssa.block_mut(current).terminator = Terminator::Goto(exit_block);
    exit_block
}

// ── Case lowering ─────────────────────────────────────────────

fn lower_case(
    c: &Case,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let merge_block = ssa.new_block("case.merge");

    // Empty case — no items
    if c.items.is_empty() {
        ssa.block_mut(block).terminator = Terminator::Goto(merge_block);
        return merge_block;
    }

    let pre_state = state.snapshot();
    let mut branch_states: Vec<(BlockId, std::collections::HashMap<String, u32>)> = Vec::new();

    // Build case body blocks
    let mut case_blocks = Vec::new();
    let mut case_exits = Vec::new();
    for (i, item) in c.items.iter().enumerate() {
        state.restore(&pre_state);
        let case_block = ssa.new_block(&format!("case.{}", i));
        let case_exit = lower_stmts(&item.body, ssa, state, case_block);
        // Don't set terminator yet — need to know next block for fall-through
        branch_states.push((case_exit, state.snapshot()));
        case_blocks.push(case_block);
        case_exits.push(case_exit);
    }

    // Wire up terminators: fall-through (;&) goes to next body, normal (;;) goes to merge
    for (i, item) in c.items.iter().enumerate() {
        let target = if item.fallthrough && i + 1 < case_blocks.len() {
            case_blocks[i + 1]
        } else {
            merge_block
        };
        ssa.block_mut(case_exits[i]).terminator = Terminator::Goto(target);
    }

    // Pre-create check blocks for dispatch chain (needed for ;;& resume wiring)
    let mut check_blocks: Vec<BlockId> = Vec::new();
    check_blocks.push(block); // check_blocks[0] = entry block
    for i in 1..c.items.len() {
        check_blocks.push(ssa.new_block(&format!("case.check.{}", i)));
    }

    // Wire up terminators: fall-through (;&) goes to next body,
    // resume (;;&) goes to next check, normal (;;) goes to merge
    for (i, item) in c.items.iter().enumerate() {
        let target = if item.fallthrough && i + 1 < case_blocks.len() {
            case_blocks[i + 1]
        } else if item.resume && i + 1 < check_blocks.len() {
            check_blocks[i + 1]
        } else {
            merge_block
        };
        ssa.block_mut(case_exits[i]).terminator = Terminator::Goto(target);
    }

    // Build dispatch chain: compare value against each pattern
    let case_value_expr = c.value.clone();

    for (i, item) in c.items.iter().enumerate() {
        let current = check_blocks[i];
        let is_wildcard = item.patterns.iter().any(|p| {
            matches!(&p.kind, ExprKind::Literal(s) if s == "*")
        });

        let next_check = if i + 1 < check_blocks.len() {
            check_blocks[i + 1]
        } else {
            merge_block
        };

        if is_wildcard {
            // Wildcard — unconditional jump, always matches
            ssa.block_mut(current).terminator = Terminator::Goto(case_blocks[i]);
            continue;
        }

        // Emit test: value == pattern1 || value == pattern2 || ...
        if item.patterns.len() == 1 {
            let test = crate::ir::TestExpr::Binary {
                left: Box::new(case_value_expr.clone()),
                op: "=glob".to_string(),
                right: Box::new(item.patterns[0].clone()),
            };
            ssa.block_mut(current).insts.push(SsaInst::Test(test));
            ssa.block_mut(current).terminator = Terminator::Branch {
                condition: "__test".to_string(),
                true_block: case_blocks[i],
                false_block: next_check,
            };
        } else {
            // Multiple patterns: chain of blocks, jump to case_block on first match
            let mut pat_current = current;
            for (pi, pat) in item.patterns.iter().enumerate() {
                let test = crate::ir::TestExpr::Binary {
                    left: Box::new(case_value_expr.clone()),
                    op: "=glob".to_string(),
                    right: Box::new(pat.clone()),
                };
                ssa.block_mut(pat_current).insts.push(SsaInst::Test(test));

                if pi + 1 < item.patterns.len() {
                    let next_pat = ssa.new_block(&format!("case.{}.pat.{}", i, pi + 1));
                    ssa.block_mut(pat_current).terminator = Terminator::Branch {
                        condition: "__test".to_string(),
                        true_block: case_blocks[i],
                        false_block: next_pat,
                    };
                    pat_current = next_pat;
                } else {
                    ssa.block_mut(pat_current).terminator = Terminator::Branch {
                        condition: "__test".to_string(),
                        true_block: case_blocks[i],
                        false_block: next_check,
                    };
                }
            }
        }
    }

    // Determine if a no-match path exists (can reach merge without entering
    // any arm).  A wildcard arm always matches, so no-match is impossible if
    // any arm is a wildcard.  Otherwise the false branch of the last check
    // falls through to merge.
    let has_wildcard = c.items.iter().any(|item| {
        item.patterns.iter().any(|p| matches!(&p.kind, ExprKind::Literal(s) if s == "*"))
    });

    // Build a no-match state: pre_state with "?" forced to 0
    // (POSIX: "if no pattern is matched, the exit status is zero")
    let no_match_path: Option<(BlockId, std::collections::HashMap<String, u32>)> = if !has_wildcard {
        let _no_match_state = pre_state.clone();
        // Define "?" = 0 on the no-match path via a dedicated block
        let no_match_block = ssa.new_block("case.nomatch");
        state.restore(&pre_state);
        let zero_ref = state.define("?");
        let no_match_snapshot = state.snapshot();
        ssa.block_mut(no_match_block).insts.push(SsaInst::Assign {
            target: zero_ref.clone(),
            value: SsaValue::Literal("0".to_string()),
        });
        ssa.block_mut(no_match_block).terminator = Terminator::Goto(merge_block);

        // Re-wire: anywhere the dispatch chain jumps to merge_block for
        // no-match, redirect to no_match_block instead
        for bid in &check_blocks {
            let block = ssa.block_mut(*bid);
            match &mut block.terminator {
                Terminator::Branch { false_block, .. } if *false_block == merge_block => {
                    *false_block = no_match_block;
                }
                _ => {}
            }
        }
        // Also check pattern sub-blocks — they may branch false to merge
        let all_block_ids: Vec<usize> = ssa.blocks.keys().cloned().collect();
        for bid in all_block_ids {
            if check_blocks.contains(&bid) || bid == merge_block || bid == no_match_block {
                continue;
            }
            let block = ssa.block_mut(bid);
            match &mut block.terminator {
                Terminator::Branch { false_block, .. } if *false_block == merge_block => {
                    *false_block = no_match_block;
                }
                _ => {}
            }
        }

        Some((no_match_block, no_match_snapshot))
    } else {
        None
    };

    // Insert phi nodes at merge across all paths (arms + optional no-match)
    let mut all_paths = branch_states.clone();
    if let Some(ref nm) = no_match_path {
        all_paths.push(nm.clone());
    }

    if all_paths.len() >= 2 {
        let all_vars: std::collections::HashSet<String> = all_paths.iter()
            .flat_map(|(_, s)| s.keys().cloned())
            .collect();

        for var in &all_vars {
            let versions: Vec<(BlockId, u32)> = all_paths.iter()
                .map(|(bid, s)| (*bid, s.get(var).copied().unwrap_or(0)))
                .collect();

            let first_ver = versions[0].1;
            if versions.iter().any(|(_, v)| *v != first_ver) {
                let target = state.define(var);
                let sources: Vec<(BlockId, VarRef)> = versions.iter()
                    .map(|(bid, ver)| (*bid, VarRef::new(var, *ver)))
                    .collect();
                ssa.block_mut(merge_block).phis.push(PhiNode {
                    target,
                    sources,
                });
            }
        }
    } else if all_paths.len() == 1 {
        // Single path (e.g. wildcard-only case) — adopt its state
        let (_, ref path_state) = all_paths[0];
        state.restore(path_state);
    }

    merge_block
}

// ── Phi insertion helper ──────────────────────────────────────

/// Insert phi nodes at a merge block for variables that differ
/// between two predecessor branches.
fn insert_phis_two_branch(
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    merge: BlockId,
    block_a: BlockId,
    state_a: &std::collections::HashMap<String, u32>,
    block_b: BlockId,
    state_b: &std::collections::HashMap<String, u32>,
) {
    let all_vars: std::collections::HashSet<&String> = state_a.keys()
        .chain(state_b.keys())
        .collect();

    for var in all_vars {
        let ver_a = state_a.get(var).copied().unwrap_or(0);
        let ver_b = state_b.get(var).copied().unwrap_or(0);
        if ver_a != ver_b {
            let target = state.define(var);
            ssa.block_mut(merge).phis.push(PhiNode {
                target,
                sources: vec![
                    (block_a, VarRef::new(var, ver_a)),
                    (block_b, VarRef::new(var, ver_b)),
                ],
            });
        }
    }
}

// ── Expression → SSA value ────────────────────────────────────

fn expr_to_value(expr: &Expr, state: &VersionState) -> SsaValue {
    match &expr.kind {
        ExprKind::Literal(s) => SsaValue::Literal(s.clone()),
        ExprKind::SimpleExpansion(var) => {
            // Coerce magic variables to builtins (DD#40)
            if var == "EPOCHSECONDS" {
                return SsaValue::CmdSubst { argv: vec![SsaValue::Literal("epochseconds".to_string())], redirects: vec![] };
            }
            SsaValue::Var(state.lookup(var))
        }
        ExprKind::SingleQuoted(s) => SsaValue::Literal(s.clone()),
        ExprKind::ArithmeticExpansion(s) => {
            // Try to parse arithmetic expressions
            let trimmed = s.trim();
            // Empty expression: $(()) → 0
            if trimmed.is_empty() {
                return SsaValue::Expr("0".to_string());
            }
            // DD#58: look up arithmetic placeholder from preprocessor
            if trimmed.starts_with("__ARITH_") && trimmed.ends_with("__") {
                if let Some(original) = crate::lookup_arith(trimmed) {
                    let e = Expr { kind: ExprKind::ArithmeticExpansion(original), span: expr.span };
                    return expr_to_value(&e, state);
                }
            }
            // Base literal: N#digits → evaluate at compile time
            if let Some(hash_pos) = trimmed.find('#') {
                let base_str = &trimmed[..hash_pos];
                let digits = &trimmed[hash_pos+1..];
                if let Ok(base) = base_str.parse::<u32>() {
                    if base >= 2 && base <= 64 && !digits.is_empty() {
                        let val = digits.chars().fold(0i64, |acc, c| {
                            let d = match c {
                                '0'..='9' => c as i64 - '0' as i64,
                                'a'..='z' => c as i64 - 'a' as i64 + 10,
                                'A'..='Z' => c as i64 - 'A' as i64 + 36,
                                '@' => 62,
                                '_' => 63,
                                _ => 0,
                            };
                            acc * base as i64 + d
                        });
                        return SsaValue::Expr(val.to_string());
                    }
                }
            }
            // Hex literal: 0x... or 0X...
            if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
                if let Ok(v) = i64::from_str_radix(&trimmed[2..], 16) {
                    return SsaValue::Expr(v.to_string());
                }
            }
            // Octal literal: 0... (but not just "0")
            if trimmed.starts_with('0') && trimmed.len() > 1 && trimmed[1..].chars().all(|c| c >= '0' && c <= '7') {
                if let Ok(v) = i64::from_str_radix(&trimmed[1..], 8) {
                    return SsaValue::Expr(v.to_string());
                }
            }
            // Bare number: $((7)) or $((-7)) or $((+1)) → literal
            if let Ok(v) = trimmed.parse::<i64>() {
                return SsaValue::Expr(v.to_string());
            }
            // $-prefixed variable: $(($x)) or $(($1)) → variable lookup
            if trimmed.starts_with('$') {
                let varname = trimmed.trim_start_matches('$');
                if varname.chars().all(|c| c.is_alphanumeric() || c == '_') && !varname.is_empty() {
                    return SsaValue::ArithBinOp {
                        left: Box::new(SsaValue::Var(state.lookup(varname))),
                        op: ArithOp::Add,
                        right: Box::new(SsaValue::Expr("0".to_string())),
                    };
                }
            }
            // DD#58: Bare function name in arithmetic → int function call
            if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_') && !trimmed.is_empty()
                && state.function_int_typed.contains(trimmed) {
                return SsaValue::IntFuncCall {
                    name: trimmed.to_string(),
                    args: vec![],
                };
            }
            // DD#58: Sexpr call (name args...) in arithmetic context
            if trimmed.starts_with('(') && trimmed.ends_with(')') {
                let inner = trimmed[1..trimmed.len()-1].trim();
                if let Some(call) = try_parse_sexpr(inner, expr.span, state) {
                    return call;
                }
            }
            // Bare variable: $((y)) → force arithmetic interpretation (empty/non-numeric → 0)
            if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_') && !trimmed.is_empty() {
                return SsaValue::ArithBinOp {
                    left: Box::new(SsaValue::Var(state.lookup(trimmed))),
                    op: ArithOp::Add,
                    right: Box::new(SsaValue::Expr("0".to_string())),
                };
            }
            // Unary plus: $((+x)) → strip plus
            if trimmed.starts_with('+') {
                let inner = trimmed[1..].trim();
                if !inner.is_empty() {
                    let e = Expr { kind: ExprKind::ArithmeticExpansion(inner.to_string()), span: expr.span };
                    return expr_to_value(&e, state);
                }
            }
            // Unary bitwise NOT: $((~expr)) → compute inline
            if trimmed.starts_with('~') {
                let inner = trimmed[1..].trim();
                if !inner.is_empty() {
                    let e = Expr { kind: ExprKind::ArithmeticExpansion(inner.to_string()), span: expr.span };
                    let inner_val = expr_to_value(&e, state);
                    // Constant fold if possible
                    if let SsaValue::Expr(ref s) = inner_val {
                        if let Ok(v) = s.parse::<i64>() {
                            return SsaValue::Expr((!v).to_string());
                        }
                    }
                    // Runtime: XOR with -1
                    return SsaValue::ArithBinOp {
                        left: Box::new(inner_val),
                        op: ArithOp::BitXor,
                        right: Box::new(SsaValue::Expr((-1i64).to_string())),
                    };
                }
            }
            // Unary logical NOT: $((! expr)) → compute inline
            if trimmed.starts_with('!') && !trimmed.starts_with("!=") {
                let inner = trimmed[1..].trim();
                if !inner.is_empty() {
                    let e = Expr { kind: ExprKind::ArithmeticExpansion(inner.to_string()), span: expr.span };
                    let inner_val = expr_to_value(&e, state);
                    if let SsaValue::Expr(ref s) = inner_val {
                        if let Ok(v) = s.parse::<i64>() {
                            return SsaValue::Expr(if v == 0 { "1" } else { "0" }.to_string());
                        }
                    }
                    // Runtime: compare with 0
                    return SsaValue::ArithBinOp {
                        left: Box::new(inner_val),
                        op: ArithOp::Eq,
                        right: Box::new(SsaValue::Expr("0".to_string())),
                    };
                }
            }
            // Arithmetic assignment: var = expr → assign var, return value
            if let Some(eq_pos) = trimmed.find('=') {
                // Make sure it's not ==, !=, <=, >=
                let before_eq = if eq_pos > 0 { trimmed.as_bytes()[eq_pos - 1] } else { 0 };
                let after_eq = if eq_pos + 1 < trimmed.len() { trimmed.as_bytes()[eq_pos + 1] } else { 0 };
                if after_eq != b'=' && before_eq != b'!' && before_eq != b'<' && before_eq != b'>' && before_eq != b'=' {
                    let lhs = trimmed[..eq_pos].trim();
                    let rhs = trimmed[eq_pos+1..].trim();
                    // Check for compound assignment: +=, -=, *=, /=, %=
                    let (var_name, op, rhs_str) = if !lhs.is_empty() && lhs.ends_with('+') {
                        (&lhs[..lhs.len()-1], Some(ArithOp::Add), rhs)
                    } else if !lhs.is_empty() && lhs.ends_with('-') {
                        (&lhs[..lhs.len()-1], Some(ArithOp::Sub), rhs)
                    } else if !lhs.is_empty() && lhs.ends_with('*') {
                        (&lhs[..lhs.len()-1], Some(ArithOp::Mul), rhs)
                    } else if !lhs.is_empty() && lhs.ends_with('/') {
                        (&lhs[..lhs.len()-1], Some(ArithOp::Div), rhs)
                    } else if !lhs.is_empty() && lhs.ends_with('%') {
                        (&lhs[..lhs.len()-1], Some(ArithOp::Mod), rhs)
                    } else {
                        (lhs, None, rhs)
                    };
                    let var_name = var_name.trim();
                    if var_name.chars().all(|c| c.is_alphanumeric() || c == '_') && !var_name.is_empty() && !rhs_str.is_empty() {
                        let rhs_expr = Expr { kind: ExprKind::ArithmeticExpansion(rhs_str.to_string()), span: expr.span };
                        let rhs_val = expr_to_value(&rhs_expr, state);
                        let result_val = if let Some(op) = op {
                            // Compound: var OP= rhs → var = var OP rhs
                            SsaValue::ArithBinOp {
                                left: Box::new(SsaValue::Var(state.lookup(var_name))),
                                op,
                                right: Box::new(rhs_val),
                            }
                        } else {
                            rhs_val
                        };
                        // For now, we can't easily do the SSA assignment + return in an expression context.
                        // But we can at least constant-fold the common case.
                        if let SsaValue::Expr(ref s) = result_val {
                            if let Ok(_) = s.parse::<i64>() {
                                // Assignment with constant value: define the var and return the value
                                // Note: we can't properly emit SSA assign here since we're in expr_to_value.
                                // Just return the value — the variable won't be updated.
                                // This handles $((x = 1)) → "1" for display purposes.
                                return result_val;
                            }
                        }
                        return result_val;
                    }
                }
            }
            // Unary minus on variable: $((-x)) → 0 - x
            if trimmed.starts_with('-') {
                let inner = trimmed[1..].trim();
                if inner.chars().all(|c| c.is_alphanumeric() || c == '_') && !inner.is_empty() {
                    return SsaValue::ArithBinOp {
                        left: Box::new(SsaValue::Expr("0".to_string())),
                        op: ArithOp::Sub,
                        right: Box::new(SsaValue::Var(state.lookup(inner))),
                    };
                }
            }
            // Ternary: cond ? true_val : false_val
            if let Some(q_pos) = trimmed.find('?') {
                let after_q = &trimmed[q_pos+1..];
                if let Some(c_pos) = after_q.find(':') {
                    let cond_str = trimmed[..q_pos].trim();
                    let true_str = after_q[..c_pos].trim();
                    let false_str = after_q[c_pos+1..].trim();
                    // Recursively evaluate each part
                    let cond_expr = Expr { kind: ExprKind::ArithmeticExpansion(cond_str.to_string()), span: expr.span };
                    let true_expr = Expr { kind: ExprKind::ArithmeticExpansion(true_str.to_string()), span: expr.span };
                    let false_expr = Expr { kind: ExprKind::ArithmeticExpansion(false_str.to_string()), span: expr.span };
                    return SsaValue::Ternary {
                        cond: Box::new(expr_to_value(&cond_expr, state)),
                        true_val: Box::new(expr_to_value(&true_expr, state)),
                        false_val: Box::new(expr_to_value(&false_expr, state)),
                    };
                }
            }
            // Parse "lhs OP rhs" — handles both "i + 1" and "i+1" and "$i+1"
            let (lhs, op_parsed, rhs) = parse_arith_binop(trimmed);
            if let Some(op) = op_parsed {
                let lhs_was_var = lhs.starts_with('$');
                let rhs_was_var = rhs.starts_with('$');
                let lhs = lhs.trim_start_matches('$');
                let rhs = rhs.trim_start_matches('$');

                // Helper: convert an arithmetic sub-expression string to SsaValue
                let to_val = |s: &str| -> SsaValue {
                    let e = Expr { kind: ExprKind::ArithmeticExpansion(s.to_string()), span: expr.span };
                    expr_to_value(&e, state)
                };

                let l_const = if lhs_was_var { Err(std::num::IntErrorKind::InvalidDigit) } else { lhs.parse::<i64>().map_err(|e| e.kind().clone()) };
                let r_const = if rhs_was_var { Err(std::num::IntErrorKind::InvalidDigit) } else { rhs.parse::<i64>().map_err(|e| e.kind().clone()) };

                // Check if lhs/rhs are complex (contain operators/parens) — if so, recurse
                let l_is_simple = lhs.chars().all(|c| c.is_alphanumeric() || c == '_');
                let r_is_simple = rhs.chars().all(|c| c.is_alphanumeric() || c == '_');

                match (l_const, r_const) {
                    (Ok(l), Ok(r)) => {
                            // Constant fold
                            let result = match op {
                                ArithOp::Add => l + r,
                                ArithOp::Sub => l - r,
                                ArithOp::Mul => l * r,
                                ArithOp::Div => if r != 0 { l / r } else { return SsaValue::ArithBinOp { left: Box::new(SsaValue::Expr(l.to_string())), op: ArithOp::Div, right: Box::new(SsaValue::Expr(r.to_string())) }; },
                                ArithOp::Mod => if r != 0 { l % r } else { return SsaValue::ArithBinOp { left: Box::new(SsaValue::Expr(l.to_string())), op: ArithOp::Mod, right: Box::new(SsaValue::Expr(r.to_string())) }; },
                                ArithOp::Eq => if l == r { 1 } else { 0 },
                                ArithOp::Ne => if l != r { 1 } else { 0 },
                                ArithOp::Lt => if l < r { 1 } else { 0 },
                                ArithOp::Le => if l <= r { 1 } else { 0 },
                                ArithOp::Gt => if l > r { 1 } else { 0 },
                                ArithOp::Ge => if l >= r { 1 } else { 0 },
                                ArithOp::And => if l != 0 && r != 0 { 1 } else { 0 },
                                ArithOp::Or => if l != 0 || r != 0 { 1 } else { 0 },
                                ArithOp::Not => 0, // shouldn't appear as binary
                                ArithOp::Shl => l << r,
                                ArithOp::Shr => l >> r,
                                ArithOp::BitAnd => l & r,
                                ArithOp::BitOr => l | r,
                                ArithOp::BitXor => l ^ r,
                                ArithOp::Pow => l.pow(r as u32),
                            };
                            return SsaValue::Expr(result.to_string());
                        }
                        (Err(_), Ok(operand)) => {
                        if l_is_simple {
                            // DD#58: check if lhs is a known int function
                            if state.function_int_typed.contains(lhs) {
                                return SsaValue::ArithBinOp {
                                    left: Box::new(SsaValue::IntFuncCall {
                                        name: lhs.to_string(),
                                        args: vec![],
                                    }),
                                    op,
                                    right: Box::new(SsaValue::Expr(operand.to_string())),
                                };
                            }
                            return SsaValue::ArithExpr {
                                var: state.lookup(lhs),
                                op,
                                operand,
                            };
                        } else {
                            return SsaValue::ArithBinOp {
                                left: Box::new(to_val(lhs)),
                                op,
                                right: Box::new(SsaValue::Expr(operand.to_string())),
                            };
                        }
                    }
                    (Ok(l_val), Err(_)) => {
                        if r_is_simple {
                            match op {
                                ArithOp::Add | ArithOp::Mul
                                | ArithOp::Eq | ArithOp::Ne
                                | ArithOp::And | ArithOp::Or => {
                                    return SsaValue::ArithExpr {
                                        var: state.lookup(rhs),
                                        op,
                                        operand: l_val,
                                    };
                                }
                                _ => {
                                    return SsaValue::ArithBinOp {
                                        left: Box::new(SsaValue::Expr(l_val.to_string())),
                                        op,
                                        right: Box::new(SsaValue::Var(state.lookup(rhs))),
                                    };
                                }
                            }
                        } else {
                            return SsaValue::ArithBinOp {
                                left: Box::new(SsaValue::Expr(l_val.to_string())),
                                op,
                                right: Box::new(to_val(rhs)),
                            };
                        }
                    }
                    (Err(_), Err(_)) => {
                        let left = if l_is_simple {
                            SsaValue::Var(state.lookup(lhs))
                        } else {
                            to_val(lhs)
                        };
                        let right = if r_is_simple {
                            SsaValue::Var(state.lookup(rhs))
                        } else {
                            to_val(rhs)
                        };
                        return SsaValue::ArithBinOp {
                            left: Box::new(left),
                            op,
                            right: Box::new(right),
                        };
                    }
                }
            }
            // Fallback: treat as literal string
            SsaValue::Expr(format!("$(({}))", s))
        }
        ExprKind::ParameterExpansion(pe) => {
            let var = state.lookup(&pe.name);
            if let Some(ref op) = pe.operator {
                let operand = pe.operand.as_ref()
                    .map(|e| expr_to_string(e))
                    .unwrap_or_default();
                SsaValue::ParamExpansion {
                    var,
                    operator: op.clone(),
                    operand,
                }
            } else {
                SsaValue::Var(var)
            }
        }
        ExprKind::DoubleQuoted(parts) => {
            // Unwrap single-element DoubleQuoted to its inner value.
            if parts.len() == 1 {
                return expr_to_value(&parts[0], state);
            }
            // Multi-part: check if all literal
            let mut all_literal = true;
            let mut lit_concat = String::new();
            for part in parts {
                match &part.kind {
                    ExprKind::Literal(lit) => lit_concat.push_str(lit),
                    _ => { all_literal = false; break; }
                }
            }
            if all_literal {
                SsaValue::Literal(lit_concat)
            } else {
                // Build a Concat of resolved parts
                let mut concat_parts = Vec::new();
                for part in parts {
                    concat_parts.push(expr_to_value(part, state));
                }
                SsaValue::Concat(concat_parts)
            }
        }
        ExprKind::AssocArray(pairs) => {
            let vals: Vec<(String, SsaValue)> = pairs.iter()
                .map(|(k, v)| (k.clone(), expr_to_value(v, state)))
                .collect();
            SsaValue::AssocArray(vals)
        }
        ExprKind::Subscript { name, index } => {
            let is_integer = !state.assoc_arrays.contains(name);
            SsaValue::Subscript {
                array: name.clone(),
                index: Box::new(expr_to_value(index, state)),
                is_integer_key: is_integer,
            }
        }
        ExprKind::Concat(parts) => {
            let vals: Vec<SsaValue> = parts.iter().map(|p| expr_to_value(p, state)).collect();
            if vals.len() == 1 {
                return vals.into_iter().next().unwrap();
            }
            SsaValue::Concat(vals)
        }
        ExprKind::Array(elems) => {
            let vals: Vec<SsaValue> = elems.iter()
                .map(|e| expr_to_value(e, state))
                .collect();
            // Check for regex filter pattern: (* =~ 'regex')
            // vals would be ["*", "=~", "pattern"]
            if vals.len() == 3 {
                if let (SsaValue::Literal(star), SsaValue::Literal(op), SsaValue::Literal(pat)) = (&vals[0], &vals[1], &vals[2]) {
                    if star == "*" && op == "=~" {
                        return SsaValue::ParamExpansion {
                            var: VarRef::new(".", 0),
                            operator: "glob_regex".to_string(),
                            operand: pat.clone(),
                        };
                    }
                }
            }
            // Check if any element is a glob pattern (contains *)
            let has_glob = vals.iter().any(|v| {
                if let SsaValue::Literal(s) = v { s.contains('*') } else { false }
            });
            if has_glob && vals.len() == 1 {
                // Single glob pattern: arr=(*.sh) → sash_glob(".", "*.sh")
                SsaValue::ParamExpansion {
                    var: VarRef::new(".", 0),
                    operator: "glob".to_string(),
                    operand: if let SsaValue::Literal(s) = &vals[0] { s.clone() } else { String::new() },
                }
            } else {
                SsaValue::IndexedArray(vals)
            }
        }
        _ => {
            // Handle CommandSubstitution → FuncCall if it's a known function,
            // or CmdSubst for runtime capture
            if let ExprKind::CommandSubstitution(stmts) = &expr.kind {
                // Filter out comments — they're not real statements
                let real_stmts: Vec<&Statement> = stmts.iter()
                    .filter(|s| !matches!(s.kind, StmtKind::Comment(_)))
                    .collect();
                if real_stmts.len() == 1 {
                    // Unwrap Redirected or Suppressed to get the inner Command
                    let (inner, _is_suppressed) = {
                        let mut s = &real_stmts[0].kind;
                        let mut suppressed = false;
                        loop {
                            match s {
                                StmtKind::Redirected(r) => { s = &r.body.kind; }
                                StmtKind::Suppressed(inner) => { s = &inner.kind; suppressed = true; }
                                other => break (other, suppressed),
                            }
                        }
                    };
                    if let StmtKind::Command(c) = inner {
                        if let ExprKind::Literal(name) = &c.name.kind {
                            if state.known_functions.contains(name.as_str()) {
                                return SsaValue::FuncCall(name.clone());
                            }
                            // Sash-native builtins: len, keys (DD#41)
                            if name == "len" {
                                if let Some(arg) = c.args.first() {
                                    let arg_str = expr_to_string(arg);
                                    if arg_str.starts_with('$') {
                                        // len $var → string length
                                        let v = state.lookup(&arg_str[1..]);
                                        return SsaValue::ParamExpansion {
                                            var: v,
                                            operator: "strlen".to_string(),
                                            operand: String::new(),
                                        };
                                    } else {
                                        // len arr → array length
                                        let v = state.lookup(&arg_str);
                                        return SsaValue::ParamExpansion {
                                            var: v,
                                            operator: "arrlen".to_string(),
                                            operand: String::new(),
                                        };
                                    }
                                }
                            }
                            if name == "keys" {
                                if let Some(arg) = c.args.first() {
                                    let arg_str = expr_to_string(arg);
                                    let v = state.lookup(&arg_str);
                                    return SsaValue::ParamExpansion {
                                        var: v,
                                        operator: "arrkeys".to_string(),
                                        operand: String::new(),
                                    };
                                }
                            }
                            if name == "join" {
                                // join @arr → space-separated
                                // join sep @arr → sep-separated
                                let (sep, arr_expr) = if c.args.len() >= 2 {
                                    // Strip quotes from separator
                                    let sep_raw = match &c.args[0].kind {
                                        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => s.clone(),
                                        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => {
                                            match &parts[0].kind {
                                                ExprKind::Literal(s) => s.clone(),
                                                _ => expr_to_string(&c.args[0]),
                                            }
                                        }
                                        _ => expr_to_string(&c.args[0]),
                                    };
                                    (sep_raw, &c.args[1])
                                } else if c.args.len() == 1 {
                                    (" ".to_string(), &c.args[0])
                                } else {
                                    panic!("ICE: join requires at least one argument");
                                };
                                // The array arg should be @arr → ParameterExpansion(arrvals)
                                let arr_name = match &arr_expr.kind {
                                    ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => pe.name.clone(),
                                    _ => expr_to_string(arr_expr),
                                };
                                let v = state.lookup(&arr_name);
                                return SsaValue::ParamExpansion {
                                    var: v,
                                    operator: "arrjoin".to_string(),
                                    operand: sep,
                                };
                            }
                        }
                        // DD#14: $(echo args) — build inline as concat
                        if let ExprKind::Literal(name) = &c.name.kind {
                          if name == "echo" {
                            if c.args.is_empty() {
                                return SsaValue::Literal(String::new());
                            }
                            let mut parts: Vec<SsaValue> = Vec::new();
                            for (i, arg) in c.args.iter().enumerate() {
                                if i > 0 {
                                    parts.push(SsaValue::Literal(" ".to_string()));
                                }
                                parts.push(expr_to_value(arg, state));
                            }
                            return if parts.len() == 1 {
                                parts.remove(0)
                            } else {
                                SsaValue::Concat(parts)
                            };
                          }
                        }

                        // Not an elidable function — emit CmdSubst with argv
                        let mut argv = Vec::new();
                        argv.push(expr_to_value(&c.name, state));
                        for arg in &c.args {
                            argv.push(expr_to_value(arg, state));
                        }
                        // Capture redirects from Redirected wrapper
                        let redirects = match &real_stmts[0].kind {
                            StmtKind::Redirected(r) => r.redirects.clone(),
                            _ => vec![],
                        };
                        return SsaValue::CmdSubst { argv, redirects };
                    }
                }
            }
            SsaValue::Expr(expr_to_string(expr))
        }
    }
}

/// Convert an expression to a simple string representation.
fn expr_to_string(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => s.clone(),
        ExprKind::SimpleExpansion(var) => format!("${}", var),
        ExprKind::SingleQuoted(s) => format!("'{}'", s),
        ExprKind::DoubleQuoted(parts) => {
            let mut s = String::from("\"");
            for p in parts {
                s.push_str(&expr_to_string(p));
            }
            s.push('"');
            s
        }
        ExprKind::ParameterExpansion(pe) => {
            let mut s = format!("${{{}", pe.name);
            if let Some(op) = &pe.operator {
                s.push_str(op);
            }
            s.push('}');
            s
        }
        ExprKind::CommandSubstitution(_) => "$(...)".to_string(),
        ExprKind::ArithmeticExpansion(e) => format!("$(({}))", e),
        ExprKind::Concat(parts) => parts.iter().map(|p| expr_to_string(p)).collect(),
        ExprKind::Array(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| expr_to_string(e)).collect();
            format!("({})", inner.join(" "))
        }
        ExprKind::AssocArray(pairs) => {
            let inner: Vec<String> = pairs.iter()
                .map(|(k, v)| format!("[{}]={}", k, expr_to_string(v)))
                .collect();
            format!("({})", inner.join(" "))
        }
        ExprKind::Subscript { name, index } => {
            format!("${{{}[{}]}}", name, expr_to_string(index))
        }
        ExprKind::Test(_) => "[[ ... ]]".to_string(),
    }
}

/// Collect variable names declared with `local` in a statement list.
fn collect_local_vars(stmts: &[Statement]) -> Vec<String> {
    let mut locals = Vec::new();
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Declaration(d) if d.keyword == DeclKeyword::Local => {
                for a in &d.assignments {
                    if !locals.contains(&a.name) {
                        locals.push(a.name.clone());
                    }
                }
            }
            _ => {}
        }
    }
    locals
}

/// Scan function body for the highest positional parameter referenced ($1..$9).
/// Returns 0 if none found.
fn collect_max_positional(stmts: &[Statement]) -> usize {
    let mut max = 0usize;
    let mut shifts = 0usize;
    for stmt in stmts {
        scan_expr_positionals_stmt(stmt, &mut max, &mut shifts);
    }
    // After `shift` N times, a read of $1 actually needs argc >= 1+N
    max + shifts
}

fn scan_expr_positionals_stmt(stmt: &Statement, max: &mut usize, shifts: &mut usize) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            // Detect `shift` command
            let name_text = match &c.name.kind {
                ExprKind::Literal(s) => s.as_str(),
                _ => "",
            };
            if name_text == "shift" {
                // shift with no args shifts by 1; shift N shifts by N
                let n = c.args.first()
                    .and_then(|a| if let ExprKind::Literal(s) = &a.kind { s.parse::<usize>().ok() } else { None })
                    .unwrap_or(1);
                *shifts += n;
            }
            scan_expr_positionals_expr(&c.name, max);
            for a in &c.args { scan_expr_positionals_expr(a, max); }
        }
        StmtKind::Assignment(a) => {
            scan_expr_positionals_expr(&a.value, max);
        }
        StmtKind::Declaration(d) => {
            for a in &d.assignments { scan_expr_positionals_expr(&a.value, max); }
        }
        StmtKind::Pipeline(p) => {
            for s in &p.commands { scan_expr_positionals_stmt(s, max, shifts); }
        }
        StmtKind::If(i) => {
            for s in &i.condition { scan_expr_positionals_stmt(s, max, shifts); }
            for s in &i.then_body { scan_expr_positionals_stmt(s, max, shifts); }
            if let Some(ref eb) = i.else_body { for s in eb { scan_expr_positionals_stmt(s, max, shifts); } }
        }
        StmtKind::While(w) => {
            for s in &w.condition { scan_expr_positionals_stmt(s, max, shifts); }
            for s in &w.body { scan_expr_positionals_stmt(s, max, shifts); }
        }
        StmtKind::For(f) => {
            for w in &f.words { scan_expr_positionals_expr(w, max); }
            for s in &f.body { scan_expr_positionals_stmt(s, max, shifts); }
        }
        StmtKind::Case(c) => {
            scan_expr_positionals_expr(&c.value, max);
            for item in &c.items {
                for s in &item.body { scan_expr_positionals_stmt(s, max, shifts); }
            }
        }
        StmtKind::Function(_) => {} // don't recurse into nested functions
        StmtKind::Redirected(r) => {
            scan_expr_positionals_stmt(&r.body, max, shifts);
        }
        StmtKind::List(l) => {
            scan_expr_positionals_stmt(&l.left, max, shifts);
            scan_expr_positionals_stmt(&l.right, max, shifts);
        }
        StmtKind::Subshell(s) => {
            for st in &s.body { scan_expr_positionals_stmt(st, max, shifts); }
        }
        StmtKind::Group(g) => {
            for st in &g.body { scan_expr_positionals_stmt(st, max, shifts); }
        }
        StmtKind::Comment(_) => {}
        StmtKind::Suppressed(inner) => {
            scan_expr_positionals_stmt(inner, max, shifts);
        }
    }
}

fn scan_expr_positionals_expr(expr: &Expr, max: &mut usize) {
    match &expr.kind {
        ExprKind::SimpleExpansion(name) => {
            if let Ok(n) = name.parse::<usize>() {
                if n >= 1 && n <= 9 && n > *max { *max = n; }
            }
        }
        ExprKind::DoubleQuoted(parts) => {
            for p in parts { scan_expr_positionals_expr(p, max); }
        }
        ExprKind::Concat(parts) => {
            for p in parts { scan_expr_positionals_expr(p, max); }
        }
        ExprKind::Array(elems) => {
            for e in elems { scan_expr_positionals_expr(e, max); }
        }
        ExprKind::ParameterExpansion(pe) => {
            if let Ok(n) = pe.name.parse::<usize>() {
                if n >= 1 && n <= 9 && n > *max { *max = n; }
            }
            if let Some(ref op) = pe.operand {
                scan_expr_positionals_expr(op, max);
            }
        }
        ExprKind::CommandSubstitution(stmts) => {
            let mut sub_shifts = 0;
            for s in stmts { scan_expr_positionals_stmt(s, max, &mut sub_shifts); }
        }
        ExprKind::Test(t) => {
            scan_test_positionals(t, max);
        }
        ExprKind::Subscript { index, .. } => {
            scan_expr_positionals_expr(index, max);
        }
        _ => {}
    }
}

fn scan_test_positionals(test: &TestExpr, max: &mut usize) {
    match test {
        TestExpr::Unary { operand, .. } => scan_expr_positionals_expr(operand, max),
        TestExpr::Binary { left, right, .. } => {
            scan_expr_positionals_expr(left, max);
            scan_expr_positionals_expr(right, max);
        }
        TestExpr::Not(inner) => scan_test_positionals(inner, max),
        TestExpr::And(a, b) | TestExpr::Or(a, b) => {
            scan_test_positionals(a, max);
            scan_test_positionals(b, max);
        }
        TestExpr::Paren(inner) => scan_test_positionals(inner, max),
    }
}

/// Desugar `for x in "$@"` into a while loop over positional params:
///   __fi = 0
///   while __fi < $#:
///     x = argv[__fi + 1]
///     body
///     __fi = __fi + 1
/// Lower `for x in "${arr[@]}"` — iterate over table entries by index.
/// Same two-pass SSA pattern as lower_for_dollar_at.
fn lower_for_array(
    f: &For,
    array_name: &str,
    use_keys: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let header = ssa.new_block("for_arr.header");
    let body_block = ssa.new_block("for_arr.body");
    let exit_block = ssa.new_block("for_arr.exit");

    // __ai = 0
    let idx_var = state.define("__ai");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: idx_var,
        value: SsaValue::Literal("0".to_string()),
    });

    ssa.block_mut(block).terminator = Terminator::Goto(header);
    let pre_state = state.snapshot();

    // Condition: __ai < sash_table_len(arr)
    // We use arrlen as a ParamExpansion that codegen already handles
    let cond_test = crate::ir::TestExpr::Binary {
        left: Box::new(Expr { kind: ExprKind::SimpleExpansion("__ai".to_string()), span: Span::default() }),
        op: "-lt".to_string(),
        right: Box::new(Expr {
            kind: ExprKind::ParameterExpansion(crate::ir::ParameterExpansion {
                name: array_name.to_string(),
                operator: Some("arrlen".to_string()),
                operand: None,
            }),
            span: Span::default(),
        }),
    };
    ssa.block_mut(header).insts.push(SsaInst::Test(cond_test.clone()));
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // x = table_value(arr, __ai)
    let idx_ref = state.lookup("__ai");
    let loop_var = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var,
        value: if use_keys {
            SsaValue::TableKey { array: array_name.to_string(), index: idx_ref.clone() }
        } else {
            SsaValue::TableValue { array: array_name.to_string(), index: idx_ref.clone() }
        },
    });

    let body_exit_1 = lower_stmts(&f.body, ssa, state, body_block);

    // __ai = __ai + 1
    let idx_cur = state.lookup("__ai");
    let idx_next = state.define("__ai");
    ssa.block_mut(body_exit_1).insts.push(SsaInst::Assign {
        target: idx_next,
        value: SsaValue::ArithExpr { var: idx_cur, op: ArithOp::Add, operand: 1 },
    });
    let body_state_1 = state.snapshot();

    insert_phis_two_branch(
        state, ssa, header,
        block, &pre_state,
        body_exit_1, &body_state_1,
    );

    // --- Pass 2 ---
    ssa.block_mut(header).insts.clear();
    ssa.block_mut(body_block).insts.clear();

    ssa.block_mut(header).insts.push(SsaInst::Test(cond_test));
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    let idx_ref_2 = state.lookup("__ai");
    let loop_var_2 = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var_2,
        value: if use_keys {
            SsaValue::TableKey { array: array_name.to_string(), index: idx_ref_2.clone() }
        } else {
            SsaValue::TableValue { array: array_name.to_string(), index: idx_ref_2.clone() }
        },
    });

    let body_exit_2 = lower_stmts(&f.body, ssa, state, body_block);

    let idx_cur_2 = state.lookup("__ai");
    let idx_next_2 = state.define("__ai");
    ssa.block_mut(body_exit_2).insts.push(SsaInst::Assign {
        target: idx_next_2,
        value: SsaValue::ArithExpr { var: idx_cur_2, op: ArithOp::Add, operand: 1 },
    });

    ssa.block_mut(body_exit_2).terminator = Terminator::Goto(header);

    let body_state_2 = state.snapshot();
    for phi in ssa.block_mut(header).phis.iter_mut() {
        let var_name = &phi.target.name;
        if let Some(ver) = body_state_2.get(var_name) {
            phi.sources[1].1 = VarRef::new(var_name, *ver);
        }
    }

    exit_block
}

fn lower_for_dollar_at(
    f: &For,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let header = ssa.new_block("for_at.header");
    let body_block = ssa.new_block("for_at.body");
    let exit_block = ssa.new_block("for_at.exit");

    // Initialize index: __fi = 0
    let idx_var = state.define("__fi");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: idx_var,
        value: SsaValue::Literal("0".to_string()),
    });

    ssa.block_mut(block).terminator = Terminator::Goto(header);

    let pre_state = state.snapshot();

    // --- Pass 1: discover modified variables ---
    // Emit condition: __fi < $#
    let _argc_ref = state.lookup("#");
    let cond_test = crate::ir::TestExpr::Binary {
        left: Box::new(Expr { kind: ExprKind::SimpleExpansion("__fi".to_string()), span: Span::default() }),
        op: "-lt".to_string(),
        right: Box::new(Expr { kind: ExprKind::SimpleExpansion("#".to_string()), span: Span::default() }),
    };
    ssa.block_mut(header).insts.push(SsaInst::Test(cond_test.clone()));
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // Assign loop variable: x = argv[__fi]
    let idx_ref = state.lookup("__fi");
    let loop_var = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var,
        value: SsaValue::ArgvIndex(idx_ref.clone()),
    });

    // Lower body
    let body_exit_1 = lower_stmts(&f.body, ssa, state, body_block);

    // Increment index: __fi = __fi + 1
    let idx_cur = state.lookup("__fi");
    let idx_next = state.define("__fi");
    ssa.block_mut(body_exit_1).insts.push(SsaInst::Assign {
        target: idx_next,
        value: SsaValue::ArithExpr { var: idx_cur, op: ArithOp::Add, operand: 1 },
    });
    let body_state_1 = state.snapshot();

    // Insert phis at header
    insert_phis_two_branch(
        state, ssa, header,
        block, &pre_state,
        body_exit_1, &body_state_1,
    );

    // --- Pass 2: re-lower with phi-merged versions ---
    ssa.block_mut(header).insts.clear();
    ssa.block_mut(body_block).insts.clear();

    // Re-emit condition
    ssa.block_mut(header).insts.push(SsaInst::Test(cond_test));
    ssa.block_mut(header).terminator = Terminator::Loop {
        condition: "condition".to_string(),
        body_block,
        exit_block,
    };

    // Re-assign loop variable from argv
    let idx_ref_2 = state.lookup("__fi");
    let loop_var_2 = state.define(&f.variable);
    ssa.block_mut(body_block).insts.push(SsaInst::Assign {
        target: loop_var_2,
        value: SsaValue::ArgvIndex(idx_ref_2.clone()),
    });

    // Re-lower body
    let body_exit_2 = lower_stmts(&f.body, ssa, state, body_block);

    // Re-increment
    let idx_cur_2 = state.lookup("__fi");
    let idx_next_2 = state.define("__fi");
    ssa.block_mut(body_exit_2).insts.push(SsaInst::Assign {
        target: idx_next_2,
        value: SsaValue::ArithExpr { var: idx_cur_2, op: ArithOp::Add, operand: 1 },
    });

    ssa.block_mut(body_exit_2).terminator = Terminator::Goto(header);

    // Update phi sources
    let body_state_2 = state.snapshot();
    for phi in ssa.block_mut(header).phis.iter_mut() {
        let var_name = &phi.target.name;
        if let Some(ver) = body_state_2.get(var_name) {
            phi.sources[1].1 = VarRef::new(var_name, *ver);
        }
    }

    exit_block
}

// ── Pipe-while-read lowering ──────────────────────────────────

/// Lower `cmd | while read -r var; do body; done`
/// The Pipeline instruction with capture_read_fd=true precedes this.
/// This lowers the while-read loop with fd=-1 (sentinel for pipe fd).
fn lower_pipe_while_read(
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if let StmtKind::While(w) = &stmt.kind {
        // The while condition should be `read -r var` — extract the var name
        // and reuse the standard while lowering, but patch ReadLine fd to -1
        let exit = lower_while(w, ssa, state, block);

        // Patch all ReadLine instructions in the loop to use fd=-1 (pipe fd sentinel)
        // Walk all blocks from header (block+1) to exit
        for bid in ssa.blocks.keys().cloned().collect::<Vec<_>>() {
            for inst in &mut ssa.block_mut(bid).insts {
                match inst {
                    SsaInst::ReadLine { fd, .. } if *fd == 0 => { *fd = -1; }
                    SsaInst::ReadLineMulti { fd, .. } if *fd == 0 => { *fd = -1; }
                    _ => {}
                }
            }
        }

        exit
    } else {
        // Shouldn't happen — caller verified this is a While
        lower_one(stmt, ssa, state, block)
    }
}

/// Parse an arithmetic binary expression like "i+1", "i + 1", "$i+1", "a*b"
/// Returns (lhs, op, rhs) or ("", None, "") if not parseable.
/// DD#58: Try to parse a sexpr function call inside arithmetic context.
/// `inner` is the content between outer parens: "add 3 4" or "f" or "add (dbl 3) 4"
/// Returns Some(IntFuncCall) if the first token is a known -> int function.
/// Resolve a bare command name to an absolute path for pipeline stages.
/// Builtins that are also available as external commands get resolved.
/// Returns the name unchanged if it already has a path or is an inline builtin.
fn resolve_pipeline_command(name: &str) -> String {
    if name.contains('/') {
        return name.to_string();
    }
    // Inline builtins handled directly in codegen — no resolution needed
    if matches!(name, "echo" | "true" | "false" | ":") {
        return name.to_string();
    }
    // DD#23: compile-time PATH resolution
    resolve_command_path(name).unwrap_or_else(|| name.to_string())
}

/// DD#23: Search PATH for a command, return absolute path.
/// Respects --sysroot for cross-compilation.
fn resolve_command_path(name: &str) -> Option<String> {
    let sysroot = crate::get_sysroot();
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        let target_path = format!("{}/{}", dir, name);
        let exists = if let Some(ref sr) = sysroot {
            crate::sysroot_file_exists(sr, &target_path)
        } else {
            std::path::Path::new(&target_path).is_file()
        };
        if exists {
            return Some(target_path);
        }
    }
    None
}

fn try_parse_sexpr(inner: &str, span: Span, state: &VersionState) -> Option<SsaValue> {
    let inner = inner.trim();
    if inner.is_empty() { return None; }

    // Extract the function name (first whitespace-delimited token)
    let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
    let name = &inner[..name_end];

    // Must be a known -> int function
    if !state.function_int_typed.contains(name) {
        return None;
    }

    // Parse arguments (space-separated, respecting nested parens)
    let args_str = inner[name_end..].trim();
    let args = if args_str.is_empty() {
        vec![]
    } else {
        parse_sexpr_args(args_str, span, state)
    };

    Some(SsaValue::IntFuncCall {
        name: name.to_string(),
        args,
    })
}

/// Parse space-separated arguments for a sexpr call, respecting nested parens.
fn parse_sexpr_args(s: &str, span: Span, state: &VersionState) -> Vec<SsaValue> {
    let mut args = Vec::new();
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        // Skip whitespace
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i >= bytes.len() { break; }

        if bytes[i] == b'(' {
            // Find matching close paren
            let start = i;
            let mut depth = 0;
            while i < bytes.len() {
                if bytes[i] == b'(' { depth += 1; }
                if bytes[i] == b')' { depth -= 1; if depth == 0 { i += 1; break; } }
                i += 1;
            }
            let token = s[start..i].trim();
            // Try as nested sexpr
            let inner = &token[1..token.len()-1];
            if let Some(call) = try_parse_sexpr(inner, span, state) {
                args.push(call);
            } else {
                // Arithmetic grouping: evaluate as arithmetic expression
                let e = Expr { kind: ExprKind::ArithmeticExpansion(inner.to_string()), span };
                args.push(expr_to_value(&e, state));
            }
        } else {
            // Read until whitespace or paren
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'(' { i += 1; }
            let token = &s[start..i];
            // Parse as arithmetic expression (number or variable)
            let e = Expr { kind: ExprKind::ArithmeticExpansion(token.to_string()), span };
            args.push(expr_to_value(&e, state));
        }
    }
    args
}

fn parse_arith_binop(s: &str) -> (&str, Option<ArithOp>, &str) {
    let bytes = s.as_bytes();
    let start = if bytes.first() == Some(&b'-') { 1 } else { 0 };
    // Check two-char operators first: ==, !=, <=, >=, ||, &&
    // Skip operators inside parentheses
    let mut depth = 0i32;
    for i in start..bytes.len().saturating_sub(1) {
        if bytes[i] == b'(' { depth += 1; continue; }
        if bytes[i] == b')' { depth -= 1; continue; }
        if depth > 0 { continue; }
        let two = &s[i..i+2];
        if let Some(op) = ArithOp::from_str(two) {
            let lhs = s[..i].trim();
            let rhs = s[i+2..].trim();
            if !lhs.is_empty() && !rhs.is_empty() {
                return (lhs, Some(op), rhs);
            }
        }
    }
    // Single-char operators: +, -, *, /, %, <, >
    depth = 0;
    for (i, &b) in bytes[start..].iter().enumerate() {
        let pos = i + start;
        if b == b'(' { depth += 1; continue; }
        if b == b')' { depth -= 1; continue; }
        if depth > 0 { continue; }
        let op_str = &s[pos..pos+1];
        if let Some(op) = ArithOp::from_str(op_str) {
            let lhs = s[..pos].trim();
            let rhs = s[pos+1..].trim();
            if !lhs.is_empty() && !rhs.is_empty() {
                return (lhs, Some(op), rhs);
            }
        }
    }
    // Check if the whole thing is parenthesized: (expr)
    let trimmed = s.trim();
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        // Strip outer parens and recurse
        let inner = &trimmed[1..trimmed.len()-1];
        return parse_arith_binop(inner);
    }
    (s, None, "")
}

/// Collect return values from a function body's IR statements.
/// Collect return values from a function body's IR statements.
/// Returns Some("N") for literal returns, Some("") for bare return, None for computed.
fn collect_returns(stmts: &[Statement]) -> Vec<Option<String>> {
    let mut returns = Vec::new();
    collect_returns_inner(stmts, &mut returns);
    returns
}

fn collect_returns_inner(stmts: &[Statement], returns: &mut Vec<Option<String>>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Command(c) => {
                let name_text = match &c.name.kind {
                    ExprKind::Literal(s) => s.as_str(),
                    _ => "",
                };
                if name_text == "return" {
                    let val = if c.args.is_empty() {
                        Some(String::new()) // bare return = implicit 0
                    } else if let Some(ExprKind::Literal(s)) = c.args.first().map(|a| &a.kind) {
                        Some(s.clone()) // literal return value
                    } else {
                        None // computed return value — assume non-trivial
                    };
                    returns.push(val);
                }
            }
            StmtKind::If(i) => {
                collect_returns_inner(&i.condition, returns);
                collect_returns_inner(&i.then_body, returns);
                if let Some(ref eb) = i.else_body {
                    collect_returns_inner(eb, returns);
                }
            }
            StmtKind::While(w) => {
                collect_returns_inner(&w.condition, returns);
                collect_returns_inner(&w.body, returns);
            }
            StmtKind::For(f) => {
                collect_returns_inner(&f.body, returns);
            }
            StmtKind::Case(c) => {
                for item in &c.items {
                    collect_returns_inner(&item.body, returns);
                }
            }
            StmtKind::Group(g) => {
                collect_returns_inner(&g.body, returns);
            }
            StmtKind::Pipeline(p) => {
                for s in &p.commands {
                    collect_returns_inner(std::slice::from_ref(s), returns);
                }
            }
            StmtKind::List(l) => {
                collect_returns_inner(std::slice::from_ref(&l.left), returns);
                collect_returns_inner(std::slice::from_ref(&l.right), returns);
            }
            StmtKind::Redirected(r) => {
                collect_returns_inner(std::slice::from_ref(&r.body), returns);
            }
            _ => {}
        }
    }
}
