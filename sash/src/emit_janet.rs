/// Janet code emitter.
///
/// Translates safebash IR into Janet source code.  Janet serves as
/// the correctness oracle: if the Janet program produces the same
/// output as the original Bash script, the IR transformation is
/// semantically correct.
///
/// Mapping:
///   Bash                    Janet
///   ─────────────────────   ─────────────────────────────
///   x=value                 (var x "value") / (set x "value")
///   echo $x                 (print x)
///   cmd arg1 arg2           (os/execute ["cmd" "arg1" "arg2"] :p)
///   cmd1 | cmd2             os/spawn pipe chain
///   if [[ cond ]]; then     (if cond body else)
///   while cond; do          (while cond body)
///   for x in a b; do        (each x ["a" "b"] body)
///   case $x in              (match x pattern body ...)
///   func() { body }         (defn func [] body)
///   $var                    x  (Janet variable reference)
///   "hello $var"            (string "hello " var)
use crate::ir::*;
use crate::ssa::BuiltinCmd;
use std::fmt::Write;

/// Emit a complete Janet program from an IR Program.
pub fn emit_janet(program: &Program) -> String {
    let mut out = String::new();
    let mut ctx = EmitCtx::new();

    // Scan for positional param usage ($1, $#, $@, shift)
    if needs_args_preamble(&program.statements) {
        writeln!(out, "(var _args (array/slice (dyn :args) 1))").unwrap();
        ctx.declare("_args");
    }

    ctx.declare("__last_exit");
    writeln!(out, "(var __last_exit 0)").unwrap();
    for stmt in &program.statements {
        emit_stmt(&mut out, &mut ctx, stmt, 0);
    }
    out
}

/// Check if any statement references positional params or shift.
fn needs_args_preamble(stmts: &[Statement]) -> bool {
    for stmt in stmts {
        if _stmt_uses_args(stmt) {
            return true;
        }
    }
    false
}

fn _stmt_uses_args(stmt: &Statement) -> bool {
    match &stmt.kind {
        StmtKind::Command(c) => {
            let name = expr_to_literal(&c.name);
            if name == "shift" {
                return true;
            }
            for arg in &c.args {
                if _expr_uses_args(arg) {
                    return true;
                }
            }
            _expr_uses_args(&c.name)
        }
        StmtKind::Assignment(a) => _expr_uses_args(&a.value),
        StmtKind::Declaration(d) => d.assignments.iter().any(|a| _expr_uses_args(&a.value)),
        StmtKind::If(i) => {
            i.condition.iter().any(_stmt_uses_args)
                || needs_args_preamble(&i.then_body)
                || i.else_body.as_ref().is_some_and(|e| needs_args_preamble(e))
        }
        StmtKind::While(w) => {
            w.condition.iter().any(_stmt_uses_args) || needs_args_preamble(&w.body)
        }
        StmtKind::For(f) => f.words.iter().any(_expr_uses_args) || needs_args_preamble(&f.body),
        StmtKind::Redirected(r) => _stmt_uses_args(&r.body),
        StmtKind::List(l) => _stmt_uses_args(&l.left) || _stmt_uses_args(&l.right),
        _ => false,
    }
}

fn _expr_uses_args(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::SimpleExpansion(v) => {
            v == "#" || v == "@" || v == "0" || v.parse::<u32>().is_ok()
        }
        ExprKind::DoubleQuoted(parts) => parts.iter().any(_expr_uses_args),
        ExprKind::Concat(parts) => parts.iter().any(_expr_uses_args),
        ExprKind::ParameterExpansion(pe) => {
            pe.name == "#" || pe.name == "@" || pe.name.parse::<u32>().is_ok()
        }
        ExprKind::Test(test) => _test_uses_args(test),
        _ => false,
    }
}

fn _test_uses_args(test: &TestExpr) -> bool {
    match test {
        TestExpr::Binary { left, right, .. } => _expr_uses_args(left) || _expr_uses_args(right),
        TestExpr::Unary { operand, .. } => _expr_uses_args(operand),
        TestExpr::Not(inner) | TestExpr::Paren(inner) => _test_uses_args(inner),
        TestExpr::And(a, b) | TestExpr::Or(a, b) => _test_uses_args(a) || _test_uses_args(b),
    }
}

/// Tracks which variables have been declared (var vs set) and their types.
struct EmitCtx {
    declared: std::collections::HashSet<String>,
    int_vars: std::collections::HashSet<String>,
    int_funcs: std::collections::HashSet<String>,
    functions: std::collections::HashSet<String>,
}

impl EmitCtx {
    fn new() -> Self {
        Self {
            declared: std::collections::HashSet::new(),
            int_vars: std::collections::HashSet::new(),
            int_funcs: std::collections::HashSet::new(),
            functions: std::collections::HashSet::new(),
        }
    }

    fn is_declared(&self, name: &str) -> bool {
        self.declared.contains(name)
    }

    fn is_int(&self, name: &str) -> bool {
        self.int_vars.contains(name)
    }

    #[allow(dead_code)]
    fn is_int_func(&self, name: &str) -> bool {
        self.int_funcs.contains(name)
    }

    fn declare(&mut self, name: &str) {
        self.declared.insert(name.to_string());
    }

    fn declare_int(&mut self, name: &str) {
        self.declared.insert(name.to_string());
        self.int_vars.insert(name.to_string());
    }

    fn define_function(&mut self, name: &str) {
        self.functions.insert(name.to_string());
    }

    fn is_function(&self, name: &str) -> bool {
        self.functions.contains(name)
    }
}

