//! Structural janet backend (DD#15, amended 2026-08-20): render the sash AST
//! 1-to-1 into a `sash/*` sexpr vocabulary so the emitted janet reads as a
//! direct transliteration of how the compiler parsed the source — a map you
//! can diff against the sash side-by-side. Semantics live in `sash-rt.janet`;
//! this stays structural. Position-based interpretation (a bare checked
//! command vs a caught condition) is made visible: `sash/run` vs `sash/ok?`.
//!
//! STAGE 1: assignments, echo, `:`, external commands, if/while, `[[ ]]` string
//! tests, `$(…)` single-external capture, bless.
//! STAGE 2: typed declarations (`int n=5` → number), user functions
//! (`sash/defn`), user-function calls (distinct from externals, `sash/true?`
//! in a condition), `return`, function parameters (`$1`→`(sash/param 1)`,
//! `$#`/`$@`) kept distinct from the process argv trio (`$(arg N)`/`$(argc)`/
//! `$(argv)`). Anything not yet handled renders as a visible `(sash/TODO …)`
//! (a preamble macro) so nothing is silently wrong.

use crate::ir::*;
use std::collections::BTreeSet;
use std::fmt::Write;

/// Rendering environment: names the emitter must know globally, e.g. the set
/// of user-defined function names (so a call to one renders as a direct
/// `(name …)` call, not an external `sash/run`).
struct Env {
    fns: BTreeSet<String>,
}

/// Render a program as the structural map (the `sash-rt.janet` preamble is
/// prepended by the caller).
pub fn emit_janet_map(program: &Program) -> String {
    let mut out = String::new();
    let mut vars: BTreeSet<String> = BTreeSet::new();
    collect_vars(&program.statements, &mut vars);
    let mut fns: BTreeSet<String> = BTreeSet::new();
    collect_functions(&program.statements, &mut fns);
    let env = Env { fns };
    for v in &vars {
        let _ = writeln!(out, "(var {} nil)", v);
    }
    if !vars.is_empty() {
        out.push('\n');
    }
    for stmt in &program.statements {
        let s = render_stmt(stmt, 0, &env);
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

/// Collect every user-defined function name (recursing into nested bodies).
fn collect_functions(stmts: &[Statement], acc: &mut BTreeSet<String>) {
    for s in stmts {
        if let StmtKind::Function(f) = &s.kind {
            acc.insert(sanitize(&f.name));
        }
        for body in s.child_bodies() {
            collect_functions(body, acc);
        }
    }
}

/// Collect every assigned variable name so they can be pre-declared. Stage 2
/// keeps a flat scope (matching the existing janet backend): declarations and
/// assignments anywhere — including inside function bodies — pre-declare a
/// top-level `(var … nil)`. `local`-shadowing is not yet modelled.
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
        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                acc.insert(sanitize(&a.name));
            }
        }
        StmtKind::If(i) => collect_if_vars(i, acc),
        StmtKind::While(w) => {
            collect_vars(&w.condition, acc);
            collect_vars(&w.body, acc);
        }
        StmtKind::For(f) => {
            acc.insert(sanitize(&f.variable));
            collect_vars(&f.body, acc);
        }
        StmtKind::Function(f) => collect_vars(&f.body, acc),
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

fn render_stmt(stmt: &Statement, depth: usize, env: &Env) -> String {
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
        StmtKind::Declaration(d) => render_declaration(d, depth),
        StmtKind::Function(f) => render_function(f, depth, env),
        StmtKind::Command(c) => format!("{}{}", ind, render_cmd(c, Ctx::Bare, env)),
        StmtKind::If(i) => render_if(i, depth, env),
        StmtKind::While(w) => render_while(w, depth, env),
        StmtKind::Suppressed(inner) => {
            // `- cmd` — suppressed: render the inner in unchecked position.
            format!(
                "{}(sash/dash {})",
                ind,
                render_stmt_inline(inner, Ctx::Cond, env)
            )
        }
        other => format!("{}(sash/TODO {})", ind, stmt_kind_name(other)),
    }
}

/// Human-readable name for an as-yet-unmapped statement kind, so a `sash/TODO`
/// placeholder names the construct (`stmt:arith-test`) rather than dumping an
/// opaque enum discriminant. Pure dispatcher.
fn stmt_kind_name(k: &StmtKind) -> String {
    let name = match k {
        StmtKind::Assignment(_) => "assignment",
        StmtKind::Declaration(_) => "declaration",
        StmtKind::Command(_) => "command",
        StmtKind::Pipeline(_) => "pipeline",
        StmtKind::If(_) => "if",
        StmtKind::While(_) => "while",
        StmtKind::For(_) => "for",
        StmtKind::Case(_) => "case",
        StmtKind::Function(_) => "function",
        StmtKind::Redirected(_) => "redirected",
        StmtKind::List(_) => "list",
        StmtKind::Group(_) => "group",
        StmtKind::Comment(_) => "comment",
        StmtKind::Suppressed(_) => "suppressed",
        StmtKind::ArithTest(_) => "arith-test",
        StmtKind::ReturnProducer(_) => "return-producer",
    };
    format!("stmt:{}", name)
}

