use crate::ir::*;
/// IR pretty-printer.
///
/// Renders the safebash IR as indented text for visual verification
/// of CST→IR lowering.  The output is designed to be:
///   - Deterministic (same IR always produces same text)
///   - Readable (clear nesting, minimal noise)
///   - Diffable (one concept per line where practical)
use std::fmt::Write;

/// Pretty-print a Program to a string.
pub fn print_program(program: &Program) -> String {
    let mut out = String::new();
    for stmt in &program.statements {
        print_stmt(&mut out, stmt, 0);
    }
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn print_stmt(out: &mut String, stmt: &Statement, depth: usize) {
    match &stmt.kind {
        StmtKind::Comment(text) => {
            indent(out, depth);
            writeln!(out, "{}", text).unwrap();
        }
        StmtKind::Assignment(a) => {
            indent(out, depth);
            write!(out, "{}=", a.name).unwrap();
            print_expr(out, &a.value);
            writeln!(out).unwrap();
        }
        StmtKind::Declaration(d) => {
            indent(out, depth);
            write!(out, "{}", decl_keyword(&d.keyword)).unwrap();
            for flag in &d.flags {
                write!(out, " {}", flag).unwrap();
            }
            for a in &d.assignments {
                write!(out, " {}=", a.name).unwrap();
                print_expr(out, &a.value);
            }
            writeln!(out).unwrap();
        }
        StmtKind::Command(c) => {
            indent(out, depth);
            print_expr(out, &c.name);
            for arg in &c.args {
                out.push(' ');
                print_expr(out, arg);
            }
            writeln!(out).unwrap();
        }
        StmtKind::Pipeline(p) => {
            if p.negated {
                indent(out, depth);
                writeln!(out, "!").unwrap();
            }
            for (i, cmd) in p.commands.iter().enumerate() {
                if i > 0 {
                    indent(out, depth);
                    writeln!(out, "|").unwrap();
                }
                print_stmt(out, cmd, depth);
            }
        }
        StmtKind::List(l) => {
            print_stmt(out, &l.left, depth);
            indent(out, depth);
            writeln!(out, "{}", list_op(&l.op)).unwrap();
            print_stmt(out, &l.right, depth);
        }
        StmtKind::If(i) => {
            indent(out, depth);
            writeln!(out, "if").unwrap();
            for cond in &i.condition {
                print_stmt(out, cond, depth + 1);
            }
            indent(out, depth);
            writeln!(out, "then").unwrap();
            for s in &i.then_body {
                print_stmt(out, s, depth + 1);
            }
            for elif in &i.elif_clauses {
                indent(out, depth);
                writeln!(out, "elif").unwrap();
                for cond in &elif.condition {
                    print_stmt(out, cond, depth + 1);
                }
                indent(out, depth);
                writeln!(out, "then").unwrap();
                for s in &elif.body {
                    print_stmt(out, s, depth + 1);
                }
            }
            if let Some(else_body) = &i.else_body {
                indent(out, depth);
                writeln!(out, "else").unwrap();
                for s in else_body {
                    print_stmt(out, s, depth + 1);
                }
            }
            indent(out, depth);
            writeln!(out, "fi").unwrap();
        }
        StmtKind::While(w) => {
            indent(out, depth);
            writeln!(out, "{}", if w.until { "until" } else { "while" }).unwrap();
            for cond in &w.condition {
                print_stmt(out, cond, depth + 1);
            }
            indent(out, depth);
            writeln!(out, "do").unwrap();
            for s in &w.body {
                print_stmt(out, s, depth + 1);
            }
            indent(out, depth);
            writeln!(out, "done").unwrap();
        }
        StmtKind::For(f) => {
            indent(out, depth);
            write!(out, "for {} in", f.variable).unwrap();
            for w in &f.words {
                out.push(' ');
                print_expr(out, w);
            }
            writeln!(out).unwrap();
            indent(out, depth);
            writeln!(out, "do").unwrap();
            for s in &f.body {
                print_stmt(out, s, depth + 1);
            }
            indent(out, depth);
            writeln!(out, "done").unwrap();
        }
        StmtKind::Case(c) => {
            indent(out, depth);
            write!(out, "case ").unwrap();
            print_expr(out, &c.value);
            writeln!(out, " in").unwrap();
            for item in &c.items {
                indent(out, depth + 1);
                for (i, pat) in item.patterns.iter().enumerate() {
                    if i > 0 {
                        out.push_str(" | ");
                    }
                    print_expr(out, pat);
                }
                writeln!(out, ")").unwrap();
                for s in &item.body {
                    print_stmt(out, s, depth + 2);
                }
                indent(out, depth + 1);
                writeln!(out, "{}", if item.fallthrough { ";&" } else { ";;" }).unwrap();
            }
            indent(out, depth);
            writeln!(out, "esac").unwrap();
        }
        StmtKind::Function(f) => {
            indent(out, depth);
            writeln!(out, "{}()", f.name).unwrap();
            for s in &f.body {
                print_stmt(out, s, depth + 1);
            }
        }
        StmtKind::Redirected(r) => {
            // Print the body statement without a newline, then redirections
            print_stmt_inline(out, &r.body, depth);
            for redir in &r.redirects {
                out.push(' ');
                if let Some(fd) = redir.fd {
                    write!(out, "{}", fd).unwrap();
                }
                write!(out, "{} ", redirect_op(&redir.op)).unwrap();
                print_expr(out, &redir.target);
            }
            writeln!(out).unwrap();
        }
        StmtKind::Subshell(s) => {
            indent(out, depth);
            writeln!(out, "(").unwrap();
            for stmt in &s.body {
                print_stmt(out, stmt, depth + 1);
            }
            indent(out, depth);
            writeln!(out, ")").unwrap();
        }
        StmtKind::Group(g) => {
            indent(out, depth);
            writeln!(out, "{{").unwrap();
            for stmt in &g.body {
                print_stmt(out, stmt, depth + 1);
            }
            indent(out, depth);
            writeln!(out, "}}").unwrap();
        }
        StmtKind::Suppressed(inner) => {
            indent(out, depth);
            write!(out, "- ").unwrap();
            print_stmt(out, inner, depth);
        }
        StmtKind::ArithTest(expr) => {
            indent(out, depth);
            write!(out, "(( ").unwrap();
            print_arith_expr(out, expr);
            writeln!(out, " ))").unwrap();
        }
    }
}

/// Print a statement on a single line (no trailing newline).
/// Used for the body of redirected statements.
fn print_stmt_inline(out: &mut String, stmt: &Statement, depth: usize) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            indent(out, depth);
            print_expr(out, &c.name);
            for arg in &c.args {
                out.push(' ');
                print_expr(out, arg);
            }
        }
        // For non-command redirected bodies, fall back to normal printing
        // and trim the trailing newline
        _ => {
            let mut tmp = String::new();
            print_stmt(&mut tmp, stmt, depth);
            out.push_str(tmp.trim_end_matches('\n'));
        }
    }
}

