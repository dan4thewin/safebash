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
    validate_stmt_ctx(stmt, diags, false);
}

fn validate_stmt_ctx(stmt: &Statement, diags: &mut Vec<Diagnostic>, in_condition: bool) {
    match &stmt.kind {
        StmtKind::Command(c) => validate_command(c, stmt.span, diags),
        StmtKind::Declaration(d) => validate_declaration(d, stmt.span, diags),
        StmtKind::Assignment(a) => {
            validate_assignment(a, stmt.span, diags);
            // SB5007: arithmetic assignment in condition context
            if in_condition {
                if matches!(&a.value.kind, ExprKind::Arithmetic(_) | ExprKind::ArithmeticExpansion(_)) {
                    diags.push(Diagnostic::error_at(
                        stmt.span.line, stmt.span.col, "SB5007",
                        format!("mutation {} = ... not allowed in condition (( )) expression context — use (( )) statement instead", a.name),
                    ));
                }
            }
        }
        StmtKind::If(i) => {
            for s in &i.condition { validate_stmt_ctx(s, diags, true); }
            for s in &i.then_body { validate_stmt_ctx(s, diags, false); }
            for elif in &i.elif_clauses {
                for s in &elif.condition { validate_stmt_ctx(s, diags, true); }
                for s in &elif.body { validate_stmt_ctx(s, diags, false); }
            }
            if let Some(ref else_body) = i.else_body {
                for s in else_body { validate_stmt_ctx(s, diags, false); }
            }
        }
        StmtKind::While(w) => {
            for s in &w.condition { validate_stmt_ctx(s, diags, true); }
            for s in &w.body { validate_stmt_ctx(s, diags, false); }
        }
        StmtKind::For(f) => {
            // Check glob patterns and unquoted expansions in for-loop words
            for w in &f.words {
                check_unquoted_glob(w, diags);
                // SB5009: unquoted $var in for-loop words implies word splitting
                if let ExprKind::SimpleExpansion(var) = &w.kind {
                    diags.push(Diagnostic::error_at(
                        w.span.line, w.span.col, "SB5009",
                        format!("unquoted ${} in for-loop would word-split — quote it or use an array", var),
                    ));
                }
            }
            for s in &f.body { validate_stmt_ctx(s, diags, false); }
        }
        StmtKind::Case(c) => {
            for item in &c.items {
                for s in &item.body { validate_stmt_ctx(s, diags, false); }
            }
        }
        StmtKind::Function(f) => {
            for s in &f.body { validate_stmt_ctx(s, diags, false); }
        }
        StmtKind::Pipeline(p) => {
            for s in &p.commands { validate_stmt_ctx(s, diags, in_condition); }
        }
        StmtKind::Redirected(r) => {
            validate_stmt_ctx(&r.body, diags, in_condition);
        }
        StmtKind::List(l) => {
            validate_stmt_ctx(&l.left, diags, in_condition);
            validate_stmt_ctx(&l.right, diags, in_condition);
        }
        StmtKind::Group(g) => {
            for s in &g.body { validate_stmt_ctx(s, diags, in_condition); }
        }
        StmtKind::Subshell(s) => {
            for st in &s.body { validate_stmt_ctx(st, diags, in_condition); }
        }
        StmtKind::Suppressed(inner) => validate_stmt_ctx(inner, diags, in_condition),
        StmtKind::ArithTest(expr) => {
            // SB5007: check arithmetic in context
            validate_arith_context(expr, stmt.span, in_condition, diags);
        }
        StmtKind::Comment(_) => {}
    }
    // SB5007: ArithTest as bare statement (not in condition) is a no-op warning
    // This is handled separately since we need condition context
    // Validate all expressions in the statement
    walk_exprs_in_stmt(stmt, &mut |expr, span| validate_expr(expr, span, diags));
}

