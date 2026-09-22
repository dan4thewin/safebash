//! `--emit-ir` — structural capture of the IR as an **inert janet data
//! literal** (DD#104).
//!
//! This is not a backend. Nothing here decides semantics, and the output is
//! not a program: it is data. A consumer reads it with `(parse (slurp …))`
//! and walks it. Janet is homoiconic, so its data syntax *is* its code
//! syntax — the file needs no JSON parser and no preamble.
//!
//! Two properties are deliberate and load-bearing:
//!
//! 1. **Every match in this module is exhaustive — no wildcard arms.** Adding
//!    an IR variant is therefore a *compile error* here, not a silently
//!    omitted node. A hand-written emitter that quietly skipped a new variant
//!    would produce a plausible-but-wrong capture, which is this project's
//!    recurring bug class (an unresolved thing rendering as a plausible
//!    constant). The compiler forecloses it.
//!
//! 2. **Tags are IR node names, not an interpretation.** `attest` is a
//!    `Command` whose name is `"attest"`, so that is exactly how it renders;
//!    there is no dedicated `:attest` tag, just as there is no `:run` vs
//!    `:ok?` distinction (that is position-based *interpretation*, which
//!    belongs to a consumer, not to a capture). Keeping this layer free of
//!    judgement is what lets a consumer lag the compiler without the capture
//!    silently meaning something new.
//!
//! `:line` is carried on statements only — enough to read the capture beside
//! the source, without buring every expression in position noise.

use crate::ir::*;
use std::fmt::Write;

/// Bumped when the shape of the emitted data changes incompatibly, so a
/// consumer can refuse a capture it does not understand.
pub const IR_VERSION: u32 = 1;

/// Render a whole program as the inert janet data literal.
pub fn emit_ir(program: &Program) -> String {
    let mut out = String::new();
    out.push_str("# safebash IR capture — inert janet data (DD#104).\n");
    out.push_str("# Read with `(parse (slurp \"prog.ir\"))`; this is data, not a program.\n");
    let _ = writeln!(out, "{{:ir-version {}", IR_VERSION);
    out.push_str(" :statements ");
    stmt_list(&program.statements, 14, &mut out);
    out.push_str("\n :end-block ");
    match &program.end_block {
        Some(body) => stmt_list(body, 13, &mut out),
        None => out.push_str("nil"),
    }
    out.push_str("}\n");
    out
}

// ── formatting helpers ────────────────────────────────────────────────────

fn nl(ind: usize, out: &mut String) {
    out.push('\n');
    for _ in 0..ind {
        out.push(' ');
    }
}

/// A janet string literal. Escapes what janet's reader requires and renders
/// control bytes as `\xHH` so the capture stays single-line per string.
fn jstr(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match escape_of(c) {
            Some(esc) => out.push_str(esc),
            None if is_control(c) => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            None => out.push(c),
        }
    }
    out.push('"');
}

/// The characters janet's reader requires us to escape.
fn escape_of(c: char) -> Option<&'static str> {
    match c {
        '"' => Some("\\\""),
        '\\' => Some("\\\\"),
        '\n' => Some("\\n"),
        '\r' => Some("\\r"),
        '\t' => Some("\\t"),
        _ => None,
    }
}

fn is_control(c: char) -> bool {
    (c as u32) < 0x20 || (c as u32) == 0x7f
}

fn jbool(b: bool, out: &mut String) {
    out.push_str(if b { "true" } else { "false" });
}

/// `[a b c]`, one element per line aligned under the opening bracket.
fn stmt_list(list: &[Statement], ind: usize, out: &mut String) {
    if list.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (i, s) in list.iter().enumerate() {
        if i > 0 {
            nl(ind + 1, out);
        }
        stmt(s, ind + 1, out);
    }
    out.push(']');
}

fn expr_list(list: &[Expr], ind: usize, out: &mut String) {
    if list.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (i, e) in list.iter().enumerate() {
        if i > 0 {
            nl(ind + 1, out);
        }
        expr(e, ind + 1, out);
    }
    out.push(']');
}

// ── leaf enums ────────────────────────────────────────────────────────────

fn var_type(t: VarType) -> &'static str {
    match t {
        VarType::String => ":string",
        VarType::Int => ":int",
    }
}

fn assign_op(o: &AssignOp) -> &'static str {
    match o {
        AssignOp::Set => ":set",
        AssignOp::Append => ":append",
    }
}

