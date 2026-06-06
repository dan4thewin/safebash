//! Statement lowering: IR Statement → SSA instructions.
//!
//! `lower_one` is the main dispatch — each StmtKind variant is handled.
//! SECURITY: This is where commands become SsaInst::Command with resolved names.

use super::control_flow::{flatten_elif, lower_case, lower_for, lower_if, lower_while};
use super::expressions::expr_to_value;
use super::functions::{collect_returns, lift_int_func_calls};
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
            super::stmt_handlers::handle_assignment(a, stmt, ssa, state, block)
        }
        StmtKind::Declaration(d) => lower_declaration_stmt(d, ssa, state, block),
        StmtKind::Command(c) => super::stmt_handlers::handle_command(c, stmt, ssa, state, block),
        StmtKind::If(i) => {
            let flattened = flatten_elif(i);
            lower_if(&flattened, ssa, state, block)
        }
        StmtKind::While(w) => lower_while_stmt(w, ssa, state, block),
        StmtKind::For(f) => lower_for(f, ssa, state, block),
        StmtKind::Pipeline(p) => super::stmt_handlers::handle_pipeline(p, stmt, ssa, state, block),
        StmtKind::List(l) => lower_list_stmt(l, stmt, ssa, state, block),
        StmtKind::Function(f) => lower_function_stmt(f, stmt, ssa, state, block),
        StmtKind::Redirected(r) => lower_redirected_stmt(r, ssa, state, block),
        StmtKind::Subshell(s) => lower_subshell_stmt(s, ssa, state, block),
        StmtKind::Group(g) => lower_stmts(&g.body, ssa, state, block),
        StmtKind::Case(c) => lower_case(c, ssa, state, block),
        StmtKind::Comment(_) => block,
        StmtKind::ArithTest(expr) => lower_arith_test_stmt(expr, stmt, ssa, state, &mut block),
        StmtKind::Suppressed(inner) => lower_suppressed_stmt(inner, ssa, state, block),
    }
}

/// `while`/`until`. For `until`, desugar to `while ! cond` by lowering as
/// `while` then patching the header block's last Test instruction with `Not`.
fn lower_while_stmt(
    w: &While,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if !w.until {
        return lower_while(w, ssa, state, block);
    }
    let mut desugared = w.clone();
    desugared.until = false;
    let exit = lower_while(&desugared, ssa, state, block);
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
}

/// Function definition. Registers the body for inline expansion at call sites,
/// then emits the body into a dedicated SSA block sequence terminating in Return.
/// Special cases: `__sash_end__` (DD#56 exit trap), `__local__*` (DD#18 local scope),
/// `-> int` return type tracking, DD#55 non-void detection + SB4017 emission.
fn lower_function_stmt(
    f: &Function,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if f.name == "__sash_end__" {
        state.exit_trap = Some(f.body.clone());
        return block;
    }
    state.function_bodies.insert(f.name.clone(), f.body.clone());
    if f.name.starts_with("__local__") {
        state.local_fn_scope.insert(f.name.clone());
    }
    if f.return_type == crate::ir::VarType::Int {
        state.function_int_typed.insert(f.name.clone());
    }
    let returns = collect_returns(&f.body);
    if !returns.is_empty() {
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
        let has_nonzero = returns.iter().any(|v| match v {
            Some(s) => !s.is_empty() && s != "0",
            None => true,
        });
        if has_nonzero && f.return_type == crate::ir::VarType::Int {
            state.function_non_void.insert(f.name.clone());
        }
    }
    let fn_entry = ssa.new_block(&format!("fn_{}", f.name));
    let saved_scope = state.local_fn_scope.clone();
    let fn_exit = lower_stmts(&f.body, ssa, state, fn_entry);
    state.local_fn_scope = saved_scope;
    ssa.block_mut(fn_exit).terminator = Terminator::Return;
    block
}

/// Statement wrapped in a redirect. For compound bodies (while/for/if/case/group/
/// subshell/function), bracket with __redirect_push/__redirect_pop. For simple
/// commands, attach the redirects to the last emitted Command/Pipeline/Read inst.
fn lower_redirected_stmt(
    r: &Redirected,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
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
        return lower_one(&r.body, ssa, state, block);
    }

    if is_compound && !r.redirects.is_empty() {
        ssa.block_mut(block).insts.push(SsaInst::Command {
            name: "__redirect_push".to_string(),
            kind: BuiltinCmd::RedirectPush,
            args: vec![],
            redirects: r.redirects.clone(),
            checked: true,
        });
        let result_block = lower_one(&r.body, ssa, state, block);
        ssa.block_mut(result_block).insts.push(SsaInst::Command {
            name: "__redirect_pop".to_string(),
            kind: BuiltinCmd::RedirectPop,
            args: vec![],
            redirects: r.redirects.clone(),
            checked: true,
        });
        return result_block;
    }

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

