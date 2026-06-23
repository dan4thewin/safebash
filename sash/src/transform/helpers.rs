//! Shared helpers for the transform module.

use crate::ir::*;
use crate::ssa::*;

pub(super) fn insert_phis_two_branch(
    state: &mut VersionState,
    ssa: &mut SsaProgram,
    merge: BlockId,
    block_a: BlockId,
    state_a: &std::collections::HashMap<String, u32>,
    block_b: BlockId,
    state_b: &std::collections::HashMap<String, u32>,
) {
    let all_vars: std::collections::HashSet<&String> =
        state_a.keys().chain(state_b.keys()).collect();

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

pub(super) fn scan_expr_positionals_stmt(stmt: &Statement, max: &mut usize, shifts: &mut usize) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            if let Some(n) = shift_amount(c) {
                *shifts += n;
            }
            scan_expr_positionals_expr(&c.name, max);
            for a in &c.args {
                scan_expr_positionals_expr(a, max);
            }
        }
        StmtKind::Assignment(a) => {
            scan_expr_positionals_expr(&a.value, max);
        }
        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                scan_expr_positionals_expr(&a.value, max);
            }
        }
        StmtKind::For(f) => {
            for w in &f.words {
                scan_expr_positionals_expr(w, max);
            }
            scan_stmts(&f.body, max, shifts);
        }
        StmtKind::Case(c) => {
            scan_expr_positionals_expr(&c.value, max);
            for item in &c.items {
                scan_stmts(&item.body, max, shifts);
            }
        }
        StmtKind::Function(_) => {} // don't recurse into nested functions
        // Default: descend into every nested statement body via
        // child_bodies(). Covers If (incl. elif_clauses), While,
        // Pipeline, Group, Subshell, List, Redirected, Suppressed.
        // Leaf StmtKinds (Comment, ArithTest) return empty bodies.
        _ => {
            for body in stmt.child_bodies() {
                scan_stmts(body, max, shifts);
            }
        }
    }
}

/// If `c` is a `shift` command, return how many positions it
/// shifts by (`shift` ≡ 1, `shift N` parses N as a positive
/// integer, falling back to 1 if N is non-numeric). Returns
/// `None` for any other command.
fn shift_amount(c: &Command) -> Option<usize> {
    let name = match &c.name.kind {
        ExprKind::Literal(s) => s.as_str(),
        _ => return None,
    };
    if name != "shift" {
        return None;
    }
    let n = c
        .args
        .first()
        .and_then(|a| match &a.kind {
            ExprKind::Literal(s) => s.parse::<usize>().ok(),
            _ => None,
        })
        .unwrap_or(1);
    Some(n)
}

/// Walk a sequence of statements, accumulating max positional ref
/// and shift count. Factor of `scan_expr_positionals_stmt` for
/// uses that already have a `&[Statement]` slice.
fn scan_stmts(stmts: &[Statement], max: &mut usize, shifts: &mut usize) {
    for s in stmts {
        scan_expr_positionals_stmt(s, max, shifts);
    }
}

pub(super) fn scan_expr_positionals_expr(expr: &Expr, max: &mut usize) {
    match &expr.kind {
        ExprKind::SimpleExpansion(name) => record_positional(name, max),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            for p in parts {
                scan_expr_positionals_expr(p, max);
            }
        }
        ExprKind::ParameterExpansion(pe) => {
            record_positional(&pe.name, max);
            if let Some(ref op) = pe.operand {
                scan_expr_positionals_expr(op, max);
            }
        }
        ExprKind::CommandSubstitution(stmts) => {
            let mut sub_shifts = 0;
            scan_stmts(stmts, max, &mut sub_shifts);
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

/// If `name` is `"1"`..`"9"`, update `max` to the larger of itself
/// and the parsed digit. Non-numeric or out-of-range names are no-
/// ops. Used for both `$N` (`SimpleExpansion`) and `${N…}`
/// (`ParameterExpansion`) name parts.
fn record_positional(name: &str, max: &mut usize) {
    if let Ok(n) = name.parse::<usize>() {
        if (1..=9).contains(&n) && n > *max {
            *max = n;
        }
    }
}

pub(super) fn scan_test_positionals(test: &TestExpr, max: &mut usize) {
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
    }
}