fn _is_function_name(_name: &str) -> bool {
    false // placeholder — ctx.is_function is the real check
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn emit_stmt(out: &mut String, ctx: &mut EmitCtx, stmt: &Statement, depth: usize) {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            indent(out, depth);
            let val = emit_expr_inline(&a.value, ctx);
            if ctx.is_declared(&a.name) {
                writeln!(out, "(set {} {})", a.name, val).unwrap();
            } else {
                ctx.declare(&a.name);
                writeln!(out, "(var {} {})", a.name, val).unwrap();
            }
        }

        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                indent(out, depth);
                let val = if d.var_type == VarType::Int {
                    // Int declarations emit bare numbers
                    emit_expr_as_int(&a.value, ctx)
                } else {
                    emit_expr_inline(&a.value, ctx)
                };
                if ctx.is_declared(&a.name) {
                    writeln!(out, "(set {} {})", a.name, val).unwrap();
                } else {
                    if d.var_type == VarType::Int {
                        ctx.declare_int(&a.name);
                    } else {
                        ctx.declare(&a.name);
                    }
                    if d.keyword == DeclKeyword::Readonly {
                        writeln!(out, "(def {} {})", a.name, val).unwrap();
                    } else {
                        writeln!(out, "(var {} {})", a.name, val).unwrap();
                    }
                }
                if d.keyword == DeclKeyword::Export {
                    indent(out, depth);
                    writeln!(out, "(os/setenv \"{}\" {})", a.name, a.name).unwrap();
                }
            }
        }

        StmtKind::Command(c) => emit_command(out, ctx, c, stmt, depth),

        StmtKind::Pipeline(p) => emit_pipeline(out, ctx, p, depth),

        StmtKind::If(i) => emit_if(out, ctx, i, depth),

        StmtKind::While(w) => emit_while(out, ctx, w, depth),

        StmtKind::For(f) => {
            indent(out, depth);
            let words: Vec<String> = f.words.iter().map(|w| emit_expr_inline(w, ctx)).collect();
            ctx.declare(&f.variable);
            writeln!(out, "(each {} [{}]", f.variable, words.join(" ")).unwrap();
            for s in &f.body {
                emit_stmt(out, ctx, s, depth + 1);
            }
            indent(out, depth);
            writeln!(out, ")").unwrap();
        }

        StmtKind::Case(c) => {
            indent(out, depth);
            let val = emit_expr_inline(&c.value, ctx);
            writeln!(out, "(match {}", val).unwrap();
            for item in &c.items {
                indent(out, depth + 1);
                if item.patterns.len() == 1 {
                    let pat = emit_expr_inline(&item.patterns[0], ctx);
                    if pat == "\"*\"" {
                        write!(out, "_ ").unwrap();
                    } else {
                        write!(out, "{} ", pat).unwrap();
                    }
                } else {
                    write!(out, "_ ").unwrap();
                }
                writeln!(out, "(do").unwrap();
                for s in &item.body {
                    emit_stmt(out, ctx, s, depth + 2);
                }
                indent(out, depth + 1);
                writeln!(out, ")").unwrap();
            }
            indent(out, depth);
            writeln!(out, ")").unwrap();
        }

        StmtKind::Function(f) => {
            ctx.define_function(&f.name);
            if f.return_type == VarType::Int {
                ctx.int_funcs.insert(f.name.clone());
            }
            indent(out, depth);
            if needs_args_preamble(&f.body) {
                writeln!(out, "(defn {} [& _args]", f.name).unwrap();
            } else {
                writeln!(out, "(defn {} []", f.name).unwrap();
            }
            for s in &f.body {
                emit_stmt(out, ctx, s, depth + 1);
            }
            indent(out, depth);
            writeln!(out, ")").unwrap();
        }

        StmtKind::Redirected(r) => emit_redirected(out, ctx, r, depth),

        StmtKind::List(l) => match l.op {
            crate::ir::ListOp::And => {
                indent(out, depth);
                writeln!(out, "(when (do").unwrap();
                emit_stmt(out, ctx, &l.left, depth + 1);
                indent(out, depth);
                writeln!(out, ")").unwrap();
                emit_stmt(out, ctx, &l.right, depth + 1);
                indent(out, depth);
                writeln!(out, ")").unwrap();
            }
            crate::ir::ListOp::Or => {
                indent(out, depth);
                writeln!(out, "(when (not (do").unwrap();
                emit_stmt(out, ctx, &l.left, depth + 1);
                indent(out, depth);
                writeln!(out, "))").unwrap();
                emit_stmt(out, ctx, &l.right, depth + 1);
                indent(out, depth);
                writeln!(out, ")").unwrap();
            }
            crate::ir::ListOp::Background => {
                indent(out, depth);
                writeln!(out, "(ev/spawn (fn []").unwrap();
                emit_stmt(out, ctx, &l.left, depth + 1);
                indent(out, depth);
                writeln!(out, "))").unwrap();
            }
        },

        StmtKind::Subshell(s) => {
            indent(out, depth);
            writeln!(out, "(do").unwrap();
            for st in &s.body {
                emit_stmt(out, ctx, st, depth + 1);
            }
            indent(out, depth);
            writeln!(out, ")").unwrap();
        }

        StmtKind::Group(g) => {
            indent(out, depth);
            writeln!(out, "(do").unwrap();
            for st in &g.body {
                emit_stmt(out, ctx, st, depth + 1);
            }
            indent(out, depth);
            writeln!(out, ")").unwrap();
        }

        StmtKind::Comment(text) => {
            indent(out, depth);
            if text.starts_with("#!") {
                // Skip shebang
            } else {
                writeln!(out, "{}", text).unwrap();
            }
        }
        StmtKind::Suppressed(inner) => {
            // In Janet, suppression means ignoring the return value
            indent(out, depth);
            writeln!(out, "(do").unwrap();
            emit_stmt(out, ctx, inner, depth + 1);
            indent(out, depth);
            writeln!(out, "  nil)").unwrap();
        }
        StmtKind::ArithTest(expr) => {
            indent(out, depth);
            writeln!(out, "(not= 0 ({}))", arith_expr_to_janet(expr)).unwrap();
        }
    }
}