fn validate_command(c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let name = expr_literal_str(&c.name);

    // SB5001: export rejected
    if name == "export" {
        diags.push(Diagnostic::error_at(
            span.line, span.col, "SB5001",
            "'export' is not allowed — use int/const/array/map instead".to_string(),
        ));
    } else if name == "declare" {
        for arg in &c.args {
            let s = expr_literal_str(arg);
            if s == "-x" || s.contains('x') && s.starts_with('-') {
                diags.push(Diagnostic::error_at(
                    span.line, span.col, "SB5001",
                    "'declare -x' is not allowed — use int/const/array/map instead".to_string(),
                ));
            }
        }
    }

    // SB4001: dynamic execution rejected
    if name == "eval" || name == "source" || name == "." || name == "command" {
        diags.push(Diagnostic::error_at(
            span.line, span.col, "SB4001",
            format!("'{}' is not allowed in sash", name),
        ));
    }

    // Coproc rejected
    if name == "coproc" {
        diags.push(Diagnostic::error_at(
            span.line, span.col, "SB4001",
            "'coproc' is not allowed in sash".to_string(),
        ));
    }

    // SB5019: numeric test operators in [[ ]]
    if name == "[[" {
        for arg in &c.args {
            if let ExprKind::Test(ref test) = arg.kind {
                check_numeric_test_ops(test, diags);
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

fn validate_assignment(a: &Assignment, span: Span, diags: &mut Vec<Diagnostic>) {
    // SB5009: IFS assignment rejected
    if a.name == "IFS" {
        diags.push(Diagnostic::error_at(
            span.line, span.col, "SB5009",
            "IFS does not exist in sash — word splitting is eliminated by design".to_string(),
        ));
    }
}

fn validate_expr(expr: &Expr, _parent_span: Span, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => {
            validate_var_name(var, expr.span, diags);
        }
        ExprKind::Literal(s) => {
            // SB5006: ** globstar is always rejected (even quoted, it's never valid)
            if s.contains("**") {
                diags.push(Diagnostic::error_at(
                    expr.span.line, expr.span.col, "SB5006",
                    format!("** globstar is not supported in sash — only * is allowed: {}", s),
                ));
            }
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

/// SB5019: reject numeric comparison operators in [[ ]]
fn check_numeric_test_ops(test: &TestExpr, diags: &mut Vec<Diagnostic>) {
    match test {
        TestExpr::Binary { left, op, right: _ } => {
            const NUMERIC_OPS: &[&str] = &["-gt", "-lt", "-ge", "-le", "-eq", "-ne"];
            if NUMERIC_OPS.contains(&op.as_str()) {
                let span = left.span;
                diags.push(Diagnostic::error_at(
                    span.line, span.col, "SB5019",
                    format!("'{}' is a numeric operator — use (( )) for arithmetic comparisons, not [[ ]]", op),
                ));
            }
        }
        TestExpr::Not(inner) => check_numeric_test_ops(inner, diags),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            check_numeric_test_ops(l, diags);
            check_numeric_test_ops(r, diags);
        }
        TestExpr::Paren(inner) => check_numeric_test_ops(inner, diags),
        _ => {}
    }
}

/// SB5007: validate arithmetic expression in context.
/// - In condition (if/while): mutations are rejected
/// - As bare statement: comparisons are rejected (no-op)
/// - Prefix ++/-- always rejected (use postfix or explicit)
fn validate_arith_context(expr: &ArithExpr, span: Span, in_condition: bool, diags: &mut Vec<Diagnostic>) {
    use crate::parse::{classify_arith_expr, ArithKind};

    // Prefix ++/-- always rejected
    if let ArithExpr::PreIncDec { var, .. } = expr {
        diags.push(Diagnostic::error_at(
            span.line, span.col, "SB5007",
            format!("prefix ++{} is not allowed — use postfix ({}++) or (( {} += 1 )) instead", var, var, var),
        ));
        return;
    }

    let kind = classify_arith_expr(expr);
    if in_condition {
        // In condition: mutations rejected
        if kind == ArithKind::Mutation {
            let desc = arith_brief(expr);
            diags.push(Diagnostic::error_at(
                span.line, span.col, "SB5007",
                format!("mutation {} not allowed in condition (( )) expression context — use (( )) statement instead", desc),
            ));
        }
    } else {
        // As bare statement: comparisons rejected (no-op)
        if kind == ArithKind::Comparison || kind == ArithKind::Test {
            // Only reject if it's actually a comparison (has comparison op)
            if kind == ArithKind::Comparison {
                let desc = arith_brief(expr);
                diags.push(Diagnostic::error_at(
                    span.line, span.col, "SB5007",
                    format!("comparison {} as bare statement has no effect — use in if/while condition", desc),
                ));
            }
        }
        // Check for mutations in sub-expressions of statement context
        check_arith_nested_mutation(expr, span, diags);
    }
}

/// Check for nested mutations like `x = y++` in statement arithmetic
fn check_arith_nested_mutation(expr: &ArithExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    match expr {
        ArithExpr::Assign { value, .. } => {
            // Check if RHS has side effects
            if has_mutation(value) {
                let desc = arith_brief(value);
                diags.push(Diagnostic::error_at(
                    span.line, span.col, "SB5007",
                    format!("mutation {} not allowed in statement (( )) expression context — use (( )) statement instead", desc),
                ));
            }
        }
        _ => {}
    }
}

fn has_mutation(expr: &ArithExpr) -> bool {
    match expr {
        ArithExpr::PostIncDec { .. } | ArithExpr::PreIncDec { .. }
        | ArithExpr::Assign { .. } | ArithExpr::CompoundAssign { .. } => true,
        ArithExpr::BinOp { left, right, .. } => has_mutation(left) || has_mutation(right),
        ArithExpr::Paren(inner) => has_mutation(inner),
        _ => false,
    }
}

fn arith_brief(expr: &ArithExpr) -> String {
    match expr {
        ArithExpr::Assign { var, .. } => format!("{} = ...", var),
        ArithExpr::CompoundAssign { var, op, .. } => {
            let op_s = match op {
                IrArithOp::Add => "+=", IrArithOp::Sub => "-=",
                _ => "?=",
            };
            format!("{} {}", var, op_s)
        }
        ArithExpr::PostIncDec { var, inc } => {
            format!("{}{}", var, if *inc { "++" } else { "--" })
        }
        ArithExpr::PreIncDec { var, inc } => {
            format!("{}{}", if *inc { "++" } else { "--" }, var)
        }
        ArithExpr::BinOp { left, op, right } => {
            format!("{} {} {}", arith_brief(left), match op {
                IrArithOp::Gt => ">", IrArithOp::Lt => "<",
                IrArithOp::Eq => "==", IrArithOp::Ne => "!=",
                IrArithOp::Ge => ">=", IrArithOp::Le => "<=",
                _ => "?",
            }, arith_brief(right))
        }
        ArithExpr::Var(name) => name.clone(),
        ArithExpr::Literal(n) => n.to_string(),
        _ => "...".to_string(),
    }
}

/// SB5006: check for ? and [...] glob in unquoted expressions
fn check_unquoted_glob(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::Literal(s) => {
            if s.contains('?') {
                diags.push(Diagnostic::error_at(
                    expr.span.line, expr.span.col, "SB5006",
                    format!("? glob is not supported in sash — only * is allowed: {}", s),
                ));
            } else if s.contains('[') && s.contains(']') {
                diags.push(Diagnostic::error_at(
                    expr.span.line, expr.span.col, "SB5006",
                    format!("[...] glob is not supported in sash — only * is allowed: {}", s),
                ));
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                if let ExprKind::Literal(s) = &p.kind {
                    if s.contains('?') {
                        diags.push(Diagnostic::error_at(
                            p.span.line, p.span.col, "SB5006",
                            format!("? glob is not supported in sash — only * is allowed: {}", s),
                        ));
                    } else if s.contains('[') && s.contains(']') {
                        diags.push(Diagnostic::error_at(
                            p.span.line, p.span.col, "SB5006",
                            format!("[...] glob is not supported in sash — only * is allowed: {}", s),
                        ));
                    }
                }
            }
        }
        _ => {}
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

// ── Taint tracking ──────────────────────────────────────────────────

use std::collections::HashSet;

/// Run taint analysis: track which variables hold tainted data,
/// reject any use of tainted variables outside bless expressions.
pub fn check_taint(program: &Program) -> Vec<Diagnostic> {
    let mut tainted: HashSet<String> = HashSet::new();
    let mut diags = Vec::new();
    for stmt in &program.statements {
        taint_stmt(stmt, &mut tainted, &mut diags);
    }
    if let Some(ref end) = program.end_block {
        for stmt in end {
            taint_stmt(stmt, &mut tainted, &mut diags);
        }
    }
    diags
}

/// Like taint_stmt but doesn't mark read/mapfile as tainted
/// (for pipeline stages where input comes from sash code).
fn taint_stmt_no_read_taint(stmt: &Statement, tainted: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::Command(c) = &stmt.kind {
        let name = expr_literal_str(&c.name);
        if name == "read" || name == "mapfile" {
            // Don't taint, but still check args for existing taint
            check_expr_taint(&c.name, tainted, diags);
            for arg in &c.args {
                check_expr_taint(arg, tainted, diags);
            }
            return;
        }
    }
    taint_stmt(stmt, tainted, diags);
}

fn taint_stmt(stmt: &Statement, tainted: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            // Check if RHS is a taint source
            if is_taint_source(&a.value) {
                tainted.insert(a.name.clone());
            } else if expr_uses_tainted(&a.value, tainted) {
                // Propagation: y=$x where x is tainted → y tainted
                tainted.insert(a.name.clone());
            }
        }
        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                if is_taint_source(&a.value) {
                    tainted.insert(a.name.clone());
                } else if expr_uses_tainted(&a.value, tainted) {
                    tainted.insert(a.name.clone());
                }
            }
        }
        StmtKind::Command(c) => {
            let name = expr_literal_str(&c.name);
            // read/mapfile produce tainted variables (when reading from stdin/pipe)
            // But read from herestring (<<<) is clean
            if name == "read" || name == "mapfile" {
                // Check if this is inside a Redirected with herestring
                // For now, mark read as tainted — herestring case handled
                // by not reaching here (redirected commands are checked via
                // the Redirected wrapper)
                for arg in c.args.iter().rev() {
                    let s = expr_literal_str(arg);
                    if !s.starts_with('-') && !s.is_empty() {
                        tainted.insert(s.to_string());
                        break;
                    }
                }
            } else if name == "bless" {
                // bless removes taint — first arg is the variable
                if let Some(arg) = c.args.first() {
                    let var = expr_literal_str(arg);
                    if !var.is_empty() {
                        tainted.remove(var);
                    }
                }
                return; // Don't check args of bless for taint
            }
            // Check all command arguments for tainted variable use
            check_expr_taint(&c.name, tainted, diags);
            for arg in &c.args {
                check_expr_taint(arg, tainted, diags);
            }
        }
        StmtKind::If(i) => {
            for s in &i.condition { taint_stmt(s, tainted, diags); }
            for s in &i.then_body { taint_stmt(s, tainted, diags); }
            for elif in &i.elif_clauses {
                for s in &elif.condition { taint_stmt(s, tainted, diags); }
                for s in &elif.body { taint_stmt(s, tainted, diags); }
            }
            if let Some(ref else_body) = i.else_body {
                for s in else_body { taint_stmt(s, tainted, diags); }
            }
        }
        StmtKind::While(w) => {
            for s in &w.condition { taint_stmt(s, tainted, diags); }
            for s in &w.body { taint_stmt(s, tainted, diags); }
        }
        StmtKind::For(f) => {
            for s in &f.body { taint_stmt(s, tainted, diags); }
        }
        StmtKind::Case(c) => {
            check_expr_taint(&c.value, tainted, diags);
            for item in &c.items {
                for s in &item.body { taint_stmt(s, tainted, diags); }
            }
        }
        StmtKind::Function(_) => {
            // Functions are checked at call site (taint flows through args)
            // For now, don't recurse into function bodies during top-level taint
        }
        StmtKind::Pipeline(p) => {
            // In pipelines, read/mapfile receives from sash-controlled code
            // Don't taint pipeline-internal reads
            for (i, s) in p.commands.iter().enumerate() {
                if i > 0 {
                    // Non-first stages: read from pipe is clean
                    taint_stmt_no_read_taint(s, tainted, diags);
                } else {
                    taint_stmt(s, tainted, diags);
                }
            }
        }
        StmtKind::Redirected(r) => {
            // Check if this is `read x <<< "literal"` — clean read
            if let StmtKind::Command(c) = &r.body.kind {
                let name = expr_literal_str(&c.name);
                if (name == "read" || name == "mapfile")
                    && r.redirects.iter().any(|rd| matches!(rd.op, crate::ir::RedirectOp::HereString | crate::ir::RedirectOp::HereDoc))
                {
                    // Herestring/heredoc read — don't taint
                    return;
                }
            }
            taint_stmt(&r.body, tainted, diags);
        }
        StmtKind::List(l) => {
            taint_stmt(&l.left, tainted, diags);
            taint_stmt(&l.right, tainted, diags);
        }
        StmtKind::Group(g) => {
            for s in &g.body { taint_stmt(s, tainted, diags); }
        }
        StmtKind::Subshell(s) => {
            for st in &s.body { taint_stmt(st, tainted, diags); }
        }
        StmtKind::Suppressed(inner) => taint_stmt(inner, tainted, diags),
        StmtKind::ArithTest(_) | StmtKind::Comment(_) => {}
    }
}

/// Check if an expression is a taint source.
fn is_taint_source(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::CommandSubstitution(stmts) => {
            if let Some(first) = stmts.first() {
                if let StmtKind::Command(c) = &first.kind {
                    let name = expr_literal_str(&c.name);
                    // Explicit taint sources
                    if name == "getenv" || name == "argv" {
                        return true;
                    }
                    // External commands (absolute path) are taint sources
                    if name.starts_with('/') {
                        return true;
                    }
                    // Unknown commands that aren't user functions — we can't
                    // know at this point if it's a user function without
                    // tracking function definitions. Be conservative: only
                    // mark as tainted if it's clearly external.
                    return false;
                }
            }
            false
        }
        _ => false,
    }
}

/// Check if an expression uses any tainted variable.
fn expr_uses_tainted(expr: &Expr, tainted: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => tainted.contains(var.as_str()),
        ExprKind::ParameterExpansion(pe) => tainted.contains(pe.name.as_str()),
        ExprKind::DoubleQuoted(parts) => parts.iter().any(|p| expr_uses_tainted(p, tainted)),
        ExprKind::Concat(parts) => parts.iter().any(|p| expr_uses_tainted(p, tainted)),
        _ => false,
    }
}

