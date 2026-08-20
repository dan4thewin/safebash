//! Structural janet backend (DD#15, amended 2026-08-20): render the sash AST
//! 1-to-1 into a `sash/*` sexpr vocabulary so the emitted janet reads as a
//! direct transliteration of how the compiler parsed the source — a map you
//! can diff against the sash side-by-side. Semantics live in `sash-rt.janet`;
//! this stays structural. Position-based interpretation (a bare checked
//! command vs a caught condition) is made visible: `sash/run` vs `sash/ok?`.
//!
//! STAGE 1: assignments, echo, `:`, external commands, if/while, `[[ ]]` string
//! tests, `$(…)` single-external capture, bless. Anything not yet handled
//! renders as a visible `(sash/TODO …)` so nothing is silently wrong.

use crate::ir::*;
use std::collections::BTreeSet;
use std::fmt::Write;

/// Render a program as the structural map (the `sash-rt.janet` preamble is
/// prepended by the caller).
pub fn emit_janet_map(program: &Program) -> String {
    let mut out = String::new();
    let mut vars: BTreeSet<String> = BTreeSet::new();
    collect_vars(&program.statements, &mut vars);
    for v in &vars {
        let _ = writeln!(out, "(var {} nil)", v);
    }
    if !vars.is_empty() {
        out.push('\n');
    }
    for stmt in &program.statements {
        let s = render_stmt(stmt, 0);
        if !s.trim().is_empty() {
            out.push_str(&s);
            out.push('\n');
        }
    }
    out
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

/// Collect every assigned variable name so they can be pre-declared.
fn collect_vars(stmts: &[Statement], acc: &mut BTreeSet<String>) {
    for s in stmts {
        collect_stmt_vars(s, acc);
    }
}

fn collect_stmt_vars(s: &Statement, acc: &mut BTreeSet<String>) {
    match &s.kind {
        StmtKind::Assignment(a) => {
            acc.insert(sanitize(&a.name));
        }
        StmtKind::If(i) => collect_if_vars(i, acc),
        StmtKind::While(w) => {
            collect_vars(&w.condition, acc);
            collect_vars(&w.body, acc);
        }
        StmtKind::Suppressed(inner) => collect_stmt_vars(inner, acc),
        _ => {}
    }
}

fn collect_if_vars(i: &If, acc: &mut BTreeSet<String>) {
    collect_vars(&i.condition, acc);
    collect_vars(&i.then_body, acc);
    for e in &i.elif_clauses {
        collect_vars(&e.condition, acc);
        collect_vars(&e.body, acc);
    }
    if let Some(eb) = &i.else_body {
        collect_vars(eb, acc);
    }
}

/// janet symbols can't be arbitrary; keep names simple (letters/digits/_).
fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true) {
        format!("v_{}", s)
    } else {
        s
    }
}

// ── statements (bare / checked position) ──────────────────────────────────

fn render_stmt(stmt: &Statement, depth: usize) -> String {
    let ind = indent(depth);
    match &stmt.kind {
        StmtKind::Comment(c) => format!("{}# {}", ind, c.trim_start_matches('#').trim()),
        StmtKind::Assignment(a) => {
            format!(
                "{}(set {} {})",
                ind,
                sanitize(&a.name),
                render_expr(&a.value)
            )
        }
        StmtKind::Command(c) => format!("{}{}", ind, render_cmd(c, Ctx::Bare)),
        StmtKind::If(i) => render_if(i, depth),
        StmtKind::While(w) => render_while(w, depth),
        StmtKind::Suppressed(inner) => {
            // `- cmd` — suppressed: render the inner in unchecked position.
            format!(
                "{}(sash/dash {})",
                ind,
                render_stmt_inline(inner, Ctx::Cond)
            )
        }
        other => format!("{}(sash/TODO {:?})", ind, std::mem::discriminant(other)),
    }
}