fn emit_if(out: &mut String, ctx: &mut EmitCtx, i: &If, depth: usize) {
    // Pre-declare any variables assigned in either branch
    // so they're visible after the if.
    let pre_declared: std::collections::HashSet<String> = ctx.declared.clone();
    let mut branch_vars = Vec::new();
    collect_assigned_vars(&i.then_body, &mut branch_vars);
    if let Some(else_body) = &i.else_body {
        collect_assigned_vars(else_body, &mut branch_vars);
    }
    for var in &branch_vars {
        if !pre_declared.contains(var) && !ctx.is_declared(var) {
            indent(out, depth);
            writeln!(out, "(var {} nil)", var).unwrap();
            ctx.declare(var);
        }
    }

    indent(out, depth);
    write!(out, "(if ").unwrap();
    // Emit condition inline
    for cond in &i.condition {
        emit_condition_inline(out, ctx, cond);
    }
    writeln!(out).unwrap();

    // Then body
    indent(out, depth + 1);
    writeln!(out, "(do").unwrap();
    for s in &i.then_body {
        emit_stmt(out, ctx, s, depth + 2);
    }
    indent(out, depth + 1);
    write!(out, ")").unwrap();

    // Else body
    if let Some(else_body) = &i.else_body {
        writeln!(out).unwrap();
        indent(out, depth + 1);
        writeln!(out, "(do").unwrap();
        for s in else_body {
            emit_stmt(out, ctx, s, depth + 2);
        }
        indent(out, depth + 1);
        write!(out, ")").unwrap();
    }
    writeln!(out, ")").unwrap();
}

fn emit_while(out: &mut String, ctx: &mut EmitCtx, w: &While, depth: usize) {
    // Check if this is `while read -r var` — emit Janet file/read loop
    let is_while_read = !w.until
        && w.condition.len() == 1
        && matches!(&w.condition[0].kind, StmtKind::Command(c) if expr_to_literal(&c.name) == "read");
    if is_while_read {
        if let StmtKind::Command(c) = &w.condition[0].kind {
            let var_name = c
                .args
                .iter()
                .map(expr_to_literal)
                .find(|a| !a.starts_with('-'))
                .unwrap_or_else(|| "line".to_string());
            ctx.declare(&var_name);
            indent(out, depth);
            writeln!(out, "(while (def _raw (file/read stdin :line))").unwrap();
            indent(out, depth + 1);
            writeln!(out, "(var {} (string/trimr _raw))", var_name).unwrap();
            for s in &w.body {
                emit_stmt(out, ctx, s, depth + 1);
            }
            indent(out, depth);
            writeln!(out, ")").unwrap();
        }
    } else {
        indent(out, depth);
        if w.until {
            write!(out, "(while (not ").unwrap();
        } else {
            write!(out, "(while ").unwrap();
        }
        for cond in &w.condition {
            emit_condition_inline(out, ctx, cond);
        }
        if w.until {
            write!(out, ")").unwrap(); // close (not ...)
        }
        writeln!(out).unwrap();
        for s in &w.body {
            emit_stmt(out, ctx, s, depth + 1);
        }
        indent(out, depth);
        writeln!(out, ")").unwrap();
    }
}

fn emit_pipeline(out: &mut String, ctx: &mut EmitCtx, p: &Pipeline, depth: usize) {
    if p.commands.len() == 1 {
        emit_stmt(out, ctx, &p.commands[0], depth);
    } else {
        // Multi-stage pipeline using os/spawn + pipes
        indent(out, depth);
        writeln!(out, "(do").unwrap();
        for (i, cmd) in p.commands.iter().enumerate() {
            let cmd_name = match &cmd.kind {
                StmtKind::Command(c) => expr_to_literal(&c.name),
                _ => "?".to_string(),
            };
            let args = match &cmd.kind {
                StmtKind::Command(c) => &c.args,
                _ => continue,
            };

            indent(out, depth + 1);
            let mut parts = vec![format!("\"{}\"", cmd_name)];
            for arg in args {
                parts.push(emit_expr_inline(arg, ctx));
            }

            if i == 0 {
                writeln!(
                    out,
                    "(def p{} (os/spawn [{}] :p {{:out :pipe}}))",
                    i,
                    parts.join(" ")
                )
                .unwrap();
            } else {
                writeln!(
                    out,
                    "(def p{} (os/spawn [{}] :p {{:in (p{} :out) :out :pipe}}))",
                    i,
                    parts.join(" "),
                    i - 1
                )
                .unwrap();
            }
        }
        // Wait for upstream processes, then read final output
        let last = p.commands.len() - 1;
        for i in 0..last {
            indent(out, depth + 1);
            writeln!(out, "(os/proc-wait p{})", i).unwrap();
        }
        indent(out, depth + 1);
        writeln!(
            out,
            "(def result (string/trim (:read (p{} :out) :all)))",
            last
        )
        .unwrap();
        indent(out, depth + 1);
        writeln!(out, "(os/proc-wait p{})", last).unwrap();
        indent(out, depth + 1);
        writeln!(out, "(print result))").unwrap();
    }
}

