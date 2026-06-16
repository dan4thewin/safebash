//! IR-based validation rules.
//!
//! Walks the parsed IR and rejects unsafe constructs. This is the
//! second-pass gate (after parse-time rejections in parse/).
//!
//! Two independent analyses:
//!   1. **Structural validation** — walks IR checking rule violations:
//!      SB5001 (export), SB5006 (globs), SB5007 (arith mutations),
//!      SB5009 (IFS/word-split), SB5011 (command resolution),
//!      SB5012 ($? state machine), SB5017/5018 (rejected/env vars),
//!      SB5019 (numeric test ops), SB5023 (regex validation)
//!   2. **Taint tracking** (`check_taint`) — forward dataflow:
//!      sources (getenv, argv, external commands, stdin read) →
//!      propagation → use-site rejection → bless removes taint
//!
//! SECURITY: This module + parse/ are the complete validation surface.
//! Together they guarantee: no tainted data reaches execution without
//! bless, no dynamic dispatch, no unknown commands, no rejected vars.
//!
//! Trust model:
//!   - Parser guarantees: only recognized syntax reaches IR
//!   - Validator guarantees: only safe *semantics* reach transform/codegen
//!   - If both pass with zero diagnostics, the program is in the safe subset

use crate::diagnostic::Diagnostic;
use crate::ir::*;

// ── Entry point ────────────────────────────────────────────────────

/// Run all validation rules on a parsed program.
/// Returns diagnostics for any violations found.
pub fn validate(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Collect declared function names
    let mut functions = HashSet::new();
    collect_functions(&program.statements, &mut functions);
    if let Some(ref end) = program.end_block {
        collect_functions(end, &mut functions);
    }

    for stmt in &program.statements {
        validate_stmt(stmt, &mut diags);
    }
    if let Some(ref end) = program.end_block {
        for stmt in end {
            validate_stmt(stmt, &mut diags);
        }
    }
    // SB5011: command resolution
    check_commands(&program.statements, &functions, &mut diags);
    if let Some(ref end) = program.end_block {
        check_commands(end, &functions, &mut diags);
    }
    // SB5012: $? availability
    check_dollar_question(&program.statements, &mut diags);
    if let Some(ref end) = program.end_block {
        check_dollar_question(end, &mut diags);
    }
    diags
}

// ── Statement-level validation ─────────────────────────────────────

fn validate_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    validate_stmt_ctx(stmt, diags, false);
}

fn validate_stmt_ctx(stmt: &Statement, diags: &mut Vec<Diagnostic>, in_condition: bool) {
    match &stmt.kind {
        StmtKind::Command(c) => validate_command(c, stmt.span, diags),
        StmtKind::Declaration(d) => validate_declaration(d, stmt.span, diags),
        StmtKind::Assignment(a) => validate_assignment_stmt(a, stmt.span, in_condition, diags),
        StmtKind::If(i) => validate_if(i, diags),
        StmtKind::While(w) => validate_while(w, diags),
        StmtKind::For(f) => validate_for(f, diags),
        StmtKind::Group(g) => validate_group(g, in_condition, diags),
        StmtKind::ArithTest(expr) => {
            // SB5007: check arithmetic in context
            validate_arith_context(expr, stmt.span, in_condition, diags);
        }
        StmtKind::Comment(_) => {}
        // Default for wrappers (Pipeline, Subshell, Suppressed,
        // Redirected, List) and reset-context constructs (Function,
        // Case). Forwarders propagate `in_condition`; resetters
        // (Function, Case) drop it to false so a function body or
        // case body is never validated as if it were in condition
        // context.
        _ => {
            let inner = forwards_condition(stmt) && in_condition;
            for body in stmt.child_bodies() {
                for s in body {
                    validate_stmt_ctx(s, diags, inner);
                }
            }
        }
    }
    // Validate all expressions in the statement
    walk_exprs_in_stmt(stmt, &mut |expr, span| validate_expr(expr, span, diags));
}

/// True for wrapping StmtKinds whose validation context is the
/// same as the wrapper itself: Pipeline, Subshell, Suppressed,
/// Redirected, List. Group is *not* listed here because DD#31
/// requires a per-call inspection; it has its own dedicated arm.
/// Function and Case are *not* listed because their bodies must
/// be validated outside of condition context regardless of the
/// wrapper's own context.
fn forwards_condition(stmt: &Statement) -> bool {
    matches!(
        &stmt.kind,
        StmtKind::Pipeline(_)
            | StmtKind::Subshell(_)
            | StmtKind::Suppressed(_)
            | StmtKind::Redirected(_)
            | StmtKind::List(_)
    )
}

fn validate_assignment_stmt(
    a: &Assignment,
    span: Span,
    in_condition: bool,
    diags: &mut Vec<Diagnostic>,
) {
    validate_assignment(a, span, diags);
    // SB5007: arithmetic assignment in condition context.
    // The DD#31 comma pattern is handled at the Group level.
    if in_condition && matches!(&a.value.kind, ExprKind::Arithmetic(_)) {
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "mutation {} = ... not allowed in condition (( )) expression context — \
                 use (( )) statement instead",
                a.name
            ),
        ));
    }
}