fn decl_keyword(k: &DeclKeyword) -> &'static str {
    match k {
        DeclKeyword::Declare => ":declare",
        DeclKeyword::Export => ":export",
        DeclKeyword::Readonly => ":readonly",
    }
}

fn redirect_op(o: RedirectOp) -> &'static str {
    match o {
        RedirectOp::Write => ":write",
        RedirectOp::Append => ":append",
        RedirectOp::Read => ":read",
        RedirectOp::DupOutput => ":dup-output",
        RedirectOp::DupInput => ":dup-input",
        RedirectOp::ReadWrite => ":read-write",
    }
}

fn list_op(o: &ListOp) -> &'static str {
    match o {
        ListOp::And => ":and",
        ListOp::Or => ":or",
        ListOp::Background => ":background",
    }
}

fn arith_op(o: IrArithOp) -> &'static str {
    match o {
        IrArithOp::Add => ":add",
        IrArithOp::Sub => ":sub",
        IrArithOp::Mul => ":mul",
        IrArithOp::Div => ":div",
        IrArithOp::Mod => ":mod",
        IrArithOp::Pow => ":pow",
        IrArithOp::Eq => ":eq",
        IrArithOp::Ne => ":ne",
        IrArithOp::Lt => ":lt",
        IrArithOp::Le => ":le",
        IrArithOp::Gt => ":gt",
        IrArithOp::Ge => ":ge",
        IrArithOp::And => ":and",
        IrArithOp::Or => ":or",
        IrArithOp::Shl => ":shl",
        IrArithOp::Shr => ":shr",
        IrArithOp::BitAnd => ":bit-and",
        IrArithOp::BitOr => ":bit-or",
        IrArithOp::BitXor => ":bit-xor",
    }
}

fn unary_arith_op(o: IrUnaryArithOp) -> &'static str {
    match o {
        IrUnaryArithOp::Neg => ":neg",
        IrUnaryArithOp::Pos => ":pos",
        IrUnaryArithOp::Not => ":not",
        IrUnaryArithOp::BitNot => ":bit-not",
    }
}

/// `[x y z]` for any element type that has its own renderer. Empty renders
/// as `[]` by construction — no special case, no way to get it wrong.
fn seq_of<T>(items: &[T], ind: usize, out: &mut String, f: fn(&T, usize, &mut String)) {
    out.push('[');
    for (n, it) in items.iter().enumerate() {
        if n > 0 {
            nl(ind, out);
        }
        f(it, ind, out);
    }
    out.push(']');
}

fn opt_expr(e: Option<&Expr>, ind: usize, out: &mut String) {
    match e {
        Some(x) => expr(x, ind, out),
        None => out.push_str("nil"),
    }
}

fn opt_stmt_list(l: Option<&[Statement]>, ind: usize, out: &mut String) {
    match l {
        Some(x) => stmt_list(x, ind, out),
        None => out.push_str("nil"),
    }
}

// ── statements ────────────────────────────────────────────────────────────

/// Pure dispatch. Every arm hands off to a per-variant helper, so this stays
/// a flat table: adding an IR statement variant is a compile error right here.
fn stmt(s: &Statement, ind: usize, out: &mut String) {
    let (ln, i) = (s.span.line, ind + 1);
    match &s.kind {
        StmtKind::Assignment(a) => st_assignment(a, ln, i, out),
        StmtKind::Declaration(d) => st_declaration(d, ln, i, out),
        StmtKind::Command(c) => st_command(c, ln, i, out),
        StmtKind::Pipeline(p) => st_pipeline(p, ln, i, out),
        StmtKind::If(f) => st_if(f, ln, i, out),
        StmtKind::While(w) => st_while(w, ln, i, out),
        StmtKind::For(f) => st_for(f, ln, i, out),
        StmtKind::Case(c) => st_case(c, ln, i, out),
        StmtKind::Function(f) => st_function(f, ln, i, out),
        StmtKind::Redirected(r) => st_redirected(r, ln, i, out),
        StmtKind::List(l) => st_list(l, ln, i, out),
        StmtKind::Group(g) => st_group(g, ln, i, out),
        StmtKind::Suppressed(b) => st_wrapper(":suppressed", b, ln, i, out),
        StmtKind::Negated(b) => st_wrapper(":negated", b, ln, i, out),
        StmtKind::ReturnProducer(b) => st_wrapper(":return-producer", b, ln, i, out),
        StmtKind::ArithTest(a) => st_arith_test(a, ln, i, out),
    }
}