fn print_expr(out: &mut String, expr: &Expr) {
    match &expr.kind {
        ExprKind::Literal(s) => {
            out.push_str(s);
        }
        ExprKind::DoubleQuoted(parts) => {
            out.push('"');
            for part in parts {
                match &part.kind {
                    ExprKind::Literal(s) => out.push_str(s),
                    ExprKind::SimpleExpansion(var) => {
                        write!(out, "${}", var).unwrap();
                    }
                    ExprKind::ParameterExpansion(pe) => {
                        print_param_expansion(out, pe);
                    }
                    ExprKind::CommandSubstitution(stmts) => {
                        out.push_str("$(");
                        print_inline_stmts(out, stmts);
                        out.push(')');
                    }
                    _ => print_expr(out, part),
                }
            }
            out.push('"');
        }
        ExprKind::SingleQuoted(s) => {
            write!(out, "'{}'", s).unwrap();
        }
        ExprKind::SimpleExpansion(var) => {
            write!(out, "${}", var).unwrap();
        }
        ExprKind::ParameterExpansion(pe) => {
            print_param_expansion(out, pe);
        }
        ExprKind::CommandSubstitution(stmts) => {
            out.push_str("$(");
            print_inline_stmts(out, stmts);
            out.push(')');
        }
        ExprKind::Arithmetic(a) => {
            out.push_str("$((");
            print_arith_expr(out, a);
            out.push_str("))");
        }
        ExprKind::Concat(parts) => {
            for part in parts {
                print_expr(out, part);
            }
        }
        ExprKind::Array(elems) => {
            out.push('(');
            for (i, elem) in elems.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                print_expr(out, elem);
            }
            out.push(')');
        }
        ExprKind::AssocArray(pairs) => {
            out.push('(');
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push('[');
                out.push_str(k);
                out.push_str("]=");
                print_expr(out, v);
            }
            out.push(')');
        }
        ExprKind::Subscript { name, index } => {
            out.push_str("${");
            out.push_str(name);
            out.push('[');
            print_expr(out, index);
            out.push_str("]}");
        }
        ExprKind::Test(test) => {
            out.push_str("[[ ");
            print_test_expr(out, test);
            out.push_str(" ]]");
        }
    }
}