fn emit_command(out: &mut String, ctx: &mut EmitCtx, c: &Command, _stmt: &Statement, depth: usize) {
    let cmd_name = expr_to_literal(&c.name);
    let kind = BuiltinCmd::from_name(&cmd_name);
    indent(out, depth);
    match kind {
        BuiltinCmd::Echo => emit_janet_echo(out, ctx, c),
        BuiltinCmd::True => emit_janet_const_bool(out, true),
        BuiltinCmd::False => emit_janet_const_bool(out, false),
        BuiltinCmd::Exit => emit_janet_exit(out, ctx, c),
        BuiltinCmd::Shift => {
            writeln!(out, "(set _args (array/slice _args 1))").unwrap();
        }
        BuiltinCmd::Test => emit_janet_test_cmd(out, ctx, c),
        BuiltinCmd::Break | BuiltinCmd::Continue => {
            // Janet's (each) doesn't support continue; use (break) as approximation
            writeln!(out, "(break)").unwrap();
        }
        BuiltinCmd::Return => emit_janet_return(out, ctx, c),
        BuiltinCmd::Printf => emit_janet_printf(out, ctx, c),
        BuiltinCmd::Read => emit_janet_read(out, ctx, c),
        BuiltinCmd::Bless => emit_janet_bless(out, c),
        BuiltinCmd::Unset => emit_janet_unset(out, ctx, c),
        // `cat` is not a builtin — special-cased so we can shortcut
        // stdin-only invocations to (file/read stdin :all).
        _ if cmd_name == "cat" => emit_janet_cat(out, ctx, c),
        _ => emit_janet_general_command(out, ctx, c, &cmd_name),
    }
}

/// `true` / `false` — set `__last_exit` and emit the literal.
fn emit_janet_const_bool(out: &mut String, success: bool) {
    let (exit_code, literal) = if success { (0, "true") } else { (1, "false") };
    writeln!(out, "(set __last_exit {})", exit_code).unwrap();
    writeln!(out, "{}", literal).unwrap();
}

/// `[[ … ]]` form attached as a `test` command argument. Emits the
/// test expression inline; silently no-ops if no Test arg is present.
fn emit_janet_test_cmd(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    if let Some(arg) = c.args.first() {
        write!(out, "{}", emit_test_expr(arg, ctx)).unwrap();
        writeln!(out).unwrap();
    }
}

/// `return [code]` — Janet's `(return …)` form. Omits the operand
/// when no code is supplied.
fn emit_janet_return(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    if let Some(code) = c.args.first() {
        let val = emit_expr_inline(code, ctx);
        writeln!(out, "(return {})", val).unwrap();
    } else {
        writeln!(out, "(return)").unwrap();
    }
}

/// `bless var [[ … ]]` — type/range/regex assertion. Janet emits a
/// comment only; the runtime check is elided in the Janet backend.
fn emit_janet_bless(out: &mut String, c: &Command) {
    if !c.args.is_empty() {
        let var_name = expr_to_literal(&c.args[0]);
        writeln!(out, "# bless {} (validation elided in janet)", var_name).unwrap();
    }
}

/// `unset var…` — set each declared variable to nil.
fn emit_janet_unset(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    for arg in &c.args {
        let name = expr_to_literal(arg);
        if ctx.is_declared(&name) {
            writeln!(out, "(set {} nil)", name).unwrap();
        }
    }
}

/// Default `_ =>` arm — known function call (Janet sexpr) or generic
/// external command (`os/execute`).
fn emit_janet_general_command(out: &mut String, ctx: &mut EmitCtx, c: &Command, cmd_name: &str) {
    if ctx.is_function(cmd_name) {
        let mut args_str = String::new();
        for arg in &c.args {
            args_str.push(' ');
            args_str.push_str(&emit_expr_inline(arg, ctx));
        }
        writeln!(out, "({}{})", cmd_name, args_str).unwrap();
    } else {
        let mut cmd_parts = vec![format!("\"{}\"", cmd_name)];
        for arg in &c.args {
            cmd_parts.push(emit_expr_inline(arg, ctx));
        }
        writeln!(out, "(os/execute [{}] :p)", cmd_parts.join(" ")).unwrap();
    }
}

fn emit_janet_echo(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    if c.args.is_empty() {
        writeln!(out, "(print)").unwrap();
    } else {
        let parts: Vec<String> = c.args.iter().map(|a| emit_expr_inline(a, ctx)).collect();
        if parts.len() == 1 {
            writeln!(out, "(print {})", parts[0]).unwrap();
        } else {
            writeln!(out, "(print (string {}))", parts.join(" \" \" ")).unwrap();
        }
    }
}

fn emit_janet_exit(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    if let Some(code) = c.args.first() {
        let code_str = emit_expr_inline(code, ctx);
        // Strip quotes for numeric exit code
        let code_val = code_str.trim_matches('"');
        if code_val.chars().all(|c| c.is_ascii_digit() || c == '-') {
            writeln!(out, "(os/exit {})", code_val).unwrap();
        } else {
            writeln!(out, "(os/exit (scan-number {}))", code_str).unwrap();
        }
    } else {
        writeln!(out, "(os/exit 0)").unwrap();
    }
}

fn emit_janet_printf(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    let args: Vec<String> = c.args.iter().map(|a| emit_expr_inline(a, ctx)).collect();
    if args.len() >= 2 {
        writeln!(
            out,
            "(prin (string/format {} {}))",
            args[0],
            args[1..].join(" ")
        )
        .unwrap();
    } else if args.len() == 1 {
        writeln!(out, "(prin {})", args[0]).unwrap();
    }
}

fn emit_janet_read(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    let var_name = c
        .args
        .iter()
        .map(expr_to_literal)
        .find(|a| !a.starts_with('-'))
        .unwrap_or_else(|| "REPLY".to_string());
    if !ctx.is_declared(&var_name) {
        ctx.declare(&var_name);
        writeln!(
            out,
            "(var {} (string/trimr (or (file/read stdin :line) \"\")))",
            var_name
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "(set {} (string/trimr (or (file/read stdin :line) \"\")))",
            var_name
        )
        .unwrap();
    }
}

fn emit_janet_cat(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    if c.args.is_empty() {
        writeln!(out, "(prin (file/read stdin :all))").unwrap();
    } else {
        let mut cmd_parts = vec!["\"cat\"".to_string()];
        for arg in &c.args {
            cmd_parts.push(emit_expr_inline(arg, ctx));
        }
        writeln!(out, "(os/execute [{}] :p)", cmd_parts.join(" ")).unwrap();
    }
}