fn st_assignment(a: &Assignment, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :assignment :line {}", ln);
    nl(i, out);
    out.push_str(":assign ");
    assignment(a, i + 8, out);
    out.push('}');
}

fn st_declaration(d: &Declaration, ln: usize, i: usize, out: &mut String) {
    let _ = write!(
        out,
        "{{:kind :declaration :line {} :keyword {} :type {}",
        ln,
        decl_keyword(&d.keyword),
        var_type(d.var_type)
    );
    nl(i, out);
    out.push_str(":flags [");
    for (n, f) in d.flags.iter().enumerate() {
        if n > 0 {
            out.push(' ');
        }
        jstr(f, out);
    }
    out.push(']');
    nl(i, out);
    out.push_str(":assignments ");
    seq_of(&d.assignments, i + 14, out, decl_assignment);
    out.push('}');
}

fn st_command(c: &Command, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :command :line {}", ln);
    nl(i, out);
    out.push_str(":name ");
    expr(&c.name, i + 6, out);
    nl(i, out);
    out.push_str(":args ");
    expr_list(&c.args, i + 6, out);
    out.push('}');
}

fn st_pipeline(p: &Pipeline, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :pipeline :line {} :negated ", ln);
    jbool(p.negated, out);
    nl(i, out);
    out.push_str(":commands ");
    stmt_list(&p.commands, i + 10, out);
    out.push('}');
}

fn st_if(f: &If, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :if :line {}", ln);
    nl(i, out);
    out.push_str(":condition ");
    stmt_list(&f.condition, i + 11, out);
    nl(i, out);
    out.push_str(":then ");
    stmt_list(&f.then_body, i + 6, out);
    nl(i, out);
    out.push_str(":elifs ");
    seq_of(&f.elif_clauses, i + 8, out, elif_clause);
    nl(i, out);
    out.push_str(":else ");
    opt_stmt_list(f.else_body.as_deref(), i + 6, out);
    out.push('}');
}

fn elif_clause(e: &ElifClause, ind: usize, out: &mut String) {
    out.push_str("{:condition ");
    stmt_list(&e.condition, ind + 11, out);
    nl(ind + 1, out);
    out.push_str(":body ");
    stmt_list(&e.body, ind + 7, out);
    out.push('}');
}

fn st_while(w: &While, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :while :line {} :until ", ln);
    jbool(w.until, out);
    nl(i, out);
    out.push_str(":condition ");
    stmt_list(&w.condition, i + 11, out);
    nl(i, out);
    out.push_str(":body ");
    stmt_list(&w.body, i + 6, out);
    out.push('}');
}

fn st_for(f: &For, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :for :line {} :var ", ln);
    jstr(&f.variable, out);
    nl(i, out);
    out.push_str(":words ");
    expr_list(&f.words, i + 7, out);
    nl(i, out);
    out.push_str(":body ");
    stmt_list(&f.body, i + 6, out);
    out.push('}');
}

fn st_case(c: &Case, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :case :line {} :op ", ln);
    jstr(&c.op, out);
    nl(i, out);
    out.push_str(":value ");
    expr(&c.value, i + 7, out);
    nl(i, out);
    out.push_str(":items ");
    seq_of(&c.items, i + 8, out, case_item);
    out.push('}');
}

fn case_item(item: &CaseItem, ind: usize, out: &mut String) {
    out.push_str("{:default ");
    jbool(item.is_default, out);
    nl(ind + 1, out);
    out.push_str(":patterns ");
    expr_list(&item.patterns, ind + 11, out);
    nl(ind + 1, out);
    out.push_str(":body ");
    stmt_list(&item.body, ind + 7, out);
    out.push('}');
}

fn st_function(f: &Function, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :function :line {} :name ", ln);
    jstr(&f.name, out);
    let _ = write!(out, " :returns {}", var_type(f.return_type));
    nl(i, out);
    out.push_str(":body ");
    stmt_list(&f.body, i + 6, out);
    out.push('}');
}

fn st_redirected(r: &Redirected, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :redirected :line {}", ln);
    nl(i, out);
    out.push_str(":body ");
    stmt(&r.body, i + 6, out);
    nl(i, out);
    out.push_str(":redirects ");
    seq_of(&r.redirects, i + 12, out, redirect);
    out.push('}');
}

fn st_list(l: &List, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :list :line {} :op {}", ln, list_op(&l.op));
    nl(i, out);
    out.push_str(":left ");
    stmt(&l.left, i + 6, out);
    nl(i, out);
    out.push_str(":right ");
    stmt(&l.right, i + 7, out);
    out.push('}');
}

