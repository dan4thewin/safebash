//! Control flow: if/while/for/case → SSA blocks with branches.

use super::expressions::{expr_to_string, expr_to_value};
use super::helpers::insert_phis_two_branch;
use super::pipelines::{lower_for_array, lower_for_dollar_at};
use super::statements::lower_stmts;
use crate::ir::*;
use crate::ssa::*;

use super::resolution::resolve_pipeline_command;

pub(super) fn flatten_elif(i: &If) -> If {
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
    let test_expr = c.args[1..].iter().find_map(|a| {
        if let crate::ir::ExprKind::Test(t) = &a.kind {
            Some(t.clone())
        } else {
            None
        }
    });
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
    let _then_snap = state.snapshot();

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

pub(super) fn lower_if(
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
            state,
            ssa,
            merge_block,
            then_exit,
            &then_state,
            else_exit,
            &else_state,
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
            state,
            ssa,
            merge_block,
            then_exit,
            &then_state,
            cond_block,
            &pre_state,
        );
    }

    // Handle elif clauses by chaining — each elif is a nested if
    // For simplicity in this implementation, elif clauses were already
    // flattened into the IR's if/elif structure. We handle the first
    // else and leave elif for future refinement.

    merge_block
}

// ── While lowering ────────────────────────────────────────────

pub(super) fn lower_while(
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
        state,
        ssa,
        header,
        block,
        &pre_state,
        body_exit_1,
        &body_state_1,
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

pub(super) fn lower_for(
    f: &For,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // Check if this is a dynamic for loop (e.g., `for x in "$@"`)
    let is_dynamic = f
        .words
        .iter()
        .any(|w| !matches!(&w.kind, ExprKind::Literal(_)));

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
            ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
                Some((pe.name.clone(), false))
            }
            ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrkeys") => {
                Some((pe.name.clone(), true))
            }
            ExprKind::DoubleQuoted(parts) if parts.len() == 1 => match &parts[0].kind {
                ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
                    Some((pe.name.clone(), false))
                }
                ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrkeys") => {
                    Some((pe.name.clone(), true))
                }
                _ => None,
            },
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
                    } else {
                        None
                    }
                } else {
                    None
                }
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
                        let args: Vec<SsaValue> =
                            c.args.iter().map(|a| expr_to_value(a, state)).collect();
                        let redirs = match &stmt.kind {
                            StmtKind::Redirected(r) => r.redirects.clone(),
                            _ => vec![],
                        };
                        stages.push((resolve_pipeline_command(&name), args, redirs));
                    }
                }
                // Emit pipeline with pipe capture
                ssa.block_mut(block).insts.push(SsaInst::Pipeline {
                    stages,
                    capture_read_fd: true,
                    checked: true,
                });

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
    let iter_blocks: Vec<BlockId> = f.words.iter().map(|_| ssa.new_block("for.iter")).collect();

    for (i, word) in f.words.iter().enumerate() {
        let iter_block = iter_blocks[i];
        ssa.block_mut(current).terminator = Terminator::Goto(iter_block);
        current = iter_block;

        // continue → next iteration (or exit if last)
        let continue_target = if i + 1 < iter_blocks.len() {
            iter_blocks[i + 1]
        } else {
            exit_block
        };
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

pub(super) fn lower_case(
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
        let is_wildcard = item
            .patterns
            .iter()
            .any(|p| matches!(&p.kind, ExprKind::Literal(s) if s == "*"));

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
        item.patterns
            .iter()
            .any(|p| matches!(&p.kind, ExprKind::Literal(s) if s == "*"))
    });

    // Build a no-match state: pre_state with "?" forced to 0
    // (POSIX: "if no pattern is matched, the exit status is zero")
    let no_match_path: Option<(BlockId, std::collections::HashMap<String, u32>)> = if !has_wildcard
    {
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
        let all_vars: std::collections::HashSet<String> = all_paths
            .iter()
            .flat_map(|(_, s)| s.keys().cloned())
            .collect();

        for var in &all_vars {
            let versions: Vec<(BlockId, u32)> = all_paths
                .iter()
                .map(|(bid, s)| (*bid, s.get(var).copied().unwrap_or(0)))
                .collect();

            let first_ver = versions[0].1;
            if versions.iter().any(|(_, v)| *v != first_ver) {
                let target = state.define(var);
                let sources: Vec<(BlockId, VarRef)> = versions
                    .iter()
                    .map(|(bid, ver)| (*bid, VarRef::new(var, *ver)))
                    .collect();
                ssa.block_mut(merge_block)
                    .phis
                    .push(PhiNode { target, sources });
            }
        }
    } else if all_paths.len() == 1 {
        // Single path (e.g. wildcard-only case) — adopt its state
        let (_, ref path_state) = all_paths[0];
        state.restore(path_state);
    }

    merge_block
}