fn emit_redirected(out: &mut String, ctx: &mut EmitCtx, r: &Redirected, depth: usize) {
    // Check for heredoc/herestring content
    let has_heredoc = r
        .redirects
        .iter()
        .any(|rd| matches!(rd.op, RedirectOp::HereDoc | RedirectOp::HereString));
    if has_heredoc {
        // Find the heredoc body
        let body_text = r
            .redirects
            .iter()
            .filter(|rd| matches!(rd.op, RedirectOp::HereDoc | RedirectOp::HereString))
            .map(|rd| expr_to_literal(&rd.target))
            .collect::<Vec<_>>()
            .join("");
        // Check if body is "cat" with no args (heredoc-as-string pattern)
        let is_cat = matches!(&r.body.kind, StmtKind::Command(c) if expr_to_literal(&c.name) == "cat" && c.args.is_empty());
        if is_cat {
            // cat <<EOF → just print the body
            indent(out, depth);
            // Escape for Janet string literal
            let escaped = body_text
                .trim_end_matches('\n')
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n");
            writeln!(out, "(print \"{}\")", escaped).unwrap();
        } else if matches!(&r.body.kind, StmtKind::Command(c) if expr_to_literal(&c.name) == "read")
        {
            // read <<< "string" — herestring
            if let StmtKind::Command(c) = &r.body.kind {
                let var_name = c
                    .args
                    .iter()
                    .map(expr_to_literal)
                    .find(|a| !a.starts_with('-'))
                    .unwrap_or_else(|| "REPLY".to_string());
                let val = body_text.trim_end_matches('\n');
                if !ctx.is_declared(&var_name) {
                    ctx.declare(&var_name);
                    indent(out, depth);
                    writeln!(out, "(var {} \"{}\")", var_name, val).unwrap();
                } else {
                    indent(out, depth);
                    writeln!(out, "(set {} \"{}\")", var_name, val).unwrap();
                }
            }
        } else {
            // Other command with heredoc — pipe body text to stdin
            indent(out, depth);
            let escaped = body_text
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n");
            writeln!(out, "(do").unwrap();
            indent(out, depth + 1);
            writeln!(out, "(def _heredoc_body \"{}\")", escaped).unwrap();
            // Write body to a temp pipe and redirect
            let cmd_name = match &r.body.kind {
                StmtKind::Command(c) => expr_to_literal(&c.name),
                _ => "?".to_string(),
            };
            let args = match &r.body.kind {
                StmtKind::Command(c) => c
                    .args
                    .iter()
                    .map(|a| emit_expr_inline(a, ctx))
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            let mut parts = vec![format!("\"{}\"", cmd_name)];
            parts.extend(args);
            indent(out, depth + 1);
            writeln!(
                out,
                "(def p (os/spawn [{}] :p {{:in :pipe}}))",
                parts.join(" ")
            )
            .unwrap();
            indent(out, depth + 1);
            writeln!(out, "(:write (p :in) _heredoc_body)").unwrap();
            indent(out, depth + 1);
            writeln!(out, "(:close (p :in))").unwrap();
            indent(out, depth + 1);
            writeln!(out, "(os/proc-wait p)").unwrap();
            indent(out, depth);
            writeln!(out, ")").unwrap();
        }
    } else {
        // Non-heredoc redirects — emit body, note redirect as comment
        for rd in &r.redirects {
            indent(out, depth);
            writeln!(out, "# redirect: fd={} op={:?}", rd.fd.unwrap_or(1), rd.op).unwrap();
        }
        emit_stmt(out, ctx, &r.body, depth);
    }
}

/// Emit a condition (test_command) inline for if/while.
fn emit_condition_inline(out: &mut String, ctx: &EmitCtx, stmt: &Statement) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            let name = expr_to_literal(&c.name);
            if name == "[[" {
                if let Some(arg) = c.args.first() {
                    write!(out, "{}", emit_test_expr(arg, ctx)).unwrap();
                } else {
                    write!(out, "true").unwrap();
                }
            } else {
                // Non-test condition — command success
                let mut parts = vec![format!("\"{}\"", name)];
                for arg in &c.args {
                    parts.push(emit_expr_inline(arg, ctx));
                }
                write!(out, "(zero? (os/execute [{}] :p))", parts.join(" ")).unwrap();
            }
        }
        _ => write!(out, "true").unwrap(),
    }
}

