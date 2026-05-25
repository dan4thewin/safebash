//! IR-based validation rules.
//!
//! Walks the parsed IR and rejects unsafe constructs.
//! Replaces the old tree-sitter-based validator.

use crate::diagnostic::Diagnostic;
use crate::ir::*;

/// Run all validation rules on a parsed program.
/// Returns diagnostics for any violations found.
pub fn validate(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in &program.statements {
        validate_stmt(stmt, &mut diags);
    }
    if let Some(ref end) = program.end_block {
        for stmt in end {
            validate_stmt(stmt, &mut diags);
        }
    }
    diags
}

fn validate_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Command(c) => validate_command(c, stmt.span, diags),
        StmtKind::Declaration(d) => validate_declaration(d, stmt.span, diags),
        StmtKind::Assignment(a) => validate_assignment(a, stmt.span, diags),
        StmtKind::If(i) => {
            for s in &i.condition { validate_stmt(s, diags); }
            for s in &i.then_body { validate_stmt(s, diags); }
            for elif in &i.elif_clauses {
                for s in &elif.condition { validate_stmt(s, diags); }
                for s in &elif.body { validate_stmt(s, diags); }
            }
            if let Some(ref else_body) = i.else_body {
                for s in else_body { validate_stmt(s, diags); }
            }
        }
        StmtKind::While(w) => {
            for s in &w.condition { validate_stmt(s, diags); }
            for s in &w.body { validate_stmt(s, diags); }
        }
        StmtKind::For(f) => {
            for s in &f.body { validate_stmt(s, diags); }
        }
        StmtKind::Case(c) => {
            for item in &c.items {
                for s in &item.body { validate_stmt(s, diags); }
            }
        }
        StmtKind::Function(f) => {
            for s in &f.body { validate_stmt(s, diags); }
        }
        StmtKind::Pipeline(p) => {
            for s in &p.commands { validate_stmt(s, diags); }
        }
        StmtKind::Redirected(r) => {
            validate_stmt(&r.body, diags);
        }
        StmtKind::List(l) => {
            validate_stmt(&l.left, diags);
            validate_stmt(&l.right, diags);
        }
        StmtKind::Group(g) => {
            for s in &g.body { validate_stmt(s, diags); }
        }
        StmtKind::Subshell(s) => {
            for st in &s.body { validate_stmt(st, diags); }
        }
        StmtKind::Suppressed(inner) => validate_stmt(inner, diags),
        StmtKind::ArithTest(_) | StmtKind::Comment(_) => {}
    }
    // Validate all expressions in the statement
    walk_exprs_in_stmt(stmt, &mut |expr, span| validate_expr(expr, span, diags));
}

fn validate_command(c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let name = expr_literal_str(&c.name);

    // SB5001: export rejected
    if name == "export" || name == "declare" {
        // Check for -x flag
        if name == "export" {
            diags.push(Diagnostic::error_at(
                span.line, span.col, "SB5001",
                "'export' is not allowed — use int/const/array/map instead".to_string(),
            ));
        } else {
            // declare with -x
            for arg in &c.args {
                if expr_literal_str(&arg) == "-x" {
                    diags.push(Diagnostic::error_at(
                        span.line, span.col, "SB5001",
                        "'declare -x' is not allowed — use int/const/array/map instead".to_string(),
                    ));
                }
            }
        }
    }
}

fn validate_declaration(d: &Declaration, span: Span, diags: &mut Vec<Diagnostic>) {
    // SB5001: export/declare -x
    if matches!(d.keyword, DeclKeyword::Export) {
        diags.push(Diagnostic::error_at(
            span.line, span.col, "SB5001",
            "'export' is not allowed — use int/const/array/map instead".to_string(),
        ));
    }
}

fn validate_assignment(_a: &Assignment, _span: Span, _diags: &mut Vec<Diagnostic>) {
    // Future: taint checks on assignment values
}

fn validate_expr(expr: &Expr, _parent_span: Span, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => {
            validate_var_name(var, expr.span, diags);
        }
        _ => {}
    }
}

// ── SB5017/5018: Rejected variables ─────────────────────────────────

const REJECTED_VARS: &[(&str, &str)] = &[
    ("RANDOM", "$RANDOM does not exist in sash"),
    ("BASH_VERSION", "$BASH_VERSION does not exist in sash"),
    ("BASH_SOURCE", "$BASH_SOURCE does not exist in sash"),
    ("BASH_LINENO", "$BASH_LINENO does not exist in sash"),
    ("FUNCNAME", "$FUNCNAME does not exist in sash"),
    ("GROUPS", "$GROUPS does not exist in sash"),
    ("DIRSTACK", "$DIRSTACK does not exist in sash"),
    ("PIPESTATUS", "$PIPESTATUS does not exist in sash"),
    ("SHELLOPTS", "$SHELLOPTS does not exist in sash"),
    ("BASHOPTS", "$BASHOPTS does not exist in sash"),
    ("MACHTYPE", "$MACHTYPE does not exist in sash"),
    ("OSTYPE", "$OSTYPE does not exist in sash"),
    ("HOSTTYPE", "$HOSTTYPE does not exist in sash"),
    ("HOSTNAME", "$HOSTNAME does not exist in sash — use $(getenv HOSTNAME)"),
    ("SECONDS", "$SECONDS does not exist in sash"),
    ("EPOCHSECONDS", "$EPOCHSECONDS does not exist in sash"),
    ("EPOCHREALTIME", "$EPOCHREALTIME does not exist in sash"),
    ("SRANDOM", "$SRANDOM does not exist in sash"),
    ("IFS", "IFS does not exist in sash — word splitting is eliminated by design"),
];