fn print_param_expansion(out: &mut String, pe: &ParameterExpansion) {
    out.push_str("${");
    if pe.operator.as_deref() == Some("strlen") {
        out.push('#');
        out.push_str(&pe.name);
    } else {
        out.push_str(&pe.name);
        if let Some(op) = &pe.operator {
            out.push_str(op);
            if let Some(operand) = &pe.operand {
                print_expr(out, operand);
            }
        }
    }
    out.push('}');
}

fn print_test_expr(out: &mut String, test: &TestExpr) {
    match test {
        TestExpr::Unary { op, operand } => {
            write!(out, "{} ", op).unwrap();
            print_expr(out, operand);
        }
        TestExpr::Binary { left, op, right } => {
            print_expr(out, left);
            write!(out, " {} ", op).unwrap();
            print_expr(out, right);
        }
        TestExpr::Not(inner) => {
            out.push_str("! ");
            print_test_expr(out, inner);
        }
        TestExpr::And(l, r) => {
            print_test_expr(out, l);
            out.push_str(" && ");
            print_test_expr(out, r);
        }
        TestExpr::Or(l, r) => {
            print_test_expr(out, l);
            out.push_str(" || ");
            print_test_expr(out, r);
        }
        TestExpr::Paren(inner) => {
            out.push_str("( ");
            print_test_expr(out, inner);
            out.push_str(" )");
        }
    }
}

/// Print statements inline, separated by semicolons (for command substitution).
fn print_inline_stmts(out: &mut String, stmts: &[Statement]) {
    for (i, stmt) in stmts.iter().enumerate() {
        if i > 0 {
            out.push_str("; ");
        }
        match &stmt.kind {
            StmtKind::Command(c) => {
                print_expr(out, &c.name);
                for arg in &c.args {
                    out.push(' ');
                    print_expr(out, arg);
                }
            }
            StmtKind::Pipeline(p) => {
                for (j, cmd) in p.commands.iter().enumerate() {
                    if j > 0 {
                        out.push_str(" | ");
                    }
                    if let StmtKind::Command(c) = &cmd.kind {
                        print_expr(out, &c.name);
                        for arg in &c.args {
                            out.push(' ');
                            print_expr(out, arg);
                        }
                    }
                }
            }
            _ => {
                let mut tmp = String::new();
                print_stmt(&mut tmp, stmt, 0);
                out.push_str(tmp.trim());
            }
        }
    }
}

fn decl_keyword(kw: &DeclKeyword) -> &'static str {
    match kw {
        DeclKeyword::Declare => "declare",
        DeclKeyword::Local => "local",
        DeclKeyword::Export => "export",
        DeclKeyword::Readonly => "readonly",
    }
}