/// Render a statement inline (no leading indent) in the given context.
fn render_stmt_inline(stmt: &Statement, ctx: Ctx) -> String {
    match &stmt.kind {
        StmtKind::Command(c) => render_cmd(c, ctx),
        StmtKind::Assignment(a) => format!("(set {} {})", sanitize(&a.name), render_expr(&a.value)),
        _ => format!("(sash/TODO {:?})", std::mem::discriminant(&stmt.kind)),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    /// Bare statement — checked: a nonzero external status aborts (DD#55 A).
    Bare,
    /// Condition (if/while) — unchecked: yields a bool, never aborts.
    Cond,
}

fn render_cmd(c: &Command, ctx: Ctx) -> String {
    let name = expr_literal(&c.name);
    match name.as_deref() {
        Some("echo") => format!("(sash/echo{})", render_args(&c.args)),
        Some(":") => "(sash/noop)".to_string(),
        Some("[[") => render_test_cmd(&c.args),
        Some("bless") => render_bless(&c.args, ctx),
        Some(n) => {
            // External command (or an as-yet-unmapped builtin).
            let verb = if ctx == Ctx::Cond {
                "sash/ok?"
            } else {
                "sash/run"
            };
            format!("({} {:?}{})", verb, n, render_args(&c.args))
        }
        None => format!("(sash/TODO dynamic-command {})", render_expr(&c.name)),
    }
}

fn render_bless(args: &[Expr], ctx: Ctx) -> String {
    // `bless VAR TEST` — find the test (a `[[ ]]` shows up as a Test expr).
    let test = args
        .iter()
        .find_map(|a| match &a.kind {
            ExprKind::Test(t) => Some(render_test(t)),
            _ => None,
        })
        .unwrap_or_else(|| "true".to_string());
    if ctx == Ctx::Cond {
        format!("(sash/bless {})", test)
    } else {
        format!("(sash/bless-checked {})", test)
    }
}

fn render_args(args: &[Expr]) -> String {
    let mut s = String::new();
    for a in args {
        s.push(' ');
        s.push_str(&render_expr(a));
    }
    s
}

// ── control flow ──────────────────────────────────────────────────────────

fn render_if(i: &If, depth: usize) -> String {
    let ind = indent(depth);
    let cond = render_cond(&i.condition);
    let mut out = format!("{}(sash/if {}\n", ind, cond);
    out.push_str(&render_body(&i.then_body, depth + 1));
    // elif → nested else-if; else.
    let mut else_src = String::new();
    render_else_chain(&i.elif_clauses, &i.else_body, depth + 1, &mut else_src);
    if !else_src.is_empty() {
        out.push('\n');
        out.push_str(&else_src);
    }
    out.push_str(&format!("{})", ind));
    out
}

fn render_else_chain(
    elifs: &[ElifClause],
    else_body: &Option<Vec<Statement>>,
    depth: usize,
    out: &mut String,
) {
    let ind = indent(depth);
    if let Some((first, rest)) = elifs.split_first() {
        let cond = render_cond(&first.condition);
        let _ = writeln!(out, "{}(sash/if {}", ind, cond);
        out.push_str(&render_body(&first.body, depth + 1));
        let mut inner = String::new();
        render_else_chain(rest, else_body, depth + 1, &mut inner);
        if !inner.is_empty() {
            out.push('\n');
            out.push_str(&inner);
        }
        out.push_str(&format!("{})", ind));
    } else if let Some(eb) = else_body {
        out.push_str(&render_body(eb, depth));
    }
}

fn render_while(w: &While, depth: usize) -> String {
    let ind = indent(depth);
    let cond = render_cond(&w.condition);
    let head = if w.until { "sash/until" } else { "sash/while" };
    let mut out = format!("{}({} {}\n", ind, head, cond);
    out.push_str(&render_body(&w.body, depth + 1));
    out.push_str(&format!("{})", ind));
    out
}

/// Render an if/while condition (a statement list) to a boolean form. Stage 1
/// handles a single-statement condition; compound `a && b` conditions later.
fn render_cond(stmts: &[Statement]) -> String {
    match stmts {
        [one] => render_stmt_inline(one, Ctx::Cond),
        [] => "true".to_string(),
        _ => "(sash/TODO compound-condition)".to_string(),
    }
}

/// Render a body as a `(do …)` block, one statement per line.
fn render_body(stmts: &[Statement], depth: usize) -> String {
    let ind = indent(depth);
    let mut out = format!("{}(do\n", ind);
    for s in stmts {
        out.push_str(&render_stmt(s, depth + 1));
        out.push('\n');
    }
    out.push_str(&format!("{})", ind));
    out
}

// ── [[ ]] tests ─────────────────────────────────────────────────────────

fn render_test_cmd(args: &[Expr]) -> String {
    args.iter()
        .find_map(|a| match &a.kind {
            ExprKind::Test(t) => Some(render_test(t)),
            _ => None,
        })
        .unwrap_or_else(|| "(sash/TODO empty-test)".to_string())
}

fn binary_test_form(op: &str) -> Option<&'static str> {
    Some(match op {
        "=" | "==" => "sash/=",
        "!=" => "sash/not=",
        "-eq" => "sash/-eq",
        "-ne" => "sash/-ne",
        "-lt" => "sash/-lt",
        "-le" => "sash/-le",
        "-gt" => "sash/-gt",
        "-ge" => "sash/-ge",
        _ => return None,
    })
}