/// `(( expr ))` as a condition: evaluate arithmetic, set exit status based on
/// result (0 = true if nonzero, 1 = false if zero).
fn lower_arith_test_stmt(
    expr: &ArithExpr,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> BlockId {
    let ir_expr = crate::ir::Expr {
        kind: crate::ir::ExprKind::Arithmetic(Box::new(expr.clone())),
        span: stmt.span,
    };
    let val = expr_to_value(&ir_expr, state);
    let val = lift_int_func_calls(val, ssa, state, block);
    let target = state.define("__arith_test");
    ssa.block_mut(*block).insts.push(SsaInst::Assign {
        target: target.clone(),
        value: val,
    });
    ssa.block_mut(*block).insts.push(SsaInst::sigil(
        BuiltinCmd::LetExit,
        vec![SsaValue::Var(target)],
        true,
    ));
    *block
}

/// DD#55: `- cmd` (suppressed) — lower the inner statement, then mark the last
/// emitted Command/Pipeline instruction(s) as unchecked.
fn lower_suppressed_stmt(
    inner: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let pre_len = ssa.block(block).insts.len();
    let result_block = lower_one(inner, ssa, state, block);
    uncheck_block_from(ssa, block, pre_len);
    if result_block != block {
        for b in (block + 1)..=result_block {
            uncheck_block_from(ssa, b, 0);
        }
    }
    result_block
}

fn uncheck_block_from(ssa: &mut SsaProgram, block_id: BlockId, from: usize) {
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

fn lower_declaration_stmt(
    d: &Declaration,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    let is_array = d.flags.contains(&"-a".to_string());
    let is_assoc = d.flags.contains(&"-A".to_string());
    for a in &d.assignments {
        // Bare `declare var` (no =value) — don't reset existing variable
        let is_bare =
            matches!(&a.value.kind, ExprKind::Literal(s) if s.is_empty()) && !is_array && !is_assoc;
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

fn lower_list_stmt(
    l: &List,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
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
            ssa.block_mut(block).insts.push(SsaInst::sigil(
                BuiltinCmd::SubshellStart,
                vec![],
                true,
            ));
            ssa.block_mut(block).terminator = Terminator::Branch {
                condition: "__fork_pid".to_string(),
                true_block: parent_block, // parent continues
                false_block: child_block, // child runs command
            };

            // Store fork pid in $! variable
            let bg_target = state.define("!");
            ssa.block_mut(parent_block).insts.push(SsaInst::sigil(
                BuiltinCmd::StoreBgPid,
                vec![SsaValue::Var(bg_target)],
                true,
            ));

            // Child: run the command then _exit
            let child_end = lower_one(&l.left, ssa, state, child_block);
            ssa.block_mut(child_end).terminator = Terminator::Return;

            parent_block
        }
    }
}

fn lower_subshell_stmt(
    s: &Subshell,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // Subshell: fork child, run body in child, wait in parent.
    // Child gets its own memory space — modifications don't propagate.
    let child_block = ssa.new_block("subshell");
    let after_block = ssa.new_block("subshell_done");

    // Emit fork — codegen stores pid in test_result_var
    ssa.block_mut(block)
        .insts
        .push(SsaInst::sigil(BuiltinCmd::SubshellStart, vec![], true));
    // Branch: parent (pid>0) → after, child (pid==0) → body
    ssa.block_mut(block).terminator = Terminator::Branch {
        condition: "__fork_pid".to_string(),
        true_block: after_block,
        false_block: child_block,
    };

    // Parent: wait for child at start of after_block
    ssa.block_mut(after_block)
        .insts
        .push(SsaInst::sigil(BuiltinCmd::SubshellWait, vec![], true));

    // Snapshot SSA state — child modifications don't affect parent
    let snap = state.snapshot();

    // Child: run body then _exit(0)
    let body_exit = lower_stmts(&s.body, ssa, state, child_block);
    ssa.block_mut(body_exit).terminator = Terminator::Return;

    // Restore parent's SSA versions
    state.restore(&snap);

    after_block
}
