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

        StmtKind::Pipeline(p) => super::stmt_handlers::handle_pipeline(p, stmt, ssa, state, block),
        StmtKind::List(l) => lower_list_stmt(l, stmt, ssa, state, block),

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

        StmtKind::Subshell(s) => lower_subshell_stmt(s, ssa, state, block),

        StmtKind::Group(g) => lower_stmts(&g.body, ssa, state, block),

        StmtKind::Case(c) => lower_case(c, ssa, state, block),

        StmtKind::Comment(_) => block,

        StmtKind::ArithTest(expr) => {
            // (( expr )) as a condition: evaluate arithmetic, set exit
            // status based on result (0 = true if nonzero, 1 = false if zero)
            let ir_expr = crate::ir::Expr {
                kind: crate::ir::ExprKind::Arithmetic(expr.clone()),
                span: stmt.span,
            };
            let val = super::expressions::expr_to_value(&ir_expr, state);
            let val = super::functions::lift_int_func_calls(val, ssa, state, &mut block);
            // Emit as a test: compare result != 0
            let target = state.define("__arith_test");
            ssa.block_mut(block).insts.push(SsaInst::Assign {
                target: target.clone(),
                value: val,
            });
            ssa.block_mut(block).insts.push(SsaInst::Command {
                name: "__let_exit".to_string(),
                args: vec![SsaValue::Var(target)],
                redirects: vec![],
                checked: true,
            });
            block
        }

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
    stmt: &Statement,
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

fn lower_subshell_stmt(
    s: &Subshell,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
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