fn unary_test_form(op: &str) -> Option<&'static str> {
    Some(match op {
        "-n" => "sash/-n",
        "-z" => "sash/-z",
        _ => return None,
    })
}

fn render_test(t: &TestExpr) -> String {
    match t {
        TestExpr::Binary { left, op, right } => {
            let (l, r) = (render_expr(left), render_expr(right));
            match binary_test_form(op) {
                Some(form) => format!("({} {} {})", form, l, r),
                None => format!("(sash/TODO test-op {:?} {} {})", op, l, r),
            }
        }
        TestExpr::Unary { op, operand } => {
            let v = render_expr(operand);
            match unary_test_form(op) {
                Some(form) => format!("({} {})", form, v),
                None => format!("(sash/TODO test-unary {:?} {})", op, v),
            }
        }
        TestExpr::Not(inner) => format!("(not {})", render_test(inner)),
        TestExpr::And(a, b) => format!("(and {} {})", render_test(a), render_test(b)),
        TestExpr::Or(a, b) => format!("(or {} {})", render_test(a), render_test(b)),
    }
}

// ── expressions / values ──────────────────────────────────────────────────

fn expr_literal(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Literal(s) => Some(s.clone()),
        _ => None,
    }
}

fn render_expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => janet_str(s),
        ExprKind::SimpleExpansion(name) => render_var_read(name),
        ExprKind::DoubleQuoted(parts) => render_dquoted(parts),
        ExprKind::Concat(parts) => render_concat(parts),
        ExprKind::CommandSubstitution(stmts) => render_capture(stmts),
        ExprKind::Test(t) => render_test(t),
        other => format!("(sash/TODO {:?})", std::mem::discriminant(other)),
    }
}

fn render_var_read(name: &str) -> String {
    // $x → ($ x); special vars keep their sigil name for the reader.
    if name == "?" {
        "($ ?)".to_string()
    } else if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
        format!("($ {})", sanitize(name))
    } else {
        format!("(sash/TODO expansion {:?})", name)
    }
}

fn render_dquoted(parts: &[Expr]) -> String {
    match parts {
        [] => "\"\"".to_string(),
        [one] => render_expr(one),
        many => render_concat(many),
    }
}

fn render_concat(parts: &[Expr]) -> String {
    let mut out = String::from("(sash/str");
    for p in parts {
        out.push(' ');
        out.push_str(&render_expr(p));
    }
    out.push(')');
    out
}

fn render_capture(stmts: &[Statement]) -> String {
    // Stage 1: a single external command → piped capture.
    let todo = || "(sash/TODO capture-nonexternal)".to_string();
    let [one] = stmts else { return todo() };
    let StmtKind::Command(c) = &one.kind else {
        return todo();
    };
    let Some(name) = expr_literal(&c.name) else {
        return todo();
    };
    if matches!(name.as_str(), "echo" | ":" | "[[") {
        return todo();
    }
    format!("(sash/capture {:?}{})", name, render_args(&c.args))
}

/// Render a Rust string as a janet string literal.
fn janet_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