fn validate_if(i: &If, diags: &mut Vec<Diagnostic>) {
    for s in &i.condition {
        validate_stmt_ctx(s, diags, true);
    }
    for s in &i.then_body {
        validate_stmt_ctx(s, diags, false);
    }
    for elif in &i.elif_clauses {
        for s in &elif.condition {
            validate_stmt_ctx(s, diags, true);
        }
        for s in &elif.body {
            validate_stmt_ctx(s, diags, false);
        }
    }
    if let Some(ref else_body) = i.else_body {
        for s in else_body {
            validate_stmt_ctx(s, diags, false);
        }
    }
}

fn validate_while(w: &While, diags: &mut Vec<Diagnostic>) {
    for s in &w.condition {
        validate_stmt_ctx(s, diags, true);
    }
    for s in &w.body {
        validate_stmt_ctx(s, diags, false);
    }
}

fn validate_for(f: &For, diags: &mut Vec<Diagnostic>) {
    // Check glob patterns and unquoted expansions in for-loop words
    for w in &f.words {
        check_unquoted_glob(w, diags);
        // SB5009: unquoted $var in for-loop words implies word splitting
        if let ExprKind::SimpleExpansion(var) = &w.kind {
            diags.push(Diagnostic::error_span(
                w.span,
                "SB5009",
                format!(
                    "unquoted ${} in for-loop would word-split — quote it or use an array",
                    var
                ),
            ));
        }
    }
    for s in &f.body {
        validate_stmt_ctx(s, diags, false);
    }
}

/// DD#31: a comma expression like `(a=1, x<5)` desugars to a
/// Group whose body is a sequence of mutations followed by a
/// final ArithTest. In condition context, the mutations must
/// validate as *non*-condition (they're statements), while the
/// trailing ArithTest validates as condition. Outside that
/// shape, forward `in_condition` unchanged to every body
/// statement.
fn validate_group(g: &Group, in_condition: bool, diags: &mut Vec<Diagnostic>) {
    let is_comma_pattern = in_condition
        && g.body
            .last()
            .is_some_and(|s| matches!(s.kind, StmtKind::ArithTest(_)));
    if !is_comma_pattern {
        for s in &g.body {
            validate_stmt_ctx(s, diags, in_condition);
        }
        return;
    }
    let last = g.body.len() - 1;
    for (i, s) in g.body.iter().enumerate() {
        validate_stmt_ctx(s, diags, i == last);
    }
}

// ── SB5001/SB5002/SB4001: command-level rejections ─────────────

fn validate_command(c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let name = expr_literal_str(&c.name);

    if check_heredoc(name, c, span, diags) {
        return;
    }
    check_export_or_declare(name, c, span, diags);
    check_dynexec(name, span, diags);
    check_test_constructs(name, c, span, diags);
}

fn heredoc_diag(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5002",
        "heredocs (<< / <<-) are not supported in sash — use herestrings (<<<) or variables"
            .to_string(),
    )
}

/// SB5002: reject heredocs as the command name or any argument.
/// Returns true if a heredoc was pushed (caller should stop further checks).
fn check_heredoc(name: &str, c: &Command, span: Span, diags: &mut Vec<Diagnostic>) -> bool {
    if name == "__HEREDOC__" {
        diags.push(heredoc_diag(span));
        return true;
    }
    for arg in &c.args {
        if let ExprKind::Literal(s) = &arg.kind {
            if s == "__HEREDOC__" {
                diags.push(heredoc_diag(span));
                return true;
            }
        }
    }
    false
}

/// SB5001: reject `export` and `declare -x`.
fn check_export_or_declare(name: &str, c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    if name == "export" {
        diags.push(Diagnostic::error_span(
            span,
            "SB5001",
            "'export' is not allowed — use int/const/array/map instead".to_string(),
        ));
    } else if name == "declare" {
        for arg in &c.args {
            let s = expr_literal_str(arg);
            if s == "-x" || s.contains('x') && s.starts_with('-') {
                diags.push(Diagnostic::error_span(
                    span,
                    "SB5001",
                    "'declare -x' is not allowed — use int/const/array/map instead".to_string(),
                ));
            }
        }
    }
}

/// SB4001: reject dynamic-execution builtins.
fn check_dynexec(name: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    if name == "eval" || name == "source" || name == "." || name == "command" {
        diags.push(Diagnostic::error_span(
            span,
            "SB4001",
            format!("'{}' is not allowed in sash", name),
        ));
    } else if name == "coproc" {
        diags.push(Diagnostic::error_span(
            span,
            "SB4001",
            "'coproc' is not allowed in sash".to_string(),
        ));
    }
}

/// SB5019/SB5023: numeric test ops + literal regex validation inside `[[ ]]`.
fn check_test_constructs(name: &str, c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    if name != "[[" {
        return;
    }
    for arg in &c.args {
        if let ExprKind::Test(ref test) = arg.kind {
            check_numeric_test_ops(test, diags);
            check_regex_literal(test, span, diags);
        }
    }
}

fn validate_declaration(d: &Declaration, span: Span, diags: &mut Vec<Diagnostic>) {
    // SB5001: export/declare -x
    if matches!(d.keyword, DeclKeyword::Export) {
        diags.push(Diagnostic::error_span(
            span,
            "SB5001",
            "'export' is not allowed — use int/const/array/map instead".to_string(),
        ));
    }
}