const REJECTED_POSITIONAL: &[(&str, &str)] = &[
    ("0", "$0 is not available in sash — use $(argv 0)"),
];

/// SB5018: Bare env vars require $(getenv NAME)
const ENV_VARS: &[&str] = &[
    "HOME", "PATH", "USER", "SHELL", "TERM", "LANG", "LC_ALL",
    "LC_CTYPE", "DISPLAY", "XDG_RUNTIME_DIR", "TMPDIR", "EDITOR",
    "VISUAL", "PAGER", "LOGNAME", "MAIL", "PWD", "OLDPWD",
];

fn validate_var_name(var: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    // SB5017: Rejected vars
    for (name, msg) in REJECTED_VARS {
        if var == *name {
            diags.push(Diagnostic::error_at(
                span.line, span.col, "SB5017", msg.to_string(),
            ));
            return;
        }
    }
    for (name, msg) in REJECTED_POSITIONAL {
        if var == *name {
            diags.push(Diagnostic::error_at(
                span.line, span.col, "SB5017", msg.to_string(),
            ));
            return;
        }
    }
    // SB5018: Bare env vars
    for name in ENV_VARS {
        if var == *name {
            diags.push(Diagnostic::error_at(
                span.line, span.col, "SB5018",
                format!("${} is not available directly — use $(getenv {})", name, name),
            ));
            return;
        }
    }
    // SB5009: IFS (also in REJECTED_VARS but keeping explicit for clarity)
}

// ── Helpers ─────────────────────────────────────────────────────────

fn expr_literal_str(expr: &Expr) -> &str {
    match &expr.kind {
        ExprKind::Literal(s) => s.as_str(),
        _ => "",
    }
}

/// Walk all expressions in a statement, passing each to the callback.
fn walk_exprs_in_stmt(stmt: &Statement, f: &mut impl FnMut(&Expr, Span)) {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            walk_expr(&a.value, f);
            if let Some(ref idx) = a.index { walk_expr(idx, f); }
        }
        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                walk_expr(&a.value, f);
            }
        }
        StmtKind::Command(c) => {
            walk_expr(&c.name, f);
            for arg in &c.args { walk_expr(arg, f); }
        }
        StmtKind::If(i) => {
            for s in &i.condition { walk_exprs_in_stmt(s, f); }
            for s in &i.then_body { walk_exprs_in_stmt(s, f); }
            for elif in &i.elif_clauses {
                for s in &elif.condition { walk_exprs_in_stmt(s, f); }
                for s in &elif.body { walk_exprs_in_stmt(s, f); }
            }
            if let Some(ref else_body) = i.else_body {
                for s in else_body { walk_exprs_in_stmt(s, f); }
            }
        }
        StmtKind::While(w) => {
            for s in &w.condition { walk_exprs_in_stmt(s, f); }
            for s in &w.body { walk_exprs_in_stmt(s, f); }
        }
        StmtKind::For(fv) => {
            for w in &fv.words { walk_expr(w, f); }
            for s in &fv.body { walk_exprs_in_stmt(s, f); }
        }
        StmtKind::Case(c) => {
            walk_expr(&c.value, f);
            for item in &c.items {
                for s in &item.body { walk_exprs_in_stmt(s, f); }
            }
        }
        StmtKind::Function(func) => {
            for s in &func.body { walk_exprs_in_stmt(s, f); }
        }
        StmtKind::Pipeline(p) => {
            for s in &p.commands { walk_exprs_in_stmt(s, f); }
        }
        StmtKind::Redirected(r) => {
            walk_exprs_in_stmt(&r.body, f);
        }
        StmtKind::List(l) => {
            walk_exprs_in_stmt(&l.left, f);
            walk_exprs_in_stmt(&l.right, f);
        }
        StmtKind::Group(g) => {
            for s in &g.body { walk_exprs_in_stmt(s, f); }
        }
        StmtKind::Subshell(s) => {
            for st in &s.body { walk_exprs_in_stmt(st, f); }
        }
        StmtKind::Suppressed(inner) => walk_exprs_in_stmt(inner, f),
        StmtKind::ArithTest(_) | StmtKind::Comment(_) => {}
    }
}

fn walk_expr(expr: &Expr, f: &mut impl FnMut(&Expr, Span)) {
    f(expr, expr.span);
    match &expr.kind {
        ExprKind::DoubleQuoted(parts) => {
            for p in parts { walk_expr(p, f); }
        }
        ExprKind::Concat(parts) => {
            for p in parts { walk_expr(p, f); }
        }
        ExprKind::Array(elems) => {
            for e in elems { walk_expr(e, f); }
        }
        ExprKind::AssocArray(pairs) => {
            for (_, v) in pairs { walk_expr(v, f); }
        }
        ExprKind::CommandSubstitution(stmts) => {
            for s in stmts { walk_exprs_in_stmt(s, f); }
        }
        ExprKind::ParameterExpansion(pe) => {
            if let Some(ref operand) = pe.operand { walk_expr(operand, f); }
        }
        ExprKind::Test(t) => walk_test_expr(t, f),
        ExprKind::Subscript { index, .. } => walk_expr(index, f),
        _ => {}
    }
}

fn walk_test_expr(t: &TestExpr, f: &mut impl FnMut(&Expr, Span)) {
    match t {
        TestExpr::Unary { operand, .. } => walk_expr(operand, f),
        TestExpr::Binary { left, right, .. } => {
            walk_expr(left, f);
            walk_expr(right, f);
        }
        TestExpr::Not(inner) => walk_test_expr(inner, f),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            walk_test_expr(l, f);
            walk_test_expr(r, f);
        }
        TestExpr::Paren(inner) => walk_test_expr(inner, f),
    }
}