/// Check an expression for tainted variable use, emitting diagnostics.
fn check_expr_taint(expr: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => {
            if tainted.contains(var.as_str()) {
                diags.push(Diagnostic::error_at(
                    expr.span.line, expr.span.col, "SB5020",
                    format!("tainted variable ${} used without bless", var),
                ));
            }
            // Also check positional params at top level
            if var.len() == 1 && var.as_bytes()[0].is_ascii_digit() && var != "0" {
                // $1-$9 at top level are tainted implicitly
                // (they come from argv)
                diags.push(Diagnostic::error_at(
                    expr.span.line, expr.span.col, "SB5020",
                    format!("${} is tainted (from argv) — use $(argv {}) and bless", var, var),
                ));
            }
        }
        ExprKind::ParameterExpansion(pe) => {
            if tainted.contains(pe.name.as_str()) {
                diags.push(Diagnostic::error_at(
                    expr.span.line, expr.span.col, "SB5020",
                    format!("tainted variable ${{{} ...}} used without bless", pe.name),
                ));
            }
        }
        ExprKind::DoubleQuoted(parts) => {
            for p in parts { check_expr_taint(p, tainted, diags); }
        }
        ExprKind::Concat(parts) => {
            for p in parts { check_expr_taint(p, tainted, diags); }
        }
        _ => {}
    }
}