fn validate_assignment(a: &Assignment, span: Span, diags: &mut Vec<Diagnostic>) {
    // SB5009: IFS assignment rejected
    if a.name == "IFS" {
        diags.push(Diagnostic::error_span(
            span,
            "SB5009",
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
                diags.push(Diagnostic::error_span(
                    expr.span,
                    "SB5006",
                    format!(
                        "** globstar is not supported in sash — only * is allowed: {}",
                        s
                    ),
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
    (
        "HOSTNAME",
        "$HOSTNAME does not exist in sash — use $(getenv HOSTNAME)",
    ),
    ("SECONDS", "$SECONDS does not exist in sash"),
    ("EPOCHSECONDS", "$EPOCHSECONDS does not exist in sash"),
    ("EPOCHREALTIME", "$EPOCHREALTIME does not exist in sash"),
    ("SRANDOM", "$SRANDOM does not exist in sash"),
    (
        "IFS",
        "IFS does not exist in sash — word splitting is eliminated by design",
    ),
];

const REJECTED_POSITIONAL: &[(&str, &str)] =
    &[("0", "$0 is not available in sash — use $(argv 0)")];

/// SB5018: Bare env vars require $(getenv NAME)
const ENV_VARS: &[&str] = &[
    "HOME",
    "PATH",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "DISPLAY",
    "XDG_RUNTIME_DIR",
    "TMPDIR",
    "EDITOR",
    "VISUAL",
    "PAGER",
    "LOGNAME",
    "MAIL",
    "PWD",
    "OLDPWD",
];

fn validate_var_name(var: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    // SB5017: Rejected vars
    for (name, msg) in REJECTED_VARS {
        if var == *name {
            diags.push(Diagnostic::error_span(span, "SB5017", msg.to_string()));
            return;
        }
    }
    for (name, msg) in REJECTED_POSITIONAL {
        if var == *name {
            diags.push(Diagnostic::error_span(span, "SB5017", msg.to_string()));
            return;
        }
    }
    // SB5018: Bare env vars
    for name in ENV_VARS {
        if var == *name {
            diags.push(Diagnostic::error_span(
                span,
                "SB5018",
                format!(
                    "${} is not available directly — use $(getenv {})",
                    name, name
                ),
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

/// Check if an expression tree references a specific variable name.
fn expr_references_var(expr: &Expr, var: &str) -> bool {
    match &expr.kind {
        ExprKind::SimpleExpansion(v) => v == var,
        ExprKind::ParameterExpansion(pe) => pe.name == var,
        ExprKind::DoubleQuoted(parts) => parts.iter().any(|p| expr_references_var(p, var)),
        ExprKind::Concat(parts) => parts.iter().any(|p| expr_references_var(p, var)),
        ExprKind::Test(t) => test_references_var(t, var),
        ExprKind::Subscript { name, .. } => name == var,
        _ => false,
    }
}

fn test_references_var(test: &TestExpr, var: &str) -> bool {
    match test {
        // For binary tests, variable MUST be the LHS
        TestExpr::Binary { left, .. } => expr_references_var(left, var),
        TestExpr::Unary { operand, .. } => expr_references_var(operand, var),
        TestExpr::Not(inner) => test_references_var(inner, var),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            test_references_var(l, var) || test_references_var(r, var)
        }
        TestExpr::Paren(inner) => test_references_var(inner, var),
    }
}

/// SB5023: validate literal regex patterns at compile time
fn check_regex_literal(test: &TestExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    if let TestExpr::Binary { op, right, .. } = test {
        if op == "=~" {
            // Only validate literal patterns (no variable interpolation)
            if let ExprKind::Literal(pattern) = &right.kind {
                if !pattern.contains('$') {
                    if let Err(msg) = crate::regex_validate::validate_regex(pattern) {
                        diags.push(Diagnostic::error_span(
                            span,
                            "SB5023",
                            format!("invalid regex pattern: {}", msg),
                        ));
                    }
                }
            }
        }
    }
}

/// SB5019: reject numeric comparison operators in [[ ]]
fn check_numeric_test_ops(test: &TestExpr, diags: &mut Vec<Diagnostic>) {
    match test {
        TestExpr::Binary { left, op, right: _ } => {
            const NUMERIC_OPS: &[&str] = &["-gt", "-lt", "-ge", "-le", "-eq", "-ne"];
            if NUMERIC_OPS.contains(&op.as_str()) {
                let span = left.span;
                diags.push(Diagnostic::error_span(span, "SB5019",
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

// ── SB5007: arithmetic context validation ───────────────────────
//
// Rules:
//   - In condition (if/while): mutations are rejected
//   - As bare statement: comparisons are rejected (no-op)
//   - Prefix ++/-- always rejected (use postfix or explicit)
//   - DD#31: comma expressions allowed if non-final=mutation, final=comparison

/// SB5007: validate arithmetic expression in context.
fn validate_arith_context(
    expr: &ArithExpr,
    span: Span,
    in_condition: bool,
    diags: &mut Vec<Diagnostic>,
) {
    use crate::parse::classify_arith_expr;

    // Prefix ++/-- always rejected
    if let ArithExpr::PreIncDec { var, .. } = expr {
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "prefix ++{} is not allowed — use postfix ({}++) or (( {} += 1 )) instead",
                var, var, var
            ),
        ));
        return;
    }

    let kind = classify_arith_expr(expr);
    if in_condition {
        validate_arith_in_condition(expr, kind, span, diags);
    } else {
        validate_arith_as_statement(expr, kind, span, diags);
    }
}

/// In condition context (`if (( … ))`, `while (( … ))`, etc.):
/// DD#31 permits comma expressions whose non-final parts are
/// mutations and final part is a comparison. Outside that shape,
/// a bare mutation is rejected (SB5007).
fn validate_arith_in_condition(
    expr: &ArithExpr,
    kind: crate::parse::ArithKind,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    use crate::parse::ArithKind;

    if let ArithExpr::Comma(exprs) = expr {
        validate_arith_comma_in_condition(exprs, span, diags);
        return;
    }
    if kind == ArithKind::Mutation {
        let desc = arith_brief(expr);
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "mutation {} not allowed in condition (( )) expression context — \
                 use (( )) statement instead",
                desc
            ),
        ));
    }
}

/// DD#31 comma expression in condition context: every non-final
/// part must be a mutation; the final part must be a comparison
/// (not a mutation). Empty comma lists are no-ops.
fn validate_arith_comma_in_condition(exprs: &[ArithExpr], span: Span, diags: &mut Vec<Diagnostic>) {
    use crate::parse::{classify_arith_expr, ArithKind};

    let Some((last, init)) = exprs.split_last() else {
        return;
    };
    for e in init {
        if classify_arith_expr(e) != ArithKind::Mutation {
            let desc = arith_brief(e);
            diags.push(Diagnostic::error_span(
                span,
                "SB5007",
                format!("non-final comma expression {} must be a mutation", desc),
            ));
        }
    }
    if classify_arith_expr(last) == ArithKind::Mutation {
        let desc = arith_brief(last);
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "final comma expression {} must be a comparison in condition context",
                desc
            ),
        ));
    }
}

/// As a bare statement (`(( … ))` standalone): a comparison is a
/// no-op (the program does nothing with the boolean); flag it
/// (SB5007). Also check for nested mutations like `x = y++` in
/// the assignment RHS.
fn validate_arith_as_statement(
    expr: &ArithExpr,
    kind: crate::parse::ArithKind,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    use crate::parse::ArithKind;

    if kind == ArithKind::Comparison {
        let desc = arith_brief(expr);
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "comparison {} as bare statement has no effect — \
                 use in if/while condition",
                desc
            ),
        ));
    }
    check_arith_nested_mutation(expr, span, diags);
}

/// Check for nested mutations like `x = y++` in statement arithmetic
fn check_arith_nested_mutation(expr: &ArithExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    if let ArithExpr::Assign { value, .. } = expr {
        // Check if RHS has side effects
        if has_mutation(value) {
            let desc = arith_brief(value);
            diags.push(Diagnostic::error_span(span, "SB5007",
                format!("mutation {} not allowed in statement (( )) expression context — use (( )) statement instead", desc),
            ));
        }
    }
}

fn has_mutation(expr: &ArithExpr) -> bool {
    match expr {
        ArithExpr::PostIncDec { .. }
        | ArithExpr::PreIncDec { .. }
        | ArithExpr::Assign { .. }
        | ArithExpr::CompoundAssign { .. } => true,
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
                IrArithOp::Add => "+=",
                IrArithOp::Sub => "-=",
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
            format!(
                "{} {} {}",
                arith_brief(left),
                match op {
                    IrArithOp::Gt => ">",
                    IrArithOp::Lt => "<",
                    IrArithOp::Eq => "==",
                    IrArithOp::Ne => "!=",
                    IrArithOp::Ge => ">=",
                    IrArithOp::Le => "<=",
                    _ => "?",
                },
                arith_brief(right)
            )
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
                diags.push(Diagnostic::error_span(
                    expr.span,
                    "SB5006",
                    format!("? glob is not supported in sash — only * is allowed: {}", s),
                ));
            } else if s.contains('[') && s.contains(']') {
                diags.push(Diagnostic::error_span(
                    expr.span,
                    "SB5006",
                    format!(
                        "[...] glob is not supported in sash — only * is allowed: {}",
                        s
                    ),
                ));
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                if let ExprKind::Literal(s) = &p.kind {
                    if s.contains('?') {
                        diags.push(Diagnostic::error_span(
                            p.span,
                            "SB5006",
                            format!("? glob is not supported in sash — only * is allowed: {}", s),
                        ));
                    } else if s.contains('[') && s.contains(']') {
                        diags.push(Diagnostic::error_span(
                            p.span,
                            "SB5006",
                            format!(
                                "[...] glob is not supported in sash — only * is allowed: {}",
                                s
                            ),
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
    crate::ir::walk_all_exprs(stmt, &mut |expr| f(expr, expr.span));
}

// ── SB5012: $? availability state machine ─────────────────────
//
// States: Undefined → Available (after non-suppressed command) → Undefined (consumed)
// In || branches: inherited status available only for first statement.
// SECURITY: prevents use-after-consume and uninitialized $? reads.

/// $? state machine: undefined → available (after command) → undefined (consumed)
#[derive(Clone, Copy, PartialEq)]
enum StatusState {
    Undefined,
    Available,
}

/// Check $? usage across a statement list.
fn check_dollar_question(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    let mut state = StatusState::Undefined;
    for stmt in stmts {
        check_dq_stmt(stmt, &mut state, diags);
    }
}

fn check_dq_stmt(stmt: &Statement, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::List(l) => check_dq_list(l, state, diags),
        StmtKind::Group(g) => {
            for s in &g.body {
                check_dq_stmt(s, state, diags);
            }
        }
        StmtKind::Function(f) => check_dq_function(f, diags),
        StmtKind::If(i) => check_dq_if(i, state, diags),
        StmtKind::While(w) => check_dq_while(w, state, diags),
        StmtKind::For(f) => check_dq_for(f, state, diags),
        StmtKind::Case(c) => check_dq_case(c, state, diags),
        _ => check_dq_leaf(stmt, state, diags),
    }
}

/// `a && b` / `a || b`: left runs normally, right is a branch so
/// inherited $? is available only for the first statement.
fn check_dq_list(l: &crate::ir::List, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    check_dq_stmt(&l.left, state, diags);
    let mut branch_state = StatusState::Available;
    check_dq_stmt_branch(&l.right, &mut branch_state, diags);
    *state = StatusState::Undefined;
}

/// A function body is checked in a fresh `Undefined` status
/// context (the caller's $? state does not flow in).
fn check_dq_function(f: &crate::ir::Function, diags: &mut Vec<Diagnostic>) {
    let mut fn_state = StatusState::Undefined;
    for s in &f.body {
        check_dq_stmt(s, &mut fn_state, diags);
    }
}

/// `if … then … (elif …)* (else …)? fi`: walk the condition with
/// the inherited state, then each branch body with the
/// first-statement-only rule. After the construct, state is
/// undefined regardless of which branch ran.
fn check_dq_if(i: &crate::ir::If, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    for s in &i.condition {
        check_dq_stmt(s, state, diags);
    }
    check_dq_body_branch(&i.then_body, *state, diags);
    for elif in &i.elif_clauses {
        let mut es = *state;
        for s in &elif.condition {
            check_dq_stmt(s, &mut es, diags);
        }
        check_dq_body_branch(&elif.body, es, diags);
    }
    if let Some(ref eb) = i.else_body {
        check_dq_body_branch(eb, *state, diags);
    }
    *state = StatusState::Undefined;
}

/// `while … do … done`: condition uses inherited state, body
/// re-reads it fresh for each iteration. After the loop, state
/// is undefined.
fn check_dq_while(w: &crate::ir::While, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    for s in &w.condition {
        check_dq_stmt(s, state, diags);
    }
    let mut bs = *state;
    for s in &w.body {
        check_dq_stmt(s, &mut bs, diags);
    }
    *state = StatusState::Undefined;
}

/// `for x in … do … done`: word list does not consume $? (literal/
/// expansion only). Body re-reads inherited state fresh for each
/// iteration. After the loop, state is undefined — for is a
/// catching construct (caught/uncaught model).
fn check_dq_for(f: &crate::ir::For, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    let mut bs = *state;
    for s in &f.body {
        check_dq_stmt(s, &mut bs, diags);
    }
    *state = StatusState::Undefined;
}

/// `case word in pat) body ;; … esac`: scrutinee does not consume
/// $? (it's a word). Each arm body re-reads inherited state fresh.
/// After the case, state is undefined — case is a catching construct.
fn check_dq_case(c: &crate::ir::Case, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    for item in &c.items {
        let mut bs = *state;
        for s in &item.body {
            check_dq_stmt(s, &mut bs, diags);
        }
    }
    *state = StatusState::Undefined;
}

/// Default arm: a leaf-ish statement (Command, Pipeline, etc.).
/// Two responsibilities: (1) if it consumes `$?`, raise SB5012
/// when the status is unavailable; (2) update state to reflect
/// what this statement does (sets, consumes, or neither).
fn check_dq_leaf(stmt: &Statement, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    if stmt_uses_status(stmt) {
        if *state != StatusState::Available {
            let mut spans = Vec::new();
            find_dollar_question(stmt, &mut spans);
            for span in spans {
                diags.push(Diagnostic::error_span(
                    span,
                    "SB5012",
                    "$? is not available here — no preceding command".to_string(),
                ));
            }
        }
        *state = StatusState::Undefined;
    }
    if is_suppressed(stmt) {
        *state = StatusState::Undefined;
    } else if is_command_like(stmt) || is_assignment_from_arith(stmt) {
        *state = StatusState::Available;
    }
}

fn find_dollar_question(stmt: &Statement, spans: &mut Vec<Span>) {
    walk_exprs_in_stmt(stmt, &mut |expr, _| {
        if let ExprKind::SimpleExpansion(var) = &expr.kind {
            if var == "?" {
                spans.push(expr.span);
            }
        }
    });
}

fn stmt_uses_status(stmt: &Statement) -> bool {
    let mut found = false;
    walk_exprs_in_stmt(stmt, &mut |expr, _| {
        if let ExprKind::SimpleExpansion(var) = &expr.kind {
            if var == "?" {
                found = true;
            }
        }
    });
    found
}

fn is_suppressed(stmt: &Statement) -> bool {
    matches!(&stmt.kind, StmtKind::Suppressed(_))
}

fn is_command_like(stmt: &Statement) -> bool {
    matches!(
        &stmt.kind,
        StmtKind::Command(_) | StmtKind::Pipeline(_) | StmtKind::List(_) | StmtKind::ArithTest(_)
    )
}

fn is_assignment_from_arith(stmt: &Statement) -> bool {
    if let StmtKind::Assignment(a) = &stmt.kind {
        matches!(&a.value.kind, ExprKind::Arithmetic(_))
    } else {
        false
    }
}

/// In || branches, status is available only for the first statement.
/// In handling branches (||, if/else), inherited status available for first statement only.
/// After that, the inherited status is gone.
fn check_dq_stmt_branch(stmt: &Statement, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Group(g) => {
            check_dq_body_branch(&g.body, *state, diags);
            *state = StatusState::Undefined;
        }
        _ => {
            // Single statement branch: normal check
            check_dq_stmt(stmt, state, diags);
        }
    }
}

/// Apply first-statement-only rule to a body (Vec<Statement>).
fn check_dq_body_branch(stmts: &[Statement], state: StatusState, diags: &mut Vec<Diagnostic>) {
    for (i, s) in stmts.iter().enumerate() {
        if i == 0 {
            let mut first_state = state;
            check_dq_stmt(s, &mut first_state, diags);
        } else {
            // After first statement, inherited status is consumed
            let mut inner = StatusState::Undefined;
            check_dq_stmt(s, &mut inner, diags);
        }
    }
}

// ── SB5011: Command resolution ─────────────────────────────────

const BUILTINS: &[&str] = &[
    // Output
    "echo",
    // Control flow
    "true",
    "false",
    ":",
    "exit",
    "return",
    "break",
    "continue",
    // Input
    "read",
    "mapfile",
    // Taint
    "bless",
    "getenv",
    // Positional/args
    "argv",
    "argc",
    "shift",
    // Variables
    "unset",
    // Test
    "[[",
    // Process/signal
    "trap",
    "signal",
    "wait",
    "kill",
    "signum",
    // Filesystem
    "cd",
    // Introspection
    "status",
    "getpid",
    "getppid",
    "getcpid",
    "getcwd",
    "epochseconds",
    // Conversion
    "dtoi",
    "itod",
];

fn collect_functions(stmts: &[Statement], funcs: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Function(f) => {
                funcs.insert(f.name.clone());
                // Local functions have __local__ prefix; also register unprefixed
                if let Some(short) = f.name.strip_prefix("__local__") {
                    funcs.insert(short.to_string());
                }
                // Also collect nested (local) functions
                collect_functions(&f.body, funcs);
            }
            StmtKind::If(i) => {
                collect_functions(&i.then_body, funcs);
                for elif in &i.elif_clauses {
                    collect_functions(&elif.body, funcs);
                }
                if let Some(ref eb) = i.else_body {
                    collect_functions(eb, funcs);
                }
            }
            StmtKind::While(w) => collect_functions(&w.body, funcs),
            StmtKind::For(f) => collect_functions(&f.body, funcs),
            StmtKind::Group(g) => collect_functions(&g.body, funcs),
            _ => {}
        }
    }
}

fn check_commands(stmts: &[Statement], funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_cmd_in_stmt(stmt, funcs, diags);
    }
}

fn check_cmd_in_stmt(stmt: &Statement, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let StmtKind::Command(c) = &stmt.kind else {
        for body in stmt.child_bodies() {
            check_commands(body, funcs, diags);
        }
        return;
    };
    if is_regex_mutation(c) {
        return;
    }
    let name = expr_literal_str(&c.name);
    if name.is_empty() || is_valid_command(name, funcs) {
        return;
    }
    diags.push(Diagnostic::error_span(
        stmt.span,
        "SB5011",
        format!(
            "command '{}' not found — use absolute path or define as function",
            name
        ),
    ));
}

/// Skip pseudo-commands produced by the `var =~ s/pat/rep/` lowering:
/// modern form is `Command{name=var, args=["s/..."]}` and legacy form
/// is the two-arg `var =~ ...`. Either way they're not real commands.
fn is_regex_mutation(c: &Command) -> bool {
    if c.args.len() == 1 {
        if let ExprKind::Literal(s) = &c.args[0].kind {
            if s.starts_with("s/") {
                return true;
            }
        }
    }
    c.args.first().is_some_and(|a| expr_literal_str(a) == "=~")
}

fn is_valid_command(name: &str, funcs: &HashSet<String>) -> bool {
    // Absolute path: always valid
    if name.starts_with('/') {
        return true;
    }
    // Builtin
    if BUILTINS.contains(&name) {
        return true;
    }
    // Declared function
    if funcs.contains(name) {
        return true;
    }
    // Glob pattern (e.g. /usr/bin/*) — not a command
    if name.contains('*') {
        return true;
    }
    // PATH-resolvable: check if command exists on system PATH
    if resolve_on_path(name) {
        return true;
    }
    false
}

fn resolve_on_path(name: &str) -> bool {
    // Don't resolve names with special characters
    if name.contains('/') || name.contains('$') {
        return false;
    }
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':') {
        let full = format!("{}/{}", dir, name);
        if std::path::Path::new(&full).exists() {
            return true;
        }
    }
    false
}

// ── Taint tracking (SB5020) ───────────────────────────────────────
//
// Forward dataflow analysis: track which variables hold tainted data.
// A variable is tainted if its value came from:
//   - $(getenv NAME), $(argv N)  — external input
//   - $(/absolute/path ...)      — external command output
//   - read from stdin (no herestring)
// A variable is clean if:
//   - assigned from literals, arithmetic, clean variables
//   - read from herestring (<<<) — data came from sash code
//   - function parameters ($1-$9) inside function bodies
//   - bless explicitly removes taint after validation
//
// SECURITY: This is definition-based, NOT name-based. There is no
// "clean list" of variable names. Taint flows through assignment.

use std::collections::HashSet;

/// Run taint analysis: track which variables hold tainted data,
/// reject any use of tainted variables outside bless expressions.
pub fn check_taint(program: &Program) -> Vec<Diagnostic> {
    let mut tainted: HashSet<String> = HashSet::new();
    // $1-$9 at top level are implicitly tainted (from argv)
    for i in 1..=9 {
        tainted.insert(i.to_string());
    }
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
fn taint_stmt_no_read_taint(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if let StmtKind::Command(c) = &stmt.kind {
        let name = expr_literal_str(&c.name);
        // read/mapfile from clean pipe source: don't taint
        if name == "read" || name == "mapfile" {
            check_expr_taint(&c.name, tainted, diags);
            for arg in &c.args {
                check_expr_taint(arg, tainted, diags);
            }
            return;
        }
    }
    taint_stmt(stmt, tainted, diags);
}

/// Check if a pipeline stage is an external source (execs).
fn is_external_source(stmt: &Statement) -> bool {
    match &stmt.kind {
        StmtKind::Command(c) => {
            let name = expr_literal_str(&c.name);
            // Absolute path = external
            if name.starts_with('/') {
                return true;
            }
            // PATH-resolved non-builtin = external
            if !name.is_empty() && !BUILTINS.contains(&name) {
                return true;
            }
            false
        }
        StmtKind::Redirected(r) => is_external_source(&r.body),
        _ => false,
    }
}

fn taint_stmt(stmt: &Statement, tainted: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Assignment(a) => taint_assignment(a, tainted, diags),
        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                taint_assignment(a, tainted, diags);
            }
        }
        StmtKind::Command(c) => taint_command(c, stmt.span, tainted, diags),
        StmtKind::Function(f) => taint_function_body(&f.body, tainted, diags),
        StmtKind::Pipeline(p) => taint_pipeline(p, tainted, diags),
        StmtKind::Redirected(r) => taint_redirected(r, tainted, diags),
        StmtKind::Case(c) => taint_case_arms(c, tainted, diags),
        _ => {
            for body in stmt.child_bodies() {
                for s in body {
                    taint_stmt(s, tainted, diags);
                }
            }
        }
    }
}