/// Check if an expression is known to be integer-typed.
#[allow(dead_code)]
fn is_int_expr(expr: &Expr, ctx: &EmitCtx) -> bool {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => ctx.is_int(var),
        ExprKind::Literal(s) => s.chars().all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// Emit a test expression as a Janet boolean expression.
fn emit_test_expr(expr: &Expr, ctx: &EmitCtx) -> String {
    match &expr.kind {
        ExprKind::Test(test) => match test {
            TestExpr::Unary { op, operand } => {
                let val = emit_expr_inline(operand, ctx);
                match op.as_str() {
                    "-f" => format!("(= (os/stat {} :mode) :file)", val),
                    "-d" => format!("(= (os/stat {} :mode) :directory)", val),
                    "-e" => format!("(not (nil? (os/stat {} :mode)))", val),
                    "-n" => format!("(not (or (nil? {0}) (empty? {0})))", val),
                    "-z" => format!("(or (nil? {0}) (empty? {0}))", val),
                    _ => format!("(truthy? {} {})", op, val),
                }
            }
            TestExpr::Binary { left, op, right } => {
                let is_numeric_op =
                    matches!(op.as_str(), "-eq" | "-ne" | "-gt" | "-lt" | "-ge" | "-le");
                let l = if is_numeric_op {
                    emit_expr_as_int(left, ctx)
                } else {
                    emit_expr_inline(left, ctx)
                };
                let r = if is_numeric_op {
                    emit_expr_as_int(right, ctx)
                } else {
                    emit_expr_inline(right, ctx)
                };
                match op.as_str() {
                    "=" | "==" => format!("(= {} {})", l, r),
                    "!=" => format!("(not= {} {})", l, r),
                    "-eq" => format!("(= {} {})", l, r),
                    "-ne" => format!("(not= {} {})", l, r),
                    "-gt" => format!("(> {} {})", l, r),
                    "-lt" => format!("(< {} {})", l, r),
                    "-ge" => format!("(>= {} {})", l, r),
                    "-le" => format!("(<= {} {})", l, r),
                    _ => format!("({} {} {})", op, l, r),
                }
            }
            TestExpr::Not(inner) => {
                let inner_expr = Expr {
                    kind: ExprKind::Test(*inner.clone()),
                    span: expr.span,
                };
                format!("(not {})", emit_test_expr(&inner_expr, ctx))
            }
            TestExpr::And(l, r) => {
                let le = Expr {
                    kind: ExprKind::Test(*l.clone()),
                    span: expr.span,
                };
                let re = Expr {
                    kind: ExprKind::Test(*r.clone()),
                    span: expr.span,
                };
                format!(
                    "(and {} {})",
                    emit_test_expr(&le, ctx),
                    emit_test_expr(&re, ctx)
                )
            }
            TestExpr::Or(l, r) => {
                let le = Expr {
                    kind: ExprKind::Test(*l.clone()),
                    span: expr.span,
                };
                let re = Expr {
                    kind: ExprKind::Test(*r.clone()),
                    span: expr.span,
                };
                format!(
                    "(or {} {})",
                    emit_test_expr(&le, ctx),
                    emit_test_expr(&re, ctx)
                )
            }
            TestExpr::Paren(inner) => {
                let ie = Expr {
                    kind: ExprKind::Test(*inner.clone()),
                    span: expr.span,
                };
                emit_test_expr(&ie, ctx)
            }
        },
        _ => "true".to_string(),
    }
}

/// Emit an expression as a Janet integer value.
/// Strips quotes from literals, converts string vars via scan-number.
fn emit_expr_as_int(expr: &Expr, ctx: &EmitCtx) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => {
            // Bare number — emit without quotes
            if s.chars().all(|c| c.is_ascii_digit() || c == '-') {
                s.clone()
            } else {
                format!("(scan-number \"{}\")", s)
            }
        }
        ExprKind::SimpleExpansion(var) => {
            if var == "#" {
                "(length _args)".to_string()
            } else if ctx.is_int(var) {
                var.clone()
            } else {
                format!("(scan-number {})", emit_expr_inline(expr, ctx))
            }
        }
        _ => format!("(scan-number {})", emit_expr_inline(expr, ctx)),
    }
}

/// Emit an expression as an inline Janet value.
fn emit_expr_inline(expr: &Expr, ctx: &EmitCtx) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => format!("\"{}\"", s),
        ExprKind::SimpleExpansion(var) => {
            if ctx.is_declared(var) {
                var.clone()
            } else if var == "#" {
                "(string (length _args))".to_string()
            } else if var == "@" {
                "(string/join _args \" \")".to_string()
            } else if let Ok(n) = var.parse::<u32>() {
                let idx = n.saturating_sub(1) as usize;
                format!("(or (get _args {}) \"\")", idx)
            } else if var == "?" {
                "(string __last_exit)".to_string()
            } else {
                format!("(or (os/getenv \"{}\") \"\")", var)
            }
        }
        ExprKind::SingleQuoted(s) => format!("\"{}\"", s),
        ExprKind::DoubleQuoted(parts) => {
            if parts.is_empty() {
                "\"\"".to_string()
            } else if parts.len() == 1 {
                emit_expr_inline(&parts[0], ctx)
            } else {
                let ps: Vec<String> = parts.iter().map(|p| emit_expr_inline(p, ctx)).collect();
                format!("(string {})", ps.join(" "))
            }
        }
        ExprKind::ParameterExpansion(pe) => emit_param_expansion_inline(pe, ctx),
        ExprKind::CommandSubstitution(stmts) => {
            // $(cmd) → capture stdout
            if let Some(stmt) = stmts.first() {
                let inner = match &stmt.kind {
                    StmtKind::Redirected(r) => &r.body.kind,
                    other => other,
                };
                if let StmtKind::Command(c) = inner {
                    let name = expr_to_literal(&c.name);
                    let mut parts = vec![format!("\"{}\"", name)];
                    for arg in &c.args {
                        parts.push(emit_expr_inline(arg, ctx));
                    }
                    return format!(
                        "(string/trim (with [f (os/spawn [{}] :p {{:out :pipe}})] (:read (f :out) :all)))",
                        parts.join(" ")
                    );
                }
            }
            "\"\"".to_string()
        }
        ExprKind::Arithmetic(a) => {
            format!("(string ({}))", arith_expr_to_janet(a))
        }
        ExprKind::Concat(parts) => {
            let ps: Vec<String> = parts.iter().map(|p| emit_expr_inline(p, ctx)).collect();
            format!("(string {})", ps.join(" "))
        }
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(|e| emit_expr_inline(e, ctx)).collect();
            format!("@[{}]", es.join(" "))
        }
        ExprKind::AssocArray(pairs) => {
            let es: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{:?} {}", k, emit_expr_inline(v, ctx)))
                .collect();
            format!("@{{{}}}", es.join(" "))
        }
        ExprKind::Subscript { name, index } => {
            let idx = emit_expr_inline(index, ctx);
            // Use numeric index for arrays
            let idx_str = idx.trim_matches('"');
            if idx_str.chars().all(|c| c.is_ascii_digit()) {
                format!("(get {} {})", name, idx_str)
            } else {
                format!("(get {} {})", name, idx)
            }
        }
        ExprKind::Test(test) => {
            let wrapper = Expr {
                kind: ExprKind::Test(test.clone()),
                span: expr.span,
            };
            emit_test_expr(&wrapper, ctx)
        }
    }
}