/// Render a statement inline (no leading indent) in the given context.
fn render_stmt_inline(stmt: &Statement, ctx: Ctx, env: &Env) -> String {
    match &stmt.kind {
        StmtKind::Command(c) => render_cmd(c, ctx, env),
        StmtKind::Assignment(a) => format!("(set {} {})", sanitize(&a.name), render_expr(&a.value)),
        _ => format!("(sash/TODO {})", stmt_kind_name(&stmt.kind)),
    }
}

/// A typed/keyword declaration (`int n=5`, `local x=hi`). Each initialiser
/// becomes a `(set name value)`. The *type shows through the value*: an
/// int-typed literal renders as a bare janet number (`(set n 5)`), an untyped
/// string as a janet string (`(set x "hi")`) — no separate type annotation.
fn render_declaration(d: &Declaration, depth: usize) -> String {
    let ind = indent(depth);
    if d.assignments.is_empty() {
        // A bare `int x` / `local x` with no initialiser — nothing to bind.
        return format!("{}# (declare {})", ind, sanitize_flags(d));
    }
    let mut lines = Vec::new();
    for a in &d.assignments {
        let val = render_decl_value(d.var_type, &a.value);
        lines.push(format!("{}(set {} {})", ind, sanitize(&a.name), val));
    }
    lines.join("\n")
}

/// The value of a declaration initialiser: an int-typed literal → a bare
/// number; anything else → the ordinary expression rendering.
fn render_decl_value(ty: VarType, value: &Expr) -> String {
    if ty == VarType::Int {
        if let ExprKind::Literal(s) = &value.kind {
            if let Some(n) = parse_int_literal(s) {
                return n.to_string();
            }
        }
    }
    render_expr(value)
}

/// Parse a sash integer literal (DD#72): optional sign, `0x`/`0o`/`0b` prefix,
/// or decimal. Returns None for anything non-literal (e.g. an interpolation),
/// in which case the caller falls back to the expression rendering.
fn parse_int_literal(s: &str) -> Option<i64> {
    let t = s.trim();
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let mag = parse_unsigned_int(body)?;
    Some(if neg { -mag } else { mag })
}

fn parse_unsigned_int(body: &str) -> Option<i64> {
    let lower = body.to_ascii_lowercase();
    if let Some(h) = lower.strip_prefix("0x") {
        i64::from_str_radix(h, 16).ok()
    } else if let Some(o) = lower.strip_prefix("0o") {
        i64::from_str_radix(o, 8).ok()
    } else if let Some(b) = lower.strip_prefix("0b") {
        i64::from_str_radix(b, 2).ok()
    } else {
        body.parse::<i64>().ok()
    }
}

/// A short readable summary of a declaration's keyword/flags for the bare
/// (no-initialiser) comment form.
fn sanitize_flags(d: &Declaration) -> String {
    let kw = match d.keyword {
        DeclKeyword::Declare => "declare",
        DeclKeyword::Local => "local",
        DeclKeyword::Export => "export",
        DeclKeyword::Readonly => "readonly",
    };
    format!("{} {:?}", kw, d.var_type)
}

/// A user-function definition: `f() { … }` → `(sash/defn f body…)`. The body
/// statements are rendered in order (the macro supplies the frame + `return`
/// label). Positional params ($1/$@/$#) inside read the call's args.
fn render_function(f: &Function, depth: usize, env: &Env) -> String {
    let ind = indent(depth);
    let mut out = format!("{}(sash/defn {}\n", ind, sanitize(&f.name));
    for s in &f.body {
        out.push_str(&render_stmt(s, depth + 1, env));
        out.push('\n');
    }
    out.push_str(&format!("{})", ind));
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    /// Bare statement — checked: a nonzero external status aborts (DD#55 A).
    Bare,
    /// Condition (if/while) — unchecked: yields a bool, never aborts.
    Cond,
}