/// Apply taint rules to an assignment.
///
/// DD#61: a tainted value may not enter an array through program syntax
/// (array/map literal, `+=` append of a literal, or `a[i]=` subscript
/// assign). Such introductions are rejected (SB5021), not quarantined —
/// tainted arrays come only from bulk external reads (read -a / mapfile).
/// Scalar assignment from a tainted source follows the normal flow:
/// the scalar becomes tainted.
fn taint_assignment(a: &Assignment, tainted: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &a.value.kind {
        ExprKind::Array(elems) => {
            for e in elems {
                reject_tainted_into_array(e, &a.name, tainted, diags);
            }
            return;
        }
        ExprKind::AssocArray(pairs) => {
            for (_k, v) in pairs {
                reject_tainted_into_array(v, &a.name, tainted, diags);
            }
            return;
        }
        _ => {}
    }
    // `a[i]=value` — subscript assignment into an array slot.
    if a.index.is_some() {
        reject_tainted_into_array(&a.value, &a.name, tainted, diags);
        return;
    }
    // Scalar (or string `+=`) assignment: normal taint propagation.
    if is_taint_source(&a.value) || expr_uses_tainted(&a.value, tainted) {
        tainted.insert(a.name.clone());
    }
}

/// DD#61: reject a tainted value being placed into an array element.
fn reject_tainted_into_array(
    e: &Expr,
    name: &str,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if is_taint_source(e) || expr_uses_tainted(e, tainted) {
        diags.push(Diagnostic::error_span(
            e.span,
            "SB5021",
            format!(
                "tainted value cannot be placed in array '{}' — external data enters \
                 arrays via read/mapfile; or bless a scalar first and assign the clean value",
                name
            ),
        ));
    }
}