fn emit_param_expansion_inline(pe: &ParameterExpansion, ctx: &EmitCtx) -> String {
    let Some(op) = pe.operator.as_deref() else {
        return if ctx.is_declared(&pe.name) {
            pe.name.clone()
        } else {
            format!("(or (os/getenv \"{}\") \"\")", pe.name)
        };
    };
    let var_ref = if ctx.is_declared(&pe.name) {
        pe.name.clone()
    } else {
        format!("(or (os/getenv \"{}\") \"\")", pe.name)
    };
    match op {
        ":-" => emit_pe_default(pe, ctx),
        "%%" => emit_pe_suffix_strip_longest(&var_ref, &pe_operand_literal(pe)),
        "%" => emit_pe_suffix_strip_shortest(&var_ref, &pe_operand_literal(pe)),
        "##" => emit_pe_prefix_strip_longest(&var_ref, &pe_operand_literal(pe)),
        "#" => emit_pe_prefix_strip_shortest(&var_ref, &pe_operand_literal(pe)),
        "^^" | ",," | "^" | "," => emit_pe_case_op(&var_ref, op),
        "strlen" | "arrlen" => format!("(string (length {}))", var_ref),
        "substring" => emit_pe_substring(&var_ref, &pe_operand_literal(pe)),
        "/" => emit_pe_replace(&var_ref, &pe_operand_literal(pe), false),
        "//" => emit_pe_replace(&var_ref, &pe_operand_literal(pe), true),
        _ => var_ref,
    }
}

/// Extract the operand of a `ParameterExpansion` as a literal string, or
/// `""` if absent.
fn pe_operand_literal(pe: &ParameterExpansion) -> String {
    pe.operand
        .as_ref()
        .map(|o| expr_to_literal(o))
        .unwrap_or_default()
}

/// `${var:-default}` — use default when var is empty or unset.
fn emit_pe_default(pe: &ParameterExpansion, ctx: &EmitCtx) -> String {
    let default = pe
        .operand
        .as_ref()
        .map(|o| emit_expr_inline(o, ctx))
        .unwrap_or_else(|| "\"\"".to_string());
    if ctx.is_declared(&pe.name) {
        format!("(if (= (length {}) 0) {} {})", pe.name, default, pe.name)
    } else {
        format!("(or (os/getenv \"{}\") {})", pe.name, default)
    }
}

/// `${var%%pat}` — longest suffix strip.
fn emit_pe_suffix_strip_longest(var_ref: &str, operand: &str) -> String {
    if let Some(lit) = operand.strip_suffix('*') {
        format!(
            "(let [s {} i (string/find \"{}\" s)] (if i (string/slice s 0 i) s))",
            var_ref, lit
        )
    } else if let Some(lit) = operand.strip_prefix('*') {
        format!(
            "(let [s {} i (string/find \"{}\" s)] (if i (string/slice s 0 i) s))",
            var_ref, lit
        )
    } else {
        format!("(let [s {} pat \"{}\"] (if (string/has-suffix? pat s) (string/slice s 0 (- (length s) (length pat))) s))",
            var_ref, operand)
    }
}

/// `${var%pat}` — shortest suffix strip.
fn emit_pe_suffix_strip_shortest(var_ref: &str, operand: &str) -> String {
    if let Some(lit) = operand.strip_suffix('*') {
        format!(
            "(let [s {} i (last (string/find-all \"{}\" s))] (if i (string/slice s 0 i) s))",
            var_ref, lit
        )
    } else if let Some(lit) = operand.strip_prefix('*') {
        format!(
            "(let [s {} i (last (string/find-all \"{}\" s))] (if i (string/slice s 0 i) s))",
            var_ref, lit
        )
    } else {
        format!("(let [s {} pat \"{}\"] (if (string/has-suffix? pat s) (string/slice s 0 (- (length s) (length pat))) s))",
            var_ref, operand)
    }
}

/// `${var##pat}` — longest prefix strip.
fn emit_pe_prefix_strip_longest(var_ref: &str, operand: &str) -> String {
    if let Some(lit) = operand.strip_prefix('*') {
        format!("(let [s {} ps (string/find-all \"{}\" s)] (if (empty? ps) s (string/slice s (+ (last ps) (length \"{}\")))))",
            var_ref, lit, lit)
    } else if let Some(lit) = operand.strip_suffix('*') {
        format!("(let [s {} ps (string/find-all \"{}\" s)] (if (empty? ps) s (string/slice s (+ (last ps) (length \"{}\")))))",
            var_ref, lit, lit)
    } else {
        format!("(let [s {} pat \"{}\"] (if (string/has-prefix? pat s) (string/slice s (length pat)) s))",
            var_ref, operand)
    }
}

/// `${var#pat}` — shortest prefix strip.
fn emit_pe_prefix_strip_shortest(var_ref: &str, operand: &str) -> String {
    if let Some(lit) = operand.strip_prefix('*') {
        format!(
            "(let [s {} i (string/find \"{}\" s)] (if i (string/slice s (+ i (length \"{}\"))) s))",
            var_ref, lit, lit
        )
    } else if let Some(lit) = operand.strip_suffix('*') {
        format!(
            "(let [s {} i (string/find \"{}\" s)] (if i (string/slice s (+ i (length \"{}\"))) s))",
            var_ref, lit, lit
        )
    } else {
        format!("(let [s {} pat \"{}\"] (if (string/has-prefix? pat s) (string/slice s (length pat)) s))",
            var_ref, operand)
    }
}

/// `${var^^}`, `${var,,}`, `${var^}`, `${var,}` — case transforms.
fn emit_pe_case_op(var_ref: &str, op: &str) -> String {
    match op {
        "^^" => format!("(string/ascii-upper {})", var_ref),
        ",," => format!("(string/ascii-lower {})", var_ref),
        "^" => format!(
            "(string (string/ascii-upper (string/slice {} 0 1)) (string/slice {} 1))",
            var_ref, var_ref
        ),
        "," => format!(
            "(string (string/ascii-lower (string/slice {} 0 1)) (string/slice {} 1))",
            var_ref, var_ref
        ),
        _ => var_ref.to_string(),
    }
}