fn list_op(op: &ListOp) -> &'static str {
    match op {
        ListOp::And => "&&",
        ListOp::Or => "||",
        ListOp::Background => "&",
    }
}

fn redirect_op(op: &RedirectOp) -> &'static str {
    match op {
        RedirectOp::Write => ">",
        RedirectOp::Append => ">>",
        RedirectOp::Read => "<",
        RedirectOp::DupOutput => ">&",
        RedirectOp::DupInput => "<&",
        RedirectOp::ReadWrite => "<>",
        RedirectOp::HereDoc => "<<",
        RedirectOp::HereString => "<<<",
    }
}

fn print_arith_expr(out: &mut String, expr: &crate::ir::ArithExpr) {
    use crate::ir::{ArithExpr, IrArithOp, IrUnaryArithOp};
    match expr {
        ArithExpr::Literal(n) => write!(out, "{}", n).unwrap(),
        ArithExpr::Var(name) => out.push_str(name),
        ArithExpr::DollarVar(name) => write!(out, "${}", name).unwrap(),
        ArithExpr::BinOp { left, op, right } => {
            print_arith_expr(out, left);
            let op_str = match op {
                IrArithOp::Add => " + ", IrArithOp::Sub => " - ",
                IrArithOp::Mul => " * ", IrArithOp::Div => " / ",
                IrArithOp::Mod => " % ", IrArithOp::Pow => " ** ",
                IrArithOp::Eq => " == ", IrArithOp::Ne => " != ",
                IrArithOp::Lt => " < ", IrArithOp::Le => " <= ",
                IrArithOp::Gt => " > ", IrArithOp::Ge => " >= ",
                IrArithOp::And => " && ", IrArithOp::Or => " || ",
                IrArithOp::Shl => " << ", IrArithOp::Shr => " >> ",
                IrArithOp::BitAnd => " & ", IrArithOp::BitOr => " | ",
                IrArithOp::BitXor => " ^ ",
            };
            out.push_str(op_str);
            print_arith_expr(out, right);
        }
        ArithExpr::UnaryOp { op, operand } => {
            let op_str = match op {
                IrUnaryArithOp::Neg => "-",
                IrUnaryArithOp::Pos => "+",
                IrUnaryArithOp::Not => "!",
                IrUnaryArithOp::BitNot => "~",
            };
            out.push_str(op_str);
            print_arith_expr(out, operand);
        }
        ArithExpr::PreIncDec { var, inc } => {
            out.push_str(if *inc { "++" } else { "--" });
            out.push_str(var);
        }
        ArithExpr::PostIncDec { var, inc } => {
            out.push_str(var);
            out.push_str(if *inc { "++" } else { "--" });
        }
        ArithExpr::Assign { var, value } => {
            out.push_str(var);
            out.push_str(" = ");
            print_arith_expr(out, value);
        }
        ArithExpr::CompoundAssign { var, op, value } => {
            out.push_str(var);
            let op_str = match op {
                IrArithOp::Add => " += ", IrArithOp::Sub => " -= ",
                IrArithOp::Mul => " *= ", IrArithOp::Div => " /= ",
                IrArithOp::Mod => " %= ", _ => " ?= ",
            };
            out.push_str(op_str);
            print_arith_expr(out, value);
        }
        ArithExpr::Ternary { cond, true_val, false_val } => {
            print_arith_expr(out, cond);
            out.push_str(" ? ");
            print_arith_expr(out, true_val);
            out.push_str(" : ");
            print_arith_expr(out, false_val);
        }
        ArithExpr::Comma(exprs) => {
            for (i, e) in exprs.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                print_arith_expr(out, e);
            }
        }
        ArithExpr::FuncCall { name, args } => {
            out.push('(');
            out.push_str(name);
            for a in args {
                out.push(' ');
                print_arith_expr(out, a);
            }
            out.push(')');
        }
        ArithExpr::Paren(inner) => {
            out.push('(');
            print_arith_expr(out, inner);
            out.push(')');
        }
    }
}