/// Taint a function body: $1-$9 are function args (clean), not top-level argv.
fn taint_function_body(body: &[Statement], tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let mut fn_tainted = tainted.clone();
    for i in 1..=9 {
        fn_tainted.remove(&i.to_string());
    }
    for s in body {
        taint_stmt(s, &mut fn_tainted, diags);
    }
}

/// Taint a pipeline: if first stage is external, subsequent reads are tainted.
fn taint_pipeline(
    p: &crate::ir::Pipeline,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let first_is_external = p.commands.first().is_some_and(is_external_source);
    for (i, s) in p.commands.iter().enumerate() {
        if i > 0 && !first_is_external {
            taint_stmt_no_read_taint(s, tainted, diags);
        } else {
            taint_stmt(s, tainted, diags);
        }
    }
}

/// Taint a redirected statement, with a `read … <<< "literal"` exception
/// (herestring/heredoc sources are sash code, not external input).
fn taint_redirected(
    r: &crate::ir::Redirected,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if let StmtKind::Command(c) = &r.body.kind {
        let name = expr_literal_str(&c.name);
        if (name == "read" || name == "mapfile")
            && r.redirects.iter().any(|rd| {
                matches!(
                    rd.op,
                    crate::ir::RedirectOp::HereString | crate::ir::RedirectOp::HereDoc
                )
            })
        {
            return;
        }
    }
    taint_stmt(&r.body, tainted, diags);
}

