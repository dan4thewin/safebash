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
            // Detect `shift` command
            let name_text = match &c.name.kind {
                ExprKind::Literal(s) => s.as_str(),
                _ => "",
            };
            if name_text == "shift" {
                // shift with no args shifts by 1; shift N shifts by N
                let n = c
                    .args
                    .first()
                    .and_then(|a| {
                        if let ExprKind::Literal(s) = &a.kind {
                            s.parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(1);
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
        StmtKind::Pipeline(p) => {
            for s in &p.commands {
                scan_expr_positionals_stmt(s, max, shifts);
            }
        }
        StmtKind::If(i) => {
            for s in &i.condition {
                scan_expr_positionals_stmt(s, max, shifts);
            }
            for s in &i.then_body {
                scan_expr_positionals_stmt(s, max, shifts);
            }
            if let Some(ref eb) = i.else_body {
                for s in eb {
                    scan_expr_positionals_stmt(s, max, shifts);
                }
            }
        }
        StmtKind::While(w) => {
            for s in &w.condition {
                scan_expr_positionals_stmt(s, max, shifts);
            }
            for s in &w.body {
                scan_expr_positionals_stmt(s, max, shifts);
            }
        }
        StmtKind::For(f) => {
            for w in &f.words {
                scan_expr_positionals_expr(w, max);
            }
            for s in &f.body {
                scan_expr_positionals_stmt(s, max, shifts);
            }
        }
        StmtKind::Case(c) => {
            scan_expr_positionals_expr(&c.value, max);
            for item in &c.items {
                for s in &item.body {
                    scan_expr_positionals_stmt(s, max, shifts);
                }
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
            for st in &s.body {
                scan_expr_positionals_stmt(st, max, shifts);
            }
        }
        StmtKind::Group(g) => {
            for st in &g.body {
                scan_expr_positionals_stmt(st, max, shifts);
            }
        }
        StmtKind::Comment(_) => {}
        StmtKind::Suppressed(inner) => {
            scan_expr_positionals_stmt(inner, max, shifts);
        }
        StmtKind::ArithTest(_) => {}
    }
}

pub(super) fn scan_expr_positionals_expr(expr: &Expr, max: &mut usize) {
    match &expr.kind {
        ExprKind::SimpleExpansion(name) => {
            if let Ok(n) = name.parse::<usize>() {
                if (1..=9).contains(&n) && n > *max {
                    *max = n;
                }
            }
        }
        ExprKind::DoubleQuoted(parts) => {
            for p in parts {
                scan_expr_positionals_expr(p, max);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                scan_expr_positionals_expr(p, max);
            }
        }
        ExprKind::Array(elems) => {
            for e in elems {
                scan_expr_positionals_expr(e, max);
            }
        }
        ExprKind::ParameterExpansion(pe) => {
            if let Ok(n) = pe.name.parse::<usize>() {
                if (1..=9).contains(&n) && n > *max {
                    *max = n;
                }
            }
            if let Some(ref op) = pe.operand {
                scan_expr_positionals_expr(op, max);
            }
        }
        ExprKind::CommandSubstitution(stmts) => {
            let mut sub_shifts = 0;
            for s in stmts {
                scan_expr_positionals_stmt(s, max, &mut sub_shifts);
            }
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
pub(super) fn parse_arith_binop(s: &str) -> (&str, Option<ArithOp>, &str) {
    let bytes = s.as_bytes();
    let start = if bytes.first() == Some(&b'-') { 1 } else { 0 };
    // Check two-char operators first: ==, !=, <=, >=, ||, &&
    // Skip operators inside parentheses
    let mut depth = 0i32;
    for i in start..bytes.len().saturating_sub(1) {
        if bytes[i] == b'(' {
            depth += 1;
            continue;
        }
        if bytes[i] == b')' {
            depth -= 1;
            continue;
        }
        if depth > 0 {
            continue;
        }
        let two = &s[i..i + 2];
        if let Some(op) = ArithOp::from_str(two) {
            let lhs = s[..i].trim();
            let rhs = s[i + 2..].trim();
            if !lhs.is_empty() && !rhs.is_empty() {
                return (lhs, Some(op), rhs);
            }
        }
    }
    // Single-char operators: +, -, *, /, %, <, >
    depth = 0;
    for (i, &b) in bytes[start..].iter().enumerate() {
        let pos = i + start;
        if b == b'(' {
            depth += 1;
            continue;
        }
        if b == b')' {
            depth -= 1;
            continue;
        }
        if depth > 0 {
            continue;
        }
        let op_str = &s[pos..pos + 1];
        if let Some(op) = ArithOp::from_str(op_str) {
            let lhs = s[..pos].trim();
            let rhs = s[pos + 1..].trim();
            if !lhs.is_empty() && !rhs.is_empty() {
                return (lhs, Some(op), rhs);
            }
        }
    }
    // Check if the whole thing is parenthesized: (expr)
    let trimmed = s.trim();
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        // Strip outer parens and recurse
        let inner = &trimmed[1..trimmed.len() - 1];
        return parse_arith_binop(inner);
    }
    (s, None, "")
}