fn render_cmd(c: &Command, ctx: Ctx, env: &Env) -> String {
    let name = expr_literal(&c.name);
    match name.as_deref() {
        Some("echo") => format!("(sash/echo{})", render_args(&c.args)),
        Some(":") => "(sash/noop)".to_string(),
        Some("[[") => render_test_cmd(&c.args),
        Some("bless") => render_bless(&c.args, ctx),
        Some("return") => render_return(&c.args),
        Some(n) => render_named_cmd(n, &c.args, ctx, env),
        None => format!("(sash/TODO dynamic-command {})", render_expr(&c.name)),
    }
}

/// A named command that is either a user-function call or an external program.
/// A user-function call renders as a direct `(name …)`; in condition position
/// it is wrapped in `sash/succeeded` (run, then branch on $?). An external is
/// `sash/run` (bare, aborts) or `sash/ok?` (condition) — the two are kept
/// visibly distinct so the reader sees function-vs-program.
fn render_named_cmd(n: &str, args: &[Expr], ctx: Ctx, env: &Env) -> String {
    let san = sanitize(n);
    if env.fns.contains(&san) {
        render_user_call(&san, args, ctx)
    } else {
        render_external(n, args, ctx)
    }
}

/// A call to a user-defined function → direct `(name …)`; in condition position
/// wrapped in `sash/succeeded` (run, then branch on $?).
fn render_user_call(name: &str, args: &[Expr], ctx: Ctx) -> String {
    let call = format!("({}{})", name, render_args(args));
    if ctx == Ctx::Cond {
        format!("(sash/true? {})", call)
    } else {
        call
    }
}

/// An external program → `sash/run` (bare, aborts on nonzero) or `sash/ok?`
/// (condition, never aborts) — kept visibly distinct from a function call.
fn render_external(n: &str, args: &[Expr], ctx: Ctx) -> String {
    let verb = if ctx == Ctx::Cond {
        "sash/ok?"
    } else {
        "sash/run"
    };
    format!("({} {:?}{})", verb, n, render_args(args))
}

/// `return N` (a predicate/procedure return). The code renders as a bare number
/// when it is an integer literal, else as the ordinary expression (coerced to a
/// code by `sash/return`). Bare `return` re-returns the current `$?`.
fn render_return(args: &[Expr]) -> String {
    match args.first() {
        Some(a) => format!("(sash/return {})", render_code_arg(a)),
        None => "(sash/return ($ ?))".to_string(),
    }
}

fn render_code_arg(a: &Expr) -> String {
    if let ExprKind::Literal(s) = &a.kind {
        if let Some(n) = parse_int_literal(s) {
            return n.to_string();
        }
    }
    render_expr(a)
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
        s.push_str(&render_arg(a));
    }
    s
}

/// A single command argument. `$@`/`$*` splice the function's parameter array
/// in place (`;(sash/params)`); everything else renders as an ordinary value.
fn render_arg(a: &Expr) -> String {
    if let ExprKind::SimpleExpansion(name) = &a.kind {
        if name == "@" || name == "*" {
            return ";(sash/params)".to_string();
        }
    }
    render_expr(a)
}

// ── control flow ──────────────────────────────────────────────────────────

fn render_if(i: &If, depth: usize, env: &Env) -> String {
    let ind = indent(depth);
    let cond = render_cond(&i.condition, env);
    let mut out = format!("{}(sash/if {}\n", ind, cond);
    out.push_str(&render_body(&i.then_body, depth + 1, env));
    // elif → nested else-if; else.
    let mut else_src = String::new();
    render_else_chain(&i.elif_clauses, &i.else_body, depth + 1, &mut else_src, env);
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
    env: &Env,
) {
    let ind = indent(depth);
    if let Some((first, rest)) = elifs.split_first() {
        let cond = render_cond(&first.condition, env);
        let _ = writeln!(out, "{}(sash/if {}", ind, cond);
        out.push_str(&render_body(&first.body, depth + 1, env));
        let mut inner = String::new();
        render_else_chain(rest, else_body, depth + 1, &mut inner, env);
        if !inner.is_empty() {
            out.push('\n');
            out.push_str(&inner);
        }
        out.push_str(&format!("{})", ind));
    } else if let Some(eb) = else_body {
        out.push_str(&render_body(eb, depth, env));
    }
}

fn render_while(w: &While, depth: usize, env: &Env) -> String {
    let ind = indent(depth);
    let cond = render_cond(&w.condition, env);
    let head = if w.until { "sash/until" } else { "sash/while" };
    let mut out = format!("{}({} {}\n", ind, head, cond);
    out.push_str(&render_body(&w.body, depth + 1, env));
    out.push_str(&format!("{})", ind));
    out
}

/// Render an if/while condition (a statement list) to a boolean form. Stage 1
/// handles a single-statement condition; compound `a && b` conditions later.
fn render_cond(stmts: &[Statement], env: &Env) -> String {
    match stmts {
        [one] => render_stmt_inline(one, Ctx::Cond, env),
        [] => "true".to_string(),
        _ => "(sash/TODO compound-condition)".to_string(),
    }
}