/// Taint a case statement's discriminant and each arm body.
fn taint_case_arms(
    c: &crate::ir::Case,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    check_expr_taint(&c.value, tainted, diags);
    for item in &c.items {
        for s in &item.body {
            taint_stmt(s, tainted, diags);
        }
    }
}

/// Taint analysis for a command statement: read/mapfile/bless + general taint checking.
fn taint_command(
    c: &Command,
    span: Span,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let name = expr_literal_str(&c.name);

    if name == "read" || name == "mapfile" {
        taint_read_target(c, tainted);
    } else if name == "bless" {
        taint_bless(c, span, tainted, diags);
        return; // Don't check args of bless for taint
    }

    // Check all command arguments for tainted variable use
    check_expr_taint(&c.name, tainted, diags);
    for arg in &c.args {
        check_expr_taint(arg, tainted, diags);
    }
}

/// `read VAR` / `mapfile VAR`: mark the rightmost non-flag argument
/// as the variable being assigned, so it becomes tainted.
fn taint_read_target(c: &Command, tainted: &mut HashSet<String>) {
    for arg in c.args.iter().rev() {
        let s = expr_literal_str(arg);
        if !s.starts_with('-') && !s.is_empty() {
            tainted.insert(s.to_string());
            return;
        }
    }
}

