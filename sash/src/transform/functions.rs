//! Function-related transformations: int-func lifting, inlining, helpers.

use crate::ir::*;
use crate::ssa::*;

use super::helpers::scan_expr_positionals_stmt;
use super::statements::lower_stmts;

/// DD#75 §5: bind the callee's implicit `args` array (0-indexed) from the
/// call's argument values, and mark `args` a collection for the inlined scope
/// so subscript/splat/length ops use the indexed-array path. `$1..$9`/`$@`/`$#`
/// desugar to reads of this array (see `simple_expansion_to_value`).
/// DD#75 §5: materialize the callee's implicit `args` array (0-indexed,
/// read-only) under a FRESH unique name, so nested/recursive calls never
/// collide — no save/restore of the array itself is needed. Records the name
/// as the current positional scope; `$1..$9`/`$@`/`$#` (and a user-written
/// `args`) resolve against it (see `positional_to_value`/`resolve_args_name`).
pub(super) fn setup_callee_args(
    arg_values: &[SsaValue],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    let name = format!("__args{}", state.args_counter);
    state.args_counter += 1;
    let v = state.define(&name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: v,
        value: SsaValue::IndexedArray(arg_values.to_vec()),
    });
    state.collections.insert(name.clone());
    state.current_args_name = name;
}

/// Save the *name* of the current positional scope (a cheap string — the array
/// itself is uniquely named, so nothing in SSA needs saving/restoring).
pub(super) fn save_caller_args(state: &mut VersionState) -> String {
    std::mem::take(&mut state.current_args_name)
}

pub(super) fn restore_caller_args(saved: String, state: &mut VersionState) {
    state.current_args_name = saved;
}

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
        SsaValue::FuncCall(name) => {
            // Option A (string-func-capture-in-arg-ice): a non-int function's
            // `$(f)` stdout capture in argument position. Reuse the assignment
            // path's capture-inline machinery (CaptureStart/End) to inline the
            // body and capture its stdout into a temp, then use the temp — so
            // it works for runtime output too, not just const-foldable stdout.
            // (Args are dropped upstream by `func_call_capture` for a string
            // `$(f)`, so none are threaded here; matches the pre-existing shape.)
            let temp = format!("__strcap_{name}");
            match super::stmt_handlers::capture_func_into(&temp, &name, &[], state, ssa, *block) {
                Some(new_block) => {
                    *block = new_block;
                    SsaValue::Var(state.lookup(&temp))
                }
                None => SsaValue::FuncCall(name),
            }
        }
        SsaValue::CmdSubstBody { body, .. } => {
            // A multi-statement/pipeline `$(…)` body: capture its stdout into a
            // temp (fresh name from the block count), then use the temp. The
            // capture is tainted (external output).
            let temp = format!("__bodycap_{}", ssa.blocks.len());
            *block = super::stmt_handlers::capture_body_into(&temp, &body, state, ssa, *block);
            SsaValue::Var(state.lookup(&temp))
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

    // DD#75 §5: positionals are backed by the `args` array (below), not scalar
    // "1".."9"/"#" vars — save/restore just `args`.
    let saved_args = save_caller_args(state);

    // Save local vars
    let saved_locals: Vec<(String, VarRef)> = locals
        .iter()
        .map(|lname| {
            let save_ref = state.lookup(lname);
            (lname.clone(), save_ref)
        })
        .collect();

    // DD#75 §5: materialize the implicit `args` array (0-indexed).
    setup_callee_args(args, ssa, state, *block);

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

    restore_caller_args(saved_args, state);

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