/// `${var:off}` or `${var:off:len}` — substring (negative len trims from end).
fn emit_pe_substring(var_ref: &str, operand: &str) -> String {
    if let Some((off, len)) = operand.split_once(':') {
        let off_n: i64 = off.parse().unwrap_or(0);
        let len_n: i64 = len.parse().unwrap_or(-1);
        if len_n < 0 {
            format!(
                "(string/slice {} {} (+ (length {}) {}))",
                var_ref, off_n, var_ref, len_n
            )
        } else {
            format!("(string/slice {} {} {})", var_ref, off_n, off_n + len_n)
        }
    } else {
        let off_n: i64 = operand.parse().unwrap_or(0);
        format!("(string/slice {} {})", var_ref, off_n)
    }
}

/// `${var/pat/rep}` (global = false) or `${var//pat/rep}` (global = true) —
/// pattern replace. Operand is `pat\0rep` from the parser.
fn emit_pe_replace(var_ref: &str, operand: &str, global: bool) -> String {
    if let Some((pat, rep)) = operand.split_once('\0') {
        let fn_name = if global {
            "string/replace-all"
        } else {
            "string/replace"
        };
        format!("({} \"{}\" \"{}\" {})", fn_name, pat, rep, var_ref)
    } else {
        var_ref.to_string()
    }
}

/// Collect variable names assigned in a statement list.
fn collect_assigned_vars(stmts: &[Statement], vars: &mut Vec<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assignment(a) => {
                if !vars.contains(&a.name) {
                    vars.push(a.name.clone());
                }
            }
            StmtKind::Declaration(d) => {
                for a in &d.assignments {
                    if !vars.contains(&a.name) {
                        vars.push(a.name.clone());
                    }
                }
            }
            StmtKind::If(i) => {
                collect_assigned_vars(&i.then_body, vars);
                if let Some(eb) = &i.else_body {
                    collect_assigned_vars(eb, vars);
                }
            }
            StmtKind::While(w) => collect_assigned_vars(&w.body, vars),
            StmtKind::For(f) => collect_assigned_vars(&f.body, vars),
            _ => {}
        }
    }
}

fn expr_to_literal(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => s.clone(),
        ExprKind::SimpleExpansion(var) => format!("${}", var),
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => expr_to_literal(&parts[0]),
        _ => "?".to_string(),
    }
}

/// Convert a structured ArithExpr to Janet code.
fn arith_expr_to_janet(expr: &crate::ir::ArithExpr) -> String {
    use crate::ir::{ArithExpr, IrArithOp, IrUnaryArithOp};
    match expr {
        ArithExpr::Literal(n) => n.to_string(),
        ArithExpr::Var(name) => name.clone(),
        ArithExpr::DollarVar(name) => name.clone(),
        ArithExpr::BinOp { left, op, right } => {
            let op_str = match op {
                IrArithOp::Add => "+",
                IrArithOp::Sub => "-",
                IrArithOp::Mul => "*",
                IrArithOp::Div => "/",
                IrArithOp::Mod => "%",
                IrArithOp::Pow => "math/pow",
                IrArithOp::Eq => "=",
                IrArithOp::Ne => "not=",
                IrArithOp::Lt => "<",
                IrArithOp::Le => "<=",
                IrArithOp::Gt => ">",
                IrArithOp::Ge => ">=",
                IrArithOp::And => "and",
                IrArithOp::Or => "or",
                IrArithOp::Shl => "blshift",
                IrArithOp::Shr => "brshift",
                IrArithOp::BitAnd => "band",
                IrArithOp::BitOr => "bor",
                IrArithOp::BitXor => "bxor",
            };
            format!(
                "({} {} {})",
                op_str,
                arith_expr_to_janet(left),
                arith_expr_to_janet(right)
            )
        }
        ArithExpr::UnaryOp { op, operand } => {
            let op_str = match op {
                IrUnaryArithOp::Neg => "-",
                IrUnaryArithOp::Pos => "+",
                IrUnaryArithOp::Not => "not",
                IrUnaryArithOp::BitNot => "bnot",
            };
            format!("({} {})", op_str, arith_expr_to_janet(operand))
        }
        ArithExpr::PreIncDec { var, inc } | ArithExpr::PostIncDec { var, inc } => {
            if *inc {
                format!("(+ {} 1)", var)
            } else {
                format!("(- {} 1)", var)
            }
        }
        ArithExpr::Assign { var, value } => {
            format!("(set {} {})", var, arith_expr_to_janet(value))
        }
        ArithExpr::CompoundAssign { var, op, value } => {
            let op_str = match op {
                IrArithOp::Add => "+",
                IrArithOp::Sub => "-",
                IrArithOp::Mul => "*",
                IrArithOp::Div => "/",
                IrArithOp::Mod => "%",
                _ => "?",
            };
            format!(
                "(set {} ({} {} {}))",
                var,
                op_str,
                var,
                arith_expr_to_janet(value)
            )
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            format!(
                "(if {} {} {})",
                arith_expr_to_janet(cond),
                arith_expr_to_janet(true_val),
                arith_expr_to_janet(false_val)
            )
        }
        ArithExpr::Comma(exprs) => {
            let parts: Vec<String> = exprs.iter().map(arith_expr_to_janet).collect();
            format!("(do {})", parts.join(" "))
        }
        ArithExpr::FuncCall { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(arith_expr_to_janet).collect();
            if arg_strs.is_empty() {
                format!("({})", name)
            } else {
                format!("({} {})", name, arg_strs.join(" "))
            }
        }
        ArithExpr::Paren(inner) => arith_expr_to_janet(inner),
    }
}