/// `bless` taint handling for all three forms (DD#24 scalar, DD#61
/// indexed array and map). Removes taint from the blessed entity if the
/// predicate validates the required name(s); otherwise emits SB5020.
///
/// - Scalar `bless v [[…]]`: predicate must reference `$v` (on the LHS).
/// - Indexed `bless x in a [[…]]`: predicate must reference the binder
///   `$x`; clears taint on `a`.
/// - Map `bless k v in m [[…]]`: predicate must reference BOTH binders
///   `$k` and `$v` (a key escaping unvalidated would be unsound); clears
///   taint on `m`.
fn taint_bless(
    c: &Command,
    span: Span,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let Some((form, test)) = classify_bless(&c.args) else {
        return;
    };
    // Each form reduces to: the binder name(s) the predicate must validate,
    // the name whose taint clears on success, and the diagnostic otherwise.
    let (binders, clear, msg): (Vec<&str>, &str, String) = match form {
        BlessForm::Scalar { var } => (
            vec![var],
            var,
            format!("bless condition must validate ${var} — condition doesn't reference the variable"),
        ),
        BlessForm::Indexed { binder, array } => (
            vec![binder],
            array,
            format!("bless-all condition must validate each element ${binder} on the left of the test"),
        ),
        BlessForm::Map { key, val, map } => (
            vec![key, val],
            map,
            format!("map bless-all must validate both the key ${key} and value ${val} on the left of a test"),
        ),
    };
    if binders.iter().all(|b| test_references_var(test, b)) {
        tainted.remove(clear);
    } else {
        diags.push(Diagnostic::error_span(span, "SB5020", msg));
    }
}