fn st_group(g: &Group, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :group :line {}", ln);
    nl(i, out);
    out.push_str(":body ");
    stmt_list(&g.body, i + 6, out);
    out.push('}');
}

/// `- cmd`, `! cmd` and `return <producer>` all wrap exactly one statement.
fn st_wrapper(tag: &str, inner: &Statement, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind {} :line {}", tag, ln);
    nl(i, out);
    out.push_str(":stmt ");
    stmt(inner, i + 6, out);
    out.push('}');
}

fn st_arith_test(a: &ArithExpr, ln: usize, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :arith-test :line {}", ln);
    nl(i, out);
    out.push_str(":expr ");
    arith(a, i + 6, out);
    out.push('}');
}

fn assignment(a: &Assignment, ind: usize, out: &mut String) {
    out.push_str("{:name ");
    jstr(&a.name, out);
    let _ = write!(out, " :op {}", assign_op(&a.op));
    nl(ind + 1, out);
    out.push_str(":index ");
    opt_expr(a.index.as_ref(), ind + 8, out);
    nl(ind + 1, out);
    out.push_str(":value ");
    expr(&a.value, ind + 8, out);
    out.push('}');
}

/// A declaration's `name` / `name=value`. No `:op` or `:index`: a declaration
/// creates a binding, so `+=` is rejected (SB4065) and an index never parses.
/// `:value nil` is a bare declaration (`int x`) — distinct from `:value ""`.
fn decl_assignment(a: &DeclAssignment, ind: usize, out: &mut String) {
    out.push_str("{:name ");
    jstr(&a.name, out);
    nl(ind + 1, out);
    out.push_str(":value ");
    opt_expr(a.value.as_ref(), ind + 8, out);
    out.push('}');
}

fn redirect(r: &Redirect, ind: usize, out: &mut String) {
    out.push_str("{:fd ");
    match r.fd {
        Some(fd) => {
            let _ = write!(out, "{}", fd);
        }
        None => out.push_str("nil"),
    }
    let _ = write!(out, " :op {}", redirect_op(r.op));
    nl(ind + 1, out);
    out.push_str(":target ");
    expr(&r.target, ind + 9, out);
    out.push('}');
}

// ── expressions ───────────────────────────────────────────────────────────

/// `ExprKind` has 12 variants — too few for the complexity gate's
/// pure-dispatcher exemption, too many for its 3×-mean bar. Splitting on the
/// real seam (a *word* is text or a list of text; a *composite* has inner
/// structure) keeps both halves flat. Note the second half lists the word
/// variants explicitly rather than using `_`, so a new variant is still a
/// compile error in BOTH functions, not a silent fallthrough.
fn expr(e: &Expr, ind: usize, out: &mut String) {
    let i = ind + 1;
    match &e.kind {
        ExprKind::Literal(s) => ex_word(":literal", ":value", s, out),
        ExprKind::SingleQuoted(s) => ex_word(":single-quoted", ":value", s, out),
        ExprKind::SimpleExpansion(n) => ex_word(":simple-expansion", ":name", n, out),
        ExprKind::DoubleQuoted(p) => ex_seq(":double-quoted", ":parts", p, i + 28, out),
        ExprKind::Concat(p) => ex_seq(":concat", ":parts", p, i + 22, out),
        ExprKind::Array(p) => ex_seq(":array", ":items", p, i + 21, out),
        ExprKind::AssocArray(_)
        | ExprKind::ParameterExpansion(_)
        | ExprKind::Subscript { .. }
        | ExprKind::CommandSubstitution(_)
        | ExprKind::SlurpFile(_)
        | ExprKind::Arithmetic(_)
        | ExprKind::Test(_) => expr_composite(&e.kind, i, out),
    }
}

fn expr_composite(k: &ExprKind, i: usize, out: &mut String) {
    match k {
        ExprKind::AssocArray(pairs) => ex_assoc(pairs, i, out),
        ExprKind::ParameterExpansion(p) => ex_param(p, i, out),
        ExprKind::Subscript { name, index } => ex_subscript(name, index, i, out),
        ExprKind::CommandSubstitution(body) => ex_cmdsub(body, i, out),
        ExprKind::SlurpFile(path) => ex_slurp(path, i, out),
        ExprKind::Arithmetic(a) => ex_arithmetic(a, i, out),
        ExprKind::Test(t) => ex_test(t, i, out),
        ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_)
        | ExprKind::SimpleExpansion(_)
        | ExprKind::DoubleQuoted(_)
        | ExprKind::Concat(_)
        | ExprKind::Array(_) => unreachable!("word forms are handled by expr()"),
    }
}