/// Render a body as a `(do …)` block, one statement per line.
fn render_body(stmts: &[Statement], depth: usize, env: &Env) -> String {
    let ind = indent(depth);
    let mut out = format!("{}(do\n", ind);
    for s in stmts {
        out.push_str(&render_stmt(s, depth + 1, env));
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
    // sash `[[ ]]` is string/file tests only — numeric comparison lives in
    // `(( ))` (ArithTest), never here (the front-end rejects `-eq` etc. with
    // SB5019). So this maps string operators only, by design.
    Some(match op {
        "=" | "==" => "sash/=",
        "!=" => "sash/not=",
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
        other => format!("(sash/TODO {})", expr_kind_name(other)),
    }
}

/// Human-readable name for an as-yet-unmapped expression kind. Pure dispatcher.
fn expr_kind_name(k: &ExprKind) -> String {
    let name = match k {
        ExprKind::Literal(_) => "literal",
        ExprKind::DoubleQuoted(_) => "double-quoted",
        ExprKind::SingleQuoted(_) => "single-quoted",
        ExprKind::SimpleExpansion(_) => "simple-expansion",
        ExprKind::ParameterExpansion(_) => "parameter-expansion",
        ExprKind::Subscript { .. } => "subscript",
        ExprKind::CommandSubstitution(_) => "command-substitution",
        ExprKind::Arithmetic(_) => "arithmetic",
        ExprKind::Concat(_) => "concat",
        ExprKind::Array(_) => "array",
        ExprKind::AssocArray(_) => "assoc-array",
        ExprKind::Test(_) => "test",
    };
    format!("expr:{}", name)
}

fn render_var_read(name: &str) -> String {
    // $? → ($ ?); positionals → their sash/* reader; else a plain var read.
    if name == "?" {
        return "($ ?)".to_string();
    }
    if let Some(p) = positional_read(name) {
        return p;
    }
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
        format!("($ {})", sanitize(name))
    } else {
        format!("(sash/TODO expansion {:?})", name)
    }
}

/// Positional parameters — a function's OWN arguments (`$1` is sugar for
/// `$args[1]`), NEVER the process argv (which is `$(arg N)` — see
/// `render_argv_intrinsic`). Function-local: the front-end bars them at top
/// level (SB5024). `$#` → (sash/nparams); `$@`/`$*` → (sash/params); `$N` →
/// (sash/param N); `$0` (the name) is not yet mapped. None for an ordinary var.
fn positional_read(name: &str) -> Option<String> {
    Some(match name {
        "#" => "(sash/nparams)".to_string(),
        "@" | "*" => "(sash/params)".to_string(),
        "0" => "(sash/TODO arg0-name)".to_string(),
        n if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
            format!("(sash/param {})", n)
        }
        _ => return None,
    })
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
    // Stage 1/2: a single command → intrinsic form or piped external capture.
    let todo = || "(sash/TODO capture-nonexternal)".to_string();
    let [one] = stmts else { return todo() };
    let StmtKind::Command(c) = &one.kind else {
        return todo();
    };
    let Some(name) = expr_literal(&c.name) else {
        return todo();
    };
    // The process argv trio — `$(arg N)` / `$(argc)` / `$(argv)` — is the
    // script's command line (tainted), NOT a spawned program and NOT the
    // function parameters ($1/$@/$#). Map to the argv family, not sash/capture.
    if let Some(form) = render_argv_intrinsic(&name, &c.args) {
        return form;
    }
    if matches!(name.as_str(), "echo" | ":" | "[[") {
        return todo();
    }
    format!("(sash/capture {:?}{})", name, render_args(&c.args))
}

/// The argv intrinsics: `$(argc)` → (sash/argc); `$(argv)` → (sash/argv);
/// `$(arg N)` / `$(arg N DEFAULT)` → (sash/arg N [DEFAULT]) — index is numeric,
/// 0-based (index 0 is the program name). None for any other command.
fn render_argv_intrinsic(name: &str, args: &[Expr]) -> Option<String> {
    match name {
        "argc" => Some("(sash/argc)".to_string()),
        "argv" => Some("(sash/argv)".to_string()),
        "arg" => {
            let idx = args
                .first()
                .map(render_code_arg)
                .unwrap_or_else(|| "0".to_string());
            match args.get(1) {
                Some(d) => Some(format!("(sash/arg {} {})", idx, render_expr(d))),
                None => Some(format!("(sash/arg {})", idx)),
            }
        }
        _ => None,
    }
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