/// Check if an expression is a taint source.
///
/// Recurses into string concatenation, double-quoted parts, and array /
/// map literals so an embedded `$(getenv …)` / `$(argv …)` / external
/// command substitution is detected wherever it appears (DD#61).
fn is_taint_source(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::CommandSubstitution(stmts) => cmdsub_is_taint_source(stmts),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            parts.iter().any(is_taint_source)
        }
        ExprKind::AssocArray(pairs) => pairs.iter().any(|(_, v)| is_taint_source(v)),
        _ => false,
    }
}

/// A command substitution is a taint source when it runs `getenv`/`argv`
/// or an external (absolute-path) command. Unknown PATH-resolved names
/// can't be classified here (might be a user function), so default clean.
fn cmdsub_is_taint_source(stmts: &[Statement]) -> bool {
    let Some(StmtKind::Command(c)) = stmts.first().map(|s| &s.kind) else {
        return false;
    };
    let name = expr_literal_str(&c.name);
    name == "getenv" || name == "argv" || name.starts_with('/')
}

/// Check if an expression uses any tainted variable.
fn expr_uses_tainted(expr: &Expr, tainted: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => tainted.contains(var.as_str()),
        ExprKind::ParameterExpansion(pe) => tainted.contains(pe.name.as_str()),
        ExprKind::Subscript { name, .. } => tainted.contains(name.as_str()),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            parts.iter().any(|p| expr_uses_tainted(p, tainted))
        }
        ExprKind::AssocArray(pairs) => pairs.iter().any(|(_, v)| expr_uses_tainted(v, tainted)),
        _ => false,
    }
}

/// Check an expression for tainted variable use, emitting diagnostics.
fn check_expr_taint(expr: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => {
            if tainted.contains(var.as_str()) {
                diags.push(Diagnostic::error_span(
                    expr.span,
                    "SB5020",
                    format!("tainted variable ${} used without bless", var),
                ));
            }
        }
        ExprKind::ParameterExpansion(pe) => {
            if tainted.contains(pe.name.as_str()) {
                diags.push(Diagnostic::error_span(
                    expr.span,
                    "SB5020",
                    format!("tainted variable ${{{} ...}} used without bless", pe.name),
                ));
            }
        }
        ExprKind::Subscript { name, .. } => {
            if tainted.contains(name.as_str()) {
                diags.push(Diagnostic::error_span(
                    expr.span,
                    "SB5020",
                    format!("tainted array ${{{}[...]}} used without bless", name),
                ));
            }
        }
        ExprKind::DoubleQuoted(parts) => {
            for p in parts {
                check_expr_taint(p, tainted, diags);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                check_expr_taint(p, tainted, diags);
            }
        }
        _ => {}
    }
}