fn ex_word(tag: &str, key: &str, s: &str, out: &mut String) {
    let _ = write!(out, "{{:kind {} {} ", tag, key);
    jstr(s, out);
    out.push('}');
}

fn ex_seq(tag: &str, key: &str, parts: &[Expr], ind: usize, out: &mut String) {
    let _ = write!(out, "{{:kind {} {} ", tag, key);
    expr_list(parts, ind, out);
    out.push('}');
}

fn assoc_pair(p: &(String, Expr), ind: usize, out: &mut String) {
    out.push_str("{:key ");
    jstr(&p.0, out);
    out.push_str(" :value ");
    expr(&p.1, ind + 8, out);
    out.push('}');
}

fn ex_assoc(pairs: &[(String, Expr)], i: usize, out: &mut String) {
    out.push_str("{:kind :assoc-array :pairs ");
    seq_of(pairs, i + 28, out, assoc_pair);
    out.push('}');
}

fn ex_param(p: &ParameterExpansion, i: usize, out: &mut String) {
    out.push_str("{:kind :parameter-expansion :name ");
    jstr(&p.name, out);
    out.push_str(" :operator ");
    match &p.operator {
        Some(op) => jstr(op, out),
        None => out.push_str("nil"),
    }
    out.push_str(" :taint-off ");
    jbool(p.taint_off, out);
    nl(i, out);
    out.push_str(":index ");
    opt_expr(p.index.as_deref(), i + 7, out);
    nl(i, out);
    out.push_str(":operand ");
    opt_expr(p.operand.as_deref(), i + 9, out);
    out.push('}');
}

fn ex_subscript(name: &str, index: &Expr, i: usize, out: &mut String) {
    out.push_str("{:kind :subscript :name ");
    jstr(name, out);
    nl(i, out);
    out.push_str(":index ");
    expr(index, i + 7, out);
    out.push('}');
}

fn ex_cmdsub(body: &[Statement], i: usize, out: &mut String) {
    out.push_str("{:kind :command-substitution :body ");
    stmt_list(body, i + 34, out);
    out.push('}');
}

fn ex_slurp(path: &Expr, i: usize, out: &mut String) {
    out.push_str("{:kind :slurp-file :path ");
    expr(path, i + 25, out);
    out.push('}');
}

fn ex_arithmetic(a: &ArithExpr, i: usize, out: &mut String) {
    out.push_str("{:kind :arithmetic :expr ");
    arith(a, i + 25, out);
    out.push('}');
}

fn ex_test(t: &TestExpr, i: usize, out: &mut String) {
    out.push_str("{:kind :test :expr ");
    test(t, i + 19, out);
    out.push('}');
}

fn test(t: &TestExpr, ind: usize, out: &mut String) {
    let i = ind + 1;
    match t {
        TestExpr::Unary { op, operand } => {
            out.push_str("{:kind :unary :op ");
            jstr(op, out);
            nl(i, out);
            out.push_str(":operand ");
            expr(operand, i + 9, out);
            out.push('}');
        }
        TestExpr::Binary { left, op, right } => {
            out.push_str("{:kind :binary :op ");
            jstr(op, out);
            nl(i, out);
            out.push_str(":left ");
            expr(left, i + 6, out);
            nl(i, out);
            out.push_str(":right ");
            expr(right, i + 7, out);
            out.push('}');
        }
        TestExpr::Not(inner) => {
            out.push_str("{:kind :not :expr ");
            test(inner, i + 18, out);
            out.push('}');
        }
        TestExpr::And(l, r) => test_pair(":and", l, r, i, out),
        TestExpr::Or(l, r) => test_pair(":or", l, r, i, out),
    }
}

fn test_pair(tag: &str, l: &TestExpr, r: &TestExpr, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind {}", tag);
    nl(i, out);
    out.push_str(":left ");
    test(l, i + 6, out);
    nl(i, out);
    out.push_str(":right ");
    test(r, i + 7, out);
    out.push('}');
}

// ── arithmetic ────────────────────────────────────────────────────────────

