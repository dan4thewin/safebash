//! Function-related transformations: int-func lifting, inlining, helpers.

use crate::ir::*;
use crate::ssa::*;

use super::helpers::scan_expr_positionals_stmt;
use super::statements::lower_stmts;

pub(super) fn lift_int_func_calls(
    val: SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    match val {
        SsaValue::IntFuncCall { name, args } => {
            // Inline the function and capture return value
            let lifted_args: Vec<SsaValue> = args
                .into_iter()
                .map(|a| lift_int_func_calls(a, ssa, state, block))
                .collect();
            inline_int_func(&name, &lifted_args, ssa, state, block)
        }
        SsaValue::ArithBinOp { left, op, right } => {
            let l = lift_int_func_calls(*left, ssa, state, block);
            let r = lift_int_func_calls(*right, ssa, state, block);
            SsaValue::ArithBinOp {
                left: Box::new(l),
                op,
                right: Box::new(r),
            }
        }
        SsaValue::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            let c = lift_int_func_calls(*cond, ssa, state, block);
            let t = lift_int_func_calls(*true_val, ssa, state, block);
            let f = lift_int_func_calls(*false_val, ssa, state, block);
            SsaValue::Ternary {
                cond: Box::new(c),
                true_val: Box::new(t),
                false_val: Box::new(f),
            }
        }
        SsaValue::Concat(parts) => {
            // A double-quoted string with an embedded `$(f)` / `$((f))` lowers
            // to a Concat; lift each part so a nested IntFuncCall materializes.
            let lifted = parts
                .into_iter()
                .map(|p| lift_int_func_calls(p, ssa, state, block))
                .collect();
            SsaValue::Concat(lifted)
        }
        other => other, // no IntFuncCall inside
    }
}

/// Option C (DD#55): materialize any `IntFuncCall` (produced by `$(f)` /
/// `$((f))` on an `-> int` function) in a command's argument list into a temp
/// variable via [`lift_int_func_calls`]. An `IntFuncCall` that reaches codegen
/// unlifted is an ICE, so every arg-building site that can carry a `$(f)`
/// capture routes its argv through this.
pub(super) fn lift_int_func_args(
    args: Vec<SsaValue>,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> Vec<SsaValue> {
    args.into_iter()
        .map(|v| lift_int_func_calls(v, ssa, state, block))
        .collect()
}

/// Inline a -> int function call, returning an SsaValue::Var pointing
/// to the temp variable holding the return value.
pub(super) fn inline_int_func(
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
    let saved_locals: Vec<(String, VarRef)> = locals
        .iter()
        .map(|lname| {
            let save_ref = state.lookup(lname);
            (lname.clone(), save_ref)
        })
        .collect();

    // Set argc
    let argc_val = SsaValue::Literal(args.len().to_string());
    let argc_var = state.define("#");
    ssa.block_mut(*block).insts.push(SsaInst::Assign {
        target: argc_var,
        value: argc_val,
    });

    // Set positional params from args
    for (i, arg) in args.iter().enumerate() {
        let param_var = state.define(&(i + 1).to_string());
        ssa.block_mut(*block).insts.push(SsaInst::Assign {
            target: param_var,
            value: arg.clone(),
        });
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

pub(super) fn collect_local_vars(stmts: &[Statement]) -> Vec<String> {
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
pub(super) fn collect_max_positional(stmts: &[Statement]) -> usize {
    let mut max = 0usize;
    let mut shifts = 0usize;
    for stmt in stmts {
        scan_expr_positionals_stmt(stmt, &mut max, &mut shifts);
    }
    // After `shift` N times, a read of $1 actually needs argc >= 1+N
    max + shifts
}
pub(super) fn collect_returns(stmts: &[Statement]) -> Vec<Option<String>> {
    let mut returns = Vec::new();
    collect_returns_inner(stmts, &mut returns);
    returns
}

fn collect_returns_inner(stmts: &[Statement], returns: &mut Vec<Option<String>>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Command(c) => maybe_collect_return(c, returns),
            // Nested function definitions have their own returns.
            StmtKind::Function(_) => {}
            // Default: descend into every nested statement body via
            // child_bodies(). Covers If (incl. elif_clauses), While,
            // For, Case, Group, Pipeline, List, Redirected, Suppressed.
            // Leaf StmtKinds (Assignment, Declaration, Comment,
            // ArithTest) return empty bodies.
            _ => {
                for body in stmt.child_bodies() {
                    collect_returns_inner(body, returns);
                }
            }
        }
    }
}

/// If `c` is a `return` command, push its target into `returns`:
/// `None` = computed/non-trivial value, `Some(s)` = literal value
/// (empty string means bare `return` → implicit 0).
fn maybe_collect_return(c: &Command, returns: &mut Vec<Option<String>>) {
    if literal_command_name(&c.name) != "return" {
        return;
    }
    let val = if c.args.is_empty() {
        Some(String::new())
    } else if let Some(ExprKind::Literal(s)) = c.args.first().map(|a| &a.kind) {
        Some(s.clone())
    } else {
        None
    };
    returns.push(val);
}

fn literal_command_name(name: &Expr) -> &str {
    match &name.kind {
        ExprKind::Literal(s) => s.as_str(),
        _ => "",
    }
}