/// Pure dispatch, same contract as [`stmt`].
fn arith(a: &ArithExpr, ind: usize, out: &mut String) {
    let i = ind + 1;
    match a {
        ArithExpr::Literal(n) => {
            let _ = write!(out, "{{:kind :arith-literal :value {}}}", n);
        }
        ArithExpr::Var(n) => ex_word(":arith-var", ":name", n, out),
        ArithExpr::DollarVar(n) => ex_word(":arith-dollar-var", ":name", n, out),
        ArithExpr::BinOp { left, op, right } => ar_binop(*op, left, right, i, out),
        ArithExpr::UnaryOp { op, operand } => ar_unop(*op, operand, i, out),
        ArithExpr::PostIncDec { .. } => {
            unreachable!("{}", crate::ir::ARITH_IR_MUTATION_INVARIANT)
        }
        ArithExpr::PreIncDec { .. } => unreachable!("{}", crate::ir::PREFIX_INCDEC_INVARIANT),
        ArithExpr::Assign { var, value } => ar_assign(var, None, value, i, out),
        ArithExpr::CompoundAssign { var, op, value } => ar_assign(var, Some(*op), value, i, out),
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => ar_ternary(cond, true_val, false_val, i, out),
        ArithExpr::Comma(items) => ar_comma(items, i, out),
        ArithExpr::FuncCall { name, args } => ar_funcall(name, args, i, out),
        ArithExpr::Paren(inner) => ar_paren(inner, i, out),
        ArithExpr::Expansion(e) => ar_expansion(e, i, out),
        ArithExpr::Subscript { name, index } => ar_subscript(name, index, i, out),
        ArithExpr::SubscriptAssign { .. } => {
            unreachable!("{}", crate::ir::ARITH_IR_MUTATION_INVARIANT)
        }
    }
}

fn ar_binop(op: IrArithOp, left: &ArithExpr, right: &ArithExpr, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :arith-binop :op {}", arith_op(op));
    nl(i, out);
    out.push_str(":left ");
    arith(left, i + 6, out);
    nl(i, out);
    out.push_str(":right ");
    arith(right, i + 7, out);
    out.push('}');
}

fn ar_unop(op: IrUnaryArithOp, operand: &ArithExpr, i: usize, out: &mut String) {
    let _ = write!(out, "{{:kind :arith-unop :op {}", unary_arith_op(op));
    nl(i, out);
    out.push_str(":operand ");
    arith(operand, i + 9, out);
    out.push('}');
}

fn ar_assign(var: &str, op: Option<IrArithOp>, value: &ArithExpr, i: usize, out: &mut String) {
    match op {
        Some(o) => {
            out.push_str("{:kind :arith-compound-assign :var ");
            jstr(var, out);
            let _ = write!(out, " :op {}", arith_op(o));
        }
        None => {
            out.push_str("{:kind :arith-assign :var ");
            jstr(var, out);
        }
    }
    nl(i, out);
    out.push_str(":value ");
    arith(value, i + 7, out);
    out.push('}');
}

fn ar_ternary(c: &ArithExpr, t: &ArithExpr, f: &ArithExpr, i: usize, out: &mut String) {
    out.push_str("{:kind :arith-ternary");
    for (key, e) in [(":cond ", c), (":then ", t), (":else ", f)] {
        nl(i, out);
        out.push_str(key);
        arith(e, i + 6, out);
    }
    out.push('}');
}

fn ar_comma(items: &[ArithExpr], i: usize, out: &mut String) {
    out.push_str("{:kind :arith-comma :items ");
    seq_of(items, i + 28, out, arith);
    out.push('}');
}

fn ar_funcall(name: &str, args: &[ArithExpr], i: usize, out: &mut String) {
    out.push_str("{:kind :arith-funcall :name ");
    jstr(name, out);
    nl(i, out);
    out.push_str(":args ");
    seq_of(args, i + 7, out, arith);
    out.push('}');
}

fn ar_paren(inner: &ArithExpr, i: usize, out: &mut String) {
    out.push_str("{:kind :arith-paren :expr ");
    arith(inner, i + 25, out);
    out.push('}');
}

fn ar_expansion(e: &Expr, i: usize, out: &mut String) {
    out.push_str("{:kind :arith-expansion :expr ");
    expr(e, i + 29, out);
    out.push('}');
}

fn ar_subscript(name: &str, index: &ArithExpr, i: usize, out: &mut String) {
    out.push_str("{:kind :arith-subscript :name ");
    jstr(name, out);
    nl(i, out);
    out.push_str(":index ");
    arith(index, i + 7, out);
    out.push('}');
}
