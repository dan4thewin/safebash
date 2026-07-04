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
    _stmt_direct_uses_args(stmt) || stmt.child_bodies().iter().any(|b| needs_args_preamble(b))
}

/// Args referenced by a statement's own exprs/condition, not its child
/// bodies (those are covered by recursing `child_bodies`). `shift` and a
/// bare `(( … ))` (`ArithTest`, whose `ArithExpr` `direct_exprs` cannot
/// surface) are handled explicitly.
fn _stmt_direct_uses_args(stmt: &Statement) -> bool {
    if let StmtKind::Command(c) = &stmt.kind {
        if expr_to_literal(&c.name) == "shift" {
            return true;
        }
    }
    if let StmtKind::ArithTest(a) = &stmt.kind {
        return _arith_uses_args(a);
    }
    let mut found = false;
    stmt.direct_exprs(&mut |e| found = found || _expr_uses_args(e));
    found
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
        ExprKind::Arithmetic(a) => _arith_uses_args(a),
        _ => false,
    }
}

/// Whether an arithmetic expression reads a positional that requires the
/// `_args` preamble (`$#`, `$@`, `$*`, `$N`). `$?` reads `__last_exit`
/// instead, so it does not force the preamble.
fn _arith_uses_args(a: &crate::ir::ArithExpr) -> bool {
    use crate::ir::ArithExpr::*;
    match a {
        DollarVar(name) => is_positional_arg_name(name),
        BinOp { left, right, .. } => _arith_uses_args(left) || _arith_uses_args(right),
        UnaryOp { operand, .. } => _arith_uses_args(operand),
        Assign { value, .. } | CompoundAssign { value, .. } => _arith_uses_args(value),
        Ternary {
            cond,
            true_val,
            false_val,
        } => _arith_uses_args(cond) || _arith_uses_args(true_val) || _arith_uses_args(false_val),
        Comma(exprs) | FuncCall { args: exprs, .. } => exprs.iter().any(_arith_uses_args),
        Paren(inner) => _arith_uses_args(inner),
        Literal(_) | Var(_) | PostIncDec { .. } | PreIncDec { .. } => false,
    }
}

/// Whether `$name` names a positional that needs the `_args` preamble
/// (`$#`, `$@`, `$*`, `$N`). `$?` uses `__last_exit`, not `_args`.
fn is_positional_arg_name(name: &str) -> bool {
    name == "#"
        || name == "@"
        || name == "*"
        || (name != "0" && !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()))
}

fn _test_uses_args(test: &TestExpr) -> bool {
    match test {
        TestExpr::Binary { left, right, .. } => _expr_uses_args(left) || _expr_uses_args(right),
        TestExpr::Unary { operand, .. } => _expr_uses_args(operand),
        TestExpr::Not(inner) => _test_uses_args(inner),
        TestExpr::And(a, b) | TestExpr::Or(a, b) => _test_uses_args(a) || _test_uses_args(b),
    }
}

/// Tracks which variables have been declared (var vs set) and their types.
struct EmitCtx {
    declared: std::collections::HashSet<String>,
    int_vars: std::collections::HashSet<String>,
    int_funcs: std::collections::HashSet<String>,
    functions: std::collections::HashSet<String>,
    map_vars: std::collections::HashSet<String>,
    /// DD#55 (A): whether a failing external command in the current position
    /// must abort. False in `-`-suppressed and `&&`/`||` left-operand
    /// positions (the failure is caught there). Conditions use a separate
    /// emit path and never abort.
    checked: bool,
}

impl EmitCtx {
    fn new() -> Self {
        Self {
            declared: std::collections::HashSet::new(),
            int_vars: std::collections::HashSet::new(),
            int_funcs: std::collections::HashSet::new(),
            functions: std::collections::HashSet::new(),
            map_vars: std::collections::HashSet::new(),
            checked: true,
        }
    }

    fn is_declared(&self, name: &str) -> bool {
        self.declared.contains(name)
    }

    fn is_int(&self, name: &str) -> bool {
        self.int_vars.contains(name)
    }

    /// A `map` (assoc array) is keyed by strings; an indexed array is keyed
    /// by integers. janet `put`/`get` require the matching key type.
    fn is_map(&self, name: &str) -> bool {
        self.map_vars.contains(name)
    }

    #[allow(dead_code)]
    fn is_int_func(&self, name: &str) -> bool {
        self.int_funcs.contains(name)
    }

    fn declare(&mut self, name: &str) {
        self.declared.insert(name.to_string());
    }

    fn declare_map(&mut self, name: &str) {
        self.declared.insert(name.to_string());
        self.map_vars.insert(name.to_string());
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
        StmtKind::Assignment(a) => emit_assignment(out, ctx, a, depth),
        StmtKind::Declaration(d) => emit_declaration(out, ctx, d, depth),
        StmtKind::Command(c) => emit_command(out, ctx, c, stmt, depth),
        StmtKind::Pipeline(p) => emit_pipeline(out, ctx, p, depth),
        StmtKind::If(i) => emit_if(out, ctx, i, depth),
        StmtKind::While(w) => emit_while(out, ctx, w, depth),
        StmtKind::For(f) => emit_for(out, ctx, f, depth),
        StmtKind::Case(c) => emit_case(out, ctx, c, depth),
        StmtKind::Function(f) => emit_function_def(out, ctx, f, depth),
        StmtKind::Redirected(r) => emit_redirected(out, ctx, r, depth),
        StmtKind::List(l) => emit_list(out, ctx, l, depth),
        StmtKind::Group(g) => emit_group(out, ctx, g, depth),
        StmtKind::Comment(text) => emit_comment(out, text, depth),
        StmtKind::Suppressed(inner) => emit_suppressed(out, ctx, inner, depth),
        StmtKind::ArithTest(expr) => emit_arith_test(out, expr, depth),
    }
}

fn emit_assignment(out: &mut String, ctx: &mut EmitCtx, a: &Assignment, depth: usize) {
    indent(out, depth);
    // Indexed assignment `a[i]=v` / `a[i]+=v` mutates one element via janet
    // `put` (arrays take integer indices, maps string keys).
    if a.index.is_some() {
        emit_indexed_assignment(out, ctx, a);
        return;
    }
    // An int-typed target keeps arithmetic numeric so later `(( ))` reads and
    // comparisons work (string-wrapping would make `(( i += 1 ))` set i to a
    // string and break the next comparison). String targets stay strings.
    let val = if ctx.is_int(&a.name) {
        emit_expr_as_int(&a.value, ctx)
    } else {
        emit_expr_inline(&a.value, ctx)
    };
    if ctx.is_declared(&a.name) {
        writeln!(out, "(set {} {})", a.name, val).unwrap();
    } else {
        ctx.declare(&a.name);
        writeln!(out, "(var {} {})", a.name, val).unwrap();
    }
}

/// `a[i]=v` / `a[i]+=v` → `(put a <idx> <val>)`. The index is an integer for
/// an indexed array and a string key for a map; append concatenates onto the
/// current element via `(string (get a <idx>) <val>)`.
fn emit_indexed_assignment(out: &mut String, ctx: &EmitCtx, a: &Assignment) {
    let idx = a.index.as_ref().expect("indexed assignment has an index");
    let index = if ctx.is_map(&a.name) {
        emit_expr_inline(idx, ctx)
    } else {
        emit_expr_as_int(idx, ctx)
    };
    let rhs = emit_expr_inline(&a.value, ctx);
    let value = if a.op == AssignOp::Append {
        format!("(string (get {n} {i}) {r})", n = a.name, i = index, r = rhs)
    } else {
        rhs
    };
    writeln!(out, "(put {} {} {})", a.name, index, value).unwrap();
}

fn emit_declaration(out: &mut String, ctx: &mut EmitCtx, d: &Declaration, depth: usize) {
    for a in &d.assignments {
        emit_declaration_assignment(out, ctx, d, a, depth);
    }
}

fn emit_declaration_assignment(
    out: &mut String,
    ctx: &mut EmitCtx,
    d: &Declaration,
    a: &Assignment,
    depth: usize,
) {
    indent(out, depth);
    let val = emit_decl_value(d, a, ctx);
    if ctx.is_declared(&a.name) {
        writeln!(out, "(set {} {})", a.name, val).unwrap();
    } else {
        declare_decl_var(ctx, d, a);
        let kw = if d.keyword == DeclKeyword::Readonly {
            "def"
        } else {
            "var"
        };
        writeln!(out, "({} {} {})", kw, a.name, val).unwrap();
    }
    if d.keyword == DeclKeyword::Export {
        indent(out, depth);
        writeln!(out, "(os/setenv \"{}\" {})", a.name, a.name).unwrap();
    }
}

/// Record a declared variable's janet type: int (numeric), map (assoc array,
/// string-keyed), or a plain string/indexed-array binding.
/// Compute the janet initializer for a declaration's assignment: int
/// coercion, a DD#60 `$(argv N)` snapshot (clamped so N >= argc → empty
/// array), or a plain inline value.
fn emit_decl_value(d: &Declaration, a: &Assignment, ctx: &EmitCtx) -> String {
    if d.var_type == VarType::Int {
        emit_expr_as_int(&a.value, ctx)
    } else if let Some(start) = argv_start_index(&a.value, ctx) {
        format!("(array/slice (dyn :args) (min {start} (length (dyn :args))))")
    } else {
        emit_expr_inline(&a.value, ctx)
    }
}

fn declare_decl_var(ctx: &mut EmitCtx, d: &Declaration, a: &Assignment) {
    if d.var_type == VarType::Int {
        ctx.declare_int(&a.name);
    } else if matches!(a.value.kind, ExprKind::AssocArray(_)) {
        ctx.declare_map(&a.name);
    } else {
        ctx.declare(&a.name);
    }
}

fn emit_for(out: &mut String, ctx: &mut EmitCtx, f: &For, depth: usize) {
    indent(out, depth);
    let mut words: Vec<String> = Vec::new();
    for w in &f.words {
        // DD#60: `for x in $(argv N)` iterates the args slice (index N..),
        // clamped so N >= argc yields zero iterations (not a slice error).
        if let Some(start) = argv_start_index(w, ctx) {
            words.push(format!(
                ";(array/slice (dyn :args) (min {start} (length (dyn :args))))"
            ));
        } else if let Some(array) = janet_arg_splat(&w.kind) {
            // `for x in @a` — splice the array's elements, don't iterate the
            // array as a single value.
            words.push(format!(";{}", array));
        } else {
            words.push(emit_expr_inline(w, ctx));
        }
    }
    ctx.declare(&f.variable);
    writeln!(out, "(each {} [{}]", f.variable, words.join(" ")).unwrap();
    for s in &f.body {
        emit_stmt(out, ctx, s, depth + 1);
    }
    indent(out, depth);
    writeln!(out, ")").unwrap();
}

fn emit_case(out: &mut String, ctx: &mut EmitCtx, c: &Case, depth: usize) {
    indent(out, depth);
    let val = emit_expr_inline(&c.value, ctx);
    writeln!(out, "(match {}", val).unwrap();
    for item in &c.items {
        emit_case_item(out, ctx, item, depth);
    }
    indent(out, depth);
    writeln!(out, ")").unwrap();
}

fn emit_case_item(out: &mut String, ctx: &mut EmitCtx, item: &crate::ir::CaseItem, depth: usize) {
    indent(out, depth + 1);
    let pats: Vec<String> = item
        .patterns
        .iter()
        .map(|p| emit_expr_inline(p, ctx))
        .collect();
    if pats.iter().any(|p| p == "\"*\"") {
        // a `*` alternative makes the whole arm match anything.
        write!(out, "_ ").unwrap();
    } else if pats.len() == 1 {
        write!(out, "{} ", pats[0]).unwrap();
    } else {
        // Multi-pattern arm `a|b|…` → a janet `match` guard alternation
        // binding a fresh var (was incorrectly emitted as `_`, a catch-all
        // that matched every subject).
        let alts: Vec<String> = pats.iter().map(|p| format!("(= _casev {p})")).collect();
        write!(out, "(_casev (or {})) ", alts.join(" ")).unwrap();
    }
    writeln!(out, "(do").unwrap();
    for s in &item.body {
        emit_stmt(out, ctx, s, depth + 2);
    }
    indent(out, depth + 1);
    writeln!(out, ")").unwrap();
}

fn emit_function_def(out: &mut String, ctx: &mut EmitCtx, f: &Function, depth: usize) {
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

fn emit_list(out: &mut String, ctx: &mut EmitCtx, l: &List, depth: usize) {
    match l.op {
        crate::ir::ListOp::And => emit_list_and(out, ctx, l, depth),
        crate::ir::ListOp::Or => emit_list_or(out, ctx, l, depth),
        crate::ir::ListOp::Background => emit_list_background(out, ctx, l, depth),
    }
}

fn emit_list_and(out: &mut String, ctx: &mut EmitCtx, l: &List, depth: usize) {
    emit_list_chain(out, ctx, l, depth, "(= 0 __last_exit)");
}

fn emit_list_or(out: &mut String, ctx: &mut EmitCtx, l: &List, depth: usize) {
    emit_list_chain(out, ctx, l, depth, "(not= 0 __last_exit)");
}

/// `a && b` / `a || b` run the LEFT for effect, then run the RIGHT based on
/// the LEFT's exit STATUS (`__last_exit`), not its janet value. `&&` runs the
/// right iff the left succeeded (status 0); `||` iff it failed. This requires
/// every command to leave `__last_exit` set (see `janet_cmd_defaults_success`
/// and the per-command emitters). The chain's resulting `__last_exit` is the
/// last command actually run — matching bash.
fn emit_list_chain(out: &mut String, ctx: &mut EmitCtx, l: &List, depth: usize, cond: &str) {
    let outer = ctx.checked;
    indent(out, depth);
    writeln!(out, "(do").unwrap();
    // The left operand's failure is caught by the chain — it never aborts.
    ctx.checked = false;
    emit_stmt(out, ctx, &l.left, depth + 1);
    indent(out, depth + 1);
    writeln!(out, "(when {cond}").unwrap();
    // The right runs as the chain's effective result — it inherits the
    // ambient checked (aborts iff the whole chain is in a checked position).
    ctx.checked = outer;
    emit_stmt(out, ctx, &l.right, depth + 2);
    indent(out, depth + 1);
    writeln!(out, "))").unwrap();
    ctx.checked = outer;
}

fn emit_list_background(out: &mut String, ctx: &mut EmitCtx, l: &List, depth: usize) {
    indent(out, depth);
    writeln!(out, "(ev/spawn (fn []").unwrap();
    emit_stmt(out, ctx, &l.left, depth + 1);
    indent(out, depth);
    writeln!(out, "))").unwrap();
}

fn emit_group(out: &mut String, ctx: &mut EmitCtx, g: &Group, depth: usize) {
    indent(out, depth);
    writeln!(out, "(do").unwrap();
    for st in &g.body {
        emit_stmt(out, ctx, st, depth + 1);
    }
    indent(out, depth);
    writeln!(out, ")").unwrap();
}

fn emit_comment(out: &mut String, text: &str, depth: usize) {
    indent(out, depth);
    if text.starts_with("#!") {
        // Skip shebang
    } else {
        writeln!(out, "{}", text).unwrap();
    }
}

fn emit_suppressed(out: &mut String, ctx: &mut EmitCtx, inner: &Statement, depth: usize) {
    indent(out, depth);
    writeln!(out, "(do").unwrap();
    // `-` suppresses the result-verdict: the inner command must not abort.
    let outer = ctx.checked;
    ctx.checked = false;
    emit_stmt(out, ctx, inner, depth + 1);
    ctx.checked = outer;
    indent(out, depth);
    writeln!(out, "  nil)").unwrap();
}

fn emit_arith_test(out: &mut String, expr: &ArithExpr, depth: usize) {
    indent(out, depth);
    writeln!(out, "(not= 0 {})", arith_expr_to_janet(expr)).unwrap();
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
    if let Some(c) = match_while_read(w) {
        emit_while_read_loop(out, ctx, c, &w.body, depth);
    } else {
        emit_generic_while(out, ctx, w, depth);
    }
}

/// Detect `while read VAR; do ... done` (not `until`, single-cmd
/// condition). Returns the read Command when matched.
fn match_while_read(w: &While) -> Option<&Command> {
    if w.until || w.condition.len() != 1 {
        return None;
    }
    let StmtKind::Command(c) = &w.condition[0].kind else {
        return None;
    };
    if expr_to_literal(&c.name) == "read" {
        Some(c)
    } else {
        None
    }
}

/// Janet idiom: `(while (def _raw (file/read stdin :line)) (var v ...) body)`.
fn emit_while_read_loop(
    out: &mut String,
    ctx: &mut EmitCtx,
    c: &Command,
    body: &[Statement],
    depth: usize,
) {
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
    for s in body {
        emit_stmt(out, ctx, s, depth + 1);
    }
    indent(out, depth);
    writeln!(out, ")").unwrap();
}

fn emit_generic_while(out: &mut String, ctx: &mut EmitCtx, w: &While, depth: usize) {
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

fn emit_pipeline(out: &mut String, ctx: &mut EmitCtx, p: &Pipeline, depth: usize) {
    if p.commands.len() == 1 {
        emit_stmt(out, ctx, &p.commands[0], depth);
        return;
    }
    let last = p.commands.len() - 1;
    indent(out, depth);
    writeln!(out, "(do").unwrap();
    for (i, cmd) in p.commands.iter().enumerate() {
        emit_pipeline_stage(out, ctx, cmd, i, last, depth + 1);
    }
    emit_pipeline_close_pipes(out, last, depth + 1);
    emit_pipeline_wait_and_collect(out, p.commands.len(), depth + 1);
}

/// Emit one `os/spawn` stage. Stages are connected by explicit
/// `(os/pipe :WR)` pipes whose ends are BLOCKING — janet's default
/// `:out :pipe` streams are non-blocking (for the ev loop), and handing a
/// non-blocking fd to a child's stdin/stdout makes the child read/write
/// EAGAIN ("Resource temporarily unavailable"). The last stage keeps
/// `:out :pipe` because janet itself drains it via the ev loop, where
/// non-blocking is correct.
fn emit_pipeline_stage(
    out: &mut String,
    ctx: &mut EmitCtx,
    cmd: &Statement,
    i: usize,
    last: usize,
    depth: usize,
) {
    let StmtKind::Command(c) = &cmd.kind else {
        return;
    };
    let cmd_name = expr_to_literal(&c.name);
    let parts = janet_command_parts(&cmd_name, c, ctx);
    if i < last {
        indent(out, depth);
        writeln!(out, "(def [_r{i} _w{i}] (os/pipe :WR))").unwrap();
    }
    let in_spec = if i > 0 {
        format!(":in _r{} ", i - 1)
    } else {
        String::new()
    };
    let out_spec = if i < last {
        format!(":out _w{i}")
    } else {
        ":out :pipe".to_string()
    };
    indent(out, depth);
    writeln!(
        out,
        "(def p{} (os/spawn [{}] :p {{{}{}}}))",
        i,
        parts.join(" "),
        in_spec,
        out_spec
    )
    .unwrap();
}

/// The parent must close its copies of every inter-stage pipe end so that
/// EOF propagates down the chain (otherwise a downstream reader never sees
/// end-of-input). There are `last` inter-stage pipes (indices 0..last-1).
fn emit_pipeline_close_pipes(out: &mut String, last: usize, depth: usize) {
    if last == 0 {
        return;
    }
    indent(out, depth);
    for i in 0..last {
        write!(out, "(:close _w{i}) (:close _r{i}) ").unwrap();
    }
    writeln!(out).unwrap();
}

/// Drain the last stage's stdout to EOF (which pulls data through the whole
/// blocking chain), then reap every stage, then print. Reading before
/// waiting avoids any pipe-buffer deadlock.
fn emit_pipeline_wait_and_collect(out: &mut String, n_commands: usize, depth: usize) {
    let last = n_commands - 1;
    indent(out, depth);
    writeln!(
        out,
        "(def result (string/trim (:read (p{} :out) :all)))",
        last
    )
    .unwrap();
    for i in 0..n_commands {
        indent(out, depth);
        // A pipeline's exit status is the last stage's (bash, no pipefail).
        if i == last {
            writeln!(out, "(set __last_exit (os/proc-wait p{}))", i).unwrap();
        } else {
            writeln!(out, "(os/proc-wait p{})", i).unwrap();
        }
    }
    indent(out, depth);
    writeln!(out, "(print result))").unwrap();
}

fn emit_command(out: &mut String, ctx: &mut EmitCtx, c: &Command, _stmt: &Statement, depth: usize) {
    let cmd_name = expr_to_literal(&c.name);
    let kind = BuiltinCmd::from_name(&cmd_name);
    let defaults_success = janet_cmd_defaults_success(&kind, &cmd_name);
    indent(out, depth);
    match kind {
        BuiltinCmd::Echo => emit_janet_echo(out, ctx, c),
        BuiltinCmd::True => emit_janet_const_bool(out, true),
        BuiltinCmd::False => emit_janet_const_bool(out, false),
        // `:` no-op — succeeds silently (the canonical `|| :` discard).
        BuiltinCmd::Noop => emit_janet_const_bool(out, true),
        BuiltinCmd::Exit => emit_janet_exit(out, ctx, c),
        BuiltinCmd::Shift => {
            writeln!(out, "(set _args (array/slice _args 1))").unwrap();
        }
        BuiltinCmd::Test => emit_janet_test_cmd(out, ctx, c),
        BuiltinCmd::Match => emit_janet_match(out, ctx, c),
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
    // DD#55/&&-||: every command must leave `__last_exit` set. The
    // success-builtins get an unconditional 0 here; true/false/`:`, test,
    // match and general commands set their own.
    if defaults_success {
        indent(out, depth);
        writeln!(out, "(set __last_exit 0)").unwrap();
    }
}

/// Builtins whose exit status is unconditionally 0. `true`/`false`/`:` and
/// `match`/`test` set their own status; general/external commands set it from
/// the child exit code; control-flow (exit/return/break/continue) transfers
/// control. `cat` (stdin passthrough) succeeds.
fn janet_cmd_defaults_success(kind: &BuiltinCmd, cmd_name: &str) -> bool {
    if cmd_name == "cat" {
        return true;
    }
    matches!(
        kind,
        BuiltinCmd::Echo
            | BuiltinCmd::Printf
            | BuiltinCmd::Read
            | BuiltinCmd::Unset
            | BuiltinCmd::Shift
            | BuiltinCmd::Bless
    )
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
        // Statement-position `[[ … ]]` (e.g. LHS of `&&`/`||`): set the exit
        // status (0 true / 1 false) so the chain branches on it.
        writeln!(
            out,
            "(set __last_exit (if {} 0 1))",
            emit_test_expr(arg, ctx)
        )
        .unwrap();
    }
}

/// DD#63: `match S P` boolean command (statement position — e.g. the LHS
/// of `&&`/`||`). Shells out to the quiet `sash-regex matchq` (exit 0
/// matched / 1 not, no stdout), sets `__last_exit`, and yields the
/// boolean so `(when (do …))` consumes it.
fn emit_janet_match(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    if c.args.len() == 3 {
        // DD#63 out-array form `match S P ARR`.
        let s = emit_expr_inline(&c.args[0], ctx);
        let p = emit_expr_inline(&c.args[1], ctx);
        let arr = expr_to_literal(&c.args[2]);
        writeln!(out, "{}", emit_janet_match_into_expr(&s, &p, &arr)).unwrap();
        return;
    }
    if c.args.len() != 2 {
        writeln!(out, "false").unwrap();
        return;
    }
    let s = emit_expr_inline(&c.args[0], ctx);
    let p = emit_expr_inline(&c.args[1], ctx);
    writeln!(
        out,
        "(let [_m (zero? (os/execute [\"sash-regex\" \"matchq\" {} {}] :p))] (set __last_exit (if _m 0 1)) _m)",
        s, p
    )
    .unwrap();
}

/// DD#63: `match S P ARR` out-array form. Shells to `sash-regex captures`
/// (NUL-separated `[whole, g1..gN]`, bash-parity group count), splits the
/// output into the destination array, sets `__last_exit`, and yields the
/// boolean (matched) so a condition / `&&` / `||` consumes it. Mirrors the
/// native `sash_regex_match_into`: clear-on-entry (a no-match leaves the
/// array empty) and a clean array (the subject is always clean).
fn emit_janet_match_into_expr(s: &str, p: &str, arr: &str) -> String {
    format!(
        "(let [_f (os/spawn [\"sash-regex\" \"captures\" {s} {p}] :p {{:out :pipe}}) \
         _raw (or (:read (_f :out) :all) \"\") _rc (os/proc-wait _f) _m (zero? _rc)] \
         (set {arr} (if _m (string/split (string/from-bytes 0) _raw) @[])) \
         (set __last_exit (if _m 0 1)) _m)"
    )
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
        // A user function sets `__last_exit` internally (its last command);
        // the call leaves that in place, matching bash `$?` after a function.
        writeln!(out, "({}{})", cmd_name, args_str).unwrap();
    } else {
        // External command: `os/execute` returns the child exit code. In a
        // checked position (bare statement — not `-`-suppressed, chain left,
        // or condition) a nonzero exit aborts with that status: the DD#55 (A)
        // channel (no diagnostic, matching native's msg=None external abort).
        let parts = janet_command_parts(cmd_name, c, ctx);
        let joined = parts.join(" ");
        if ctx.checked {
            writeln!(
                out,
                "(let [_rc (os/execute [{joined}] :p)] (set __last_exit _rc) (when (not= 0 _rc) (os/exit _rc)))"
            )
            .unwrap();
        } else {
            writeln!(out, "(set __last_exit (os/execute [{joined}] :p))").unwrap();
        }
    }
}

/// Build the Janet command tuple parts `["cmd" arg…]`, expanding any
/// `@a` splat to a `;` splice (DD#20/DD#22 — one element per value).
fn janet_command_parts(cmd_name: &str, c: &Command, ctx: &mut EmitCtx) -> Vec<String> {
    let mut parts = vec![format!("\"{}\"", cmd_name)];
    for arg in &c.args {
        match janet_arg_splat(&arg.kind) {
            Some(array) => parts.push(format!(";{}", array)),
            None => parts.push(emit_expr_inline(arg, ctx)),
        }
    }
    parts
}

/// Recognise `@a` / single-part `"${a[@]}"` (ParameterExpansion
/// `arrvals`) as an array splat, returning the array name.
fn janet_arg_splat(kind: &ExprKind) -> Option<String> {
    match kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            Some(pe.name.clone())
        }
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => janet_arg_splat(&parts[0].kind),
        _ => None,
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
    let args: Vec<String> = c
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            // The format string (arg 0) has printf backslash escapes
            // interpreted; the rest are ordinary values.
            if i == 0 {
                if let Some(fmt) = printf_format_literal(a) {
                    return format!(
                        "\"{}\"",
                        escape_for_janet_string(&interpret_printf_escapes(&fmt))
                    );
                }
            }
            emit_expr_inline(a, ctx)
        })
        .collect();
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
    if has_herestring_redirect(&r.redirects) {
        let body_text = collect_herestring_body_text(&r.redirects);
        emit_redirected_herestring(out, ctx, &r.body.kind, &body_text, depth);
    } else {
        emit_redirected_no_herestring(out, ctx, r, depth);
    }
}

fn has_herestring_redirect(redirects: &[Redirect]) -> bool {
    redirects
        .iter()
        .any(|rd| matches!(rd.op, RedirectOp::HereString))
}

fn collect_herestring_body_text(redirects: &[Redirect]) -> String {
    redirects
        .iter()
        .filter(|rd| matches!(rd.op, RedirectOp::HereString))
        .map(|rd| expr_to_literal(&rd.target))
        .collect::<Vec<_>>()
        .join("")
}

/// Escape a string for inclusion as a Janet string literal:
/// backslash → \\, double-quote → \", newline → \n.
fn escape_for_janet_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// `printf` interprets C-style backslash escapes in its format string.
/// Apply them here (so `\n`→newline etc.) before the result is escaped for
/// Janet string syntax — Janet's `string/format` does not interpret them.
fn interpret_printf_escapes(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('a') => out.push('\x07'),
            Some('b') => out.push('\x08'),
            Some('f') => out.push('\x0c'),
            Some('v') => out.push('\x0b'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Literal text of a `printf` format argument, if it is a plain literal.
fn printf_format_literal(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => Some(s.clone()),
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => printf_format_literal(&parts[0]),
        _ => None,
    }
}

fn is_cat_no_args(kind: &StmtKind) -> bool {
    matches!(kind, StmtKind::Command(c) if expr_to_literal(&c.name) == "cat" && c.args.is_empty())
}

fn emit_redirected_herestring(
    out: &mut String,
    ctx: &mut EmitCtx,
    body_kind: &StmtKind,
    body_text: &str,
    depth: usize,
) {
    if is_cat_no_args(body_kind) {
        emit_redirected_cat_herestring(out, body_text, depth);
        return;
    }
    if let StmtKind::Command(c) = body_kind {
        if expr_to_literal(&c.name) == "read" {
            emit_redirected_herestring_read(out, ctx, body_text, c, depth);
            return;
        }
    }
    emit_redirected_generic_herestring(out, ctx, body_kind, body_text, depth);
}

/// `cat <<< "string"` — herestring-as-string pattern; just print body.
fn emit_redirected_cat_herestring(out: &mut String, body_text: &str, depth: usize) {
    indent(out, depth);
    let escaped = escape_for_janet_string(body_text.trim_end_matches('\n'));
    writeln!(out, "(print \"{}\")", escaped).unwrap();
}

/// `read <<< "string"` — herestring assigns trimmed body to var (default REPLY).
fn emit_redirected_herestring_read(
    out: &mut String,
    ctx: &mut EmitCtx,
    body_text: &str,
    c: &Command,
    depth: usize,
) {
    let var_name = c
        .args
        .iter()
        .map(expr_to_literal)
        .find(|a| !a.starts_with('-'))
        .unwrap_or_else(|| "REPLY".to_string());
    let val = body_text.trim_end_matches('\n');
    indent(out, depth);
    if !ctx.is_declared(&var_name) {
        ctx.declare(&var_name);
        writeln!(out, "(var {} \"{}\")", var_name, val).unwrap();
    } else {
        writeln!(out, "(set {} \"{}\")", var_name, val).unwrap();
    }
}

/// Generic command with herestring — spawn the command and pipe the body
/// to its stdin.
fn emit_redirected_generic_herestring(
    out: &mut String,
    ctx: &mut EmitCtx,
    body_kind: &StmtKind,
    body_text: &str,
    depth: usize,
) {
    indent(out, depth);
    let escaped = escape_for_janet_string(body_text);
    writeln!(out, "(do").unwrap();
    indent(out, depth + 1);
    writeln!(out, "(def _herestring_body \"{}\")", escaped).unwrap();
    let (cmd_name, args) = extract_command_name_and_args(body_kind, ctx);
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
    writeln!(out, "(:write (p :in) _herestring_body)").unwrap();
    indent(out, depth + 1);
    writeln!(out, "(:close (p :in))").unwrap();
    indent(out, depth + 1);
    writeln!(out, "(os/proc-wait p)").unwrap();
    indent(out, depth);
    writeln!(out, ")").unwrap();
}

fn extract_command_name_and_args(kind: &StmtKind, ctx: &EmitCtx) -> (String, Vec<String>) {
    match kind {
        StmtKind::Command(c) => (
            expr_to_literal(&c.name),
            c.args.iter().map(|a| emit_expr_inline(a, ctx)).collect(),
        ),
        _ => ("?".to_string(), vec![]),
    }
}

fn emit_redirected_no_herestring(
    out: &mut String,
    ctx: &mut EmitCtx,
    r: &Redirected,
    depth: usize,
) {
    for rd in &r.redirects {
        indent(out, depth);
        writeln!(out, "# redirect: fd={} op={:?}", rd.fd.unwrap_or(1), rd.op).unwrap();
    }
    emit_stmt(out, ctx, &r.body, depth);
}

/// Emit a condition (test_command) inline for if/while.
fn emit_condition_inline(out: &mut String, ctx: &EmitCtx, stmt: &Statement) {
    match &stmt.kind {
        StmtKind::Command(c) => emit_command_condition(out, ctx, c),
        // `(( … ))` condition: true iff the arith value is non-zero. Arith
        // comparisons/logicals now yield 1/0 integers (arith_binop_to_janet),
        // so this is correct for both comparison and value forms.
        StmtKind::ArithTest(a) => write!(out, "(not= 0 {})", arith_expr_to_janet(a)).unwrap(),
        _ => write!(out, "true").unwrap(),
    }
}

/// A command used as an if/while condition: `[[ … ]]` lowers to the test
/// expression; any other command is true iff it exits 0.
fn emit_command_condition(out: &mut String, ctx: &EmitCtx, c: &Command) {
    let name = expr_to_literal(&c.name);
    if name == "[[" {
        match c.args.first() {
            Some(arg) => write!(out, "{}", emit_test_expr(arg, ctx)).unwrap(),
            None => write!(out, "true").unwrap(),
        }
        return;
    }
    // DD#63: `match S P` boolean command → the `sash-regex` helper. Use
    // the quiet `matchq` op (exit status only) so the matched text does
    // NOT leak to stdout; `(zero? …)` is the condition truth.
    if name == "match" && c.args.len() == 2 {
        let s = emit_expr_inline(&c.args[0], ctx);
        let p = emit_expr_inline(&c.args[1], ctx);
        write!(
            out,
            "(zero? (os/execute [\"sash-regex\" \"matchq\" {} {}] :p))",
            s, p
        )
        .unwrap();
        return;
    }
    // DD#63 out-array form `match S P ARR` in a condition: populate the
    // array and yield the boolean.
    if name == "match" && c.args.len() == 3 {
        let s = emit_expr_inline(&c.args[0], ctx);
        let p = emit_expr_inline(&c.args[1], ctx);
        let arr = expr_to_literal(&c.args[2]);
        write!(out, "{}", emit_janet_match_into_expr(&s, &p, &arr)).unwrap();
        return;
    }
    let mut parts = vec![format!("\"{}\"", name)];
    for arg in &c.args {
        parts.push(emit_expr_inline(arg, ctx));
    }
    write!(out, "(zero? (os/execute [{}] :p))", parts.join(" ")).unwrap();
}

/// Check if an expression is known to be integer-typed.
fn is_int_expr(expr: &Expr, ctx: &EmitCtx) -> bool {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => ctx.is_int(var),
        ExprKind::Literal(s) => s.chars().all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// Emit a test expression as a Janet boolean expression.
fn emit_test_expr(expr: &Expr, ctx: &EmitCtx) -> String {
    let ExprKind::Test(test) = &expr.kind else {
        return "true".to_string();
    };
    match test {
        TestExpr::Unary { op, operand } => emit_unary_test(op, operand, ctx),
        TestExpr::Binary { left, op, right } => emit_binary_test(left, op, right, ctx),
        TestExpr::Not(inner) => format!("(not {})", emit_inner_test(inner, expr.span, ctx)),
        TestExpr::And(l, r) => format!(
            "(and {} {})",
            emit_inner_test(l, expr.span, ctx),
            emit_inner_test(r, expr.span, ctx)
        ),
        TestExpr::Or(l, r) => format!(
            "(or {} {})",
            emit_inner_test(l, expr.span, ctx),
            emit_inner_test(r, expr.span, ctx)
        ),
    }
}

/// Wrap a TestExpr child into a synthetic Expr::Test and recurse —
/// shared between Not/And/Or/Paren.
fn emit_inner_test(inner: &TestExpr, span: Span, ctx: &EmitCtx) -> String {
    let wrapped = Expr {
        kind: ExprKind::Test(inner.clone()),
        span,
    };
    emit_test_expr(&wrapped, ctx)
}

fn emit_unary_test(op: &str, operand: &Expr, ctx: &EmitCtx) -> String {
    let val = emit_expr_inline(operand, ctx);
    match op {
        "-f" => format!("(= (os/stat {} :mode) :file)", val),
        "-d" => format!("(= (os/stat {} :mode) :directory)", val),
        "-e" => format!("(not (nil? (os/stat {} :mode)))", val),
        "-n" => format!("(not (or (nil? {0}) (empty? {0})))", val),
        "-z" => format!("(or (nil? {0}) (empty? {0}))", val),
        _ => format!("(truthy? {} {})", op, val),
    }
}

fn emit_binary_test(left: &Expr, op: &str, right: &Expr, ctx: &EmitCtx) -> String {
    let (l, r) = if is_numeric_test_op(op) {
        (emit_expr_as_int(left, ctx), emit_expr_as_int(right, ctx))
    } else {
        (
            emit_test_string_operand(left, ctx),
            emit_test_string_operand(right, ctx),
        )
    };
    match op {
        "=" | "==" | "-eq" => format!("(= {} {})", l, r),
        "!=" | "-ne" => format!("(not= {} {})", l, r),
        "-gt" => format!("(> {} {})", l, r),
        "-lt" => format!("(< {} {})", l, r),
        "-ge" => format!("(>= {} {})", l, r),
        "-le" => format!("(<= {} {})", l, r),
        _ => format!("({} {} {})", op, l, r),
    }
}

/// A string-comparison operand. An int-typed value emits as a bare janet
/// number, so stringify it to compare like bash's all-strings model — e.g.
/// `[[ $i != 0 ]]` with an int `i` must compare "0" to "0", not the janet
/// integer 0 to the string "0" (which is always unequal → infinite loops).
fn emit_test_string_operand(expr: &Expr, ctx: &EmitCtx) -> String {
    let s = emit_expr_inline(expr, ctx);
    if is_int_expr(expr, ctx) {
        format!("(string {})", s)
    } else {
        s
    }
}

fn is_numeric_test_op(op: &str) -> bool {
    matches!(op, "-eq" | "-ne" | "-gt" | "-lt" | "-ge" | "-le")
}

/// Emit an expression as a Janet integer value.
/// Strips quotes from literals, converts string vars via scan-number.
fn emit_expr_as_int(expr: &Expr, ctx: &EmitCtx) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => int_literal_to_janet(s),
        ExprKind::SimpleExpansion(var) => int_expansion_to_janet(var, expr, ctx),
        ExprKind::Arithmetic(a) => arith_expr_to_janet(a),
        _ => format!("(scan-number {})", emit_expr_inline(expr, ctx)),
    }
}

/// A bare numeric literal stays unquoted; anything else is parsed at runtime.
fn int_literal_to_janet(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_digit() || c == '-') {
        s.to_string()
    } else {
        format!("(scan-number \"{}\")", s)
    }
}

/// Read a `$var`/`$#` as an integer: `$#` is the arg count, an int-typed
/// binding is already numeric, otherwise parse its string value.
fn int_expansion_to_janet(var: &str, expr: &Expr, ctx: &EmitCtx) -> String {
    if var == "#" {
        "(length _args)".to_string()
    } else if ctx.is_int(var) {
        var.to_string()
    } else {
        format!("(scan-number {})", emit_expr_inline(expr, ctx))
    }
}

/// Emit an expression as an inline Janet value.
fn emit_expr_inline(expr: &Expr, ctx: &EmitCtx) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => format!("\"{}\"", escape_for_janet_string(s)),
        ExprKind::SimpleExpansion(var) => emit_expr_simple_expansion(var, ctx),
        ExprKind::SingleQuoted(s) => format!("\"{}\"", escape_for_janet_string(s)),
        ExprKind::DoubleQuoted(parts) => emit_expr_double_quoted(parts, ctx),
        ExprKind::ParameterExpansion(pe) => emit_param_expansion_inline(pe, ctx),
        ExprKind::CommandSubstitution(stmts) => emit_expr_command_substitution(stmts, ctx),
        ExprKind::Arithmetic(a) => format!("(string {})", arith_expr_to_janet(a)),
        ExprKind::Concat(parts) => emit_expr_concat(parts, ctx),
        ExprKind::Array(elems) => emit_expr_array(elems, ctx),
        ExprKind::AssocArray(pairs) => emit_expr_assoc_array(pairs, ctx),
        ExprKind::Subscript { name, index } => emit_expr_subscript(name, index, ctx),
        ExprKind::Test(test) => emit_expr_test(test, expr.span, ctx),
    }
}

fn emit_expr_simple_expansion(var: &str, ctx: &EmitCtx) -> String {
    if ctx.is_declared(var) {
        var.to_string()
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

fn emit_expr_double_quoted(parts: &[Expr], ctx: &EmitCtx) -> String {
    if parts.is_empty() {
        "\"\"".to_string()
    } else if parts.len() == 1 {
        emit_expr_inline(&parts[0], ctx)
    } else {
        let ps: Vec<String> = parts.iter().map(|p| emit_expr_inline(p, ctx)).collect();
        format!("(string {})", ps.join(" "))
    }
}

fn emit_expr_command_substitution(stmts: &[Statement], ctx: &EmitCtx) -> String {
    if let Some(stmt) = stmts.first() {
        let inner = match &stmt.kind {
            StmtKind::Redirected(r) => &r.body.kind,
            other => other,
        };
        if let StmtKind::Command(c) = inner {
            return emit_expr_cmdsub_command(c, ctx);
        }
    }
    "\"\"".to_string()
}

/// DD#60: recognise `$(argv)` / `$(argv N)` (an argv splat, not a scalar
/// cmdsub). Returns the janet start-index expression (0 when N is omitted),
/// or None if `expr` is not an argv splat.
fn argv_start_index(expr: &Expr, ctx: &EmitCtx) -> Option<String> {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return None;
    };
    let stmt = stmts.first()?;
    let inner = match &stmt.kind {
        StmtKind::Redirected(r) => &r.body.kind,
        other => other,
    };
    let StmtKind::Command(c) = inner else {
        return None;
    };
    if expr_to_literal(&c.name) != "argv" {
        return None;
    }
    Some(match c.args.first() {
        Some(a) => emit_expr_as_int(a, ctx),
        None => "0".to_string(),
    })
}

fn emit_expr_cmdsub_command(c: &Command, ctx: &EmitCtx) -> String {
    let name = expr_to_literal(&c.name);
    if let Some(s) = cmdsub_regex_intrinsic(&name, c, ctx) {
        return s;
    }
    if let Some(s) = cmdsub_data_intrinsic(&name, c, ctx) {
        return s;
    }
    // Generic fallback: shell out to the named command and capture stdout.
    let mut parts = vec![format!("\"{}\"", name)];
    for arg in &c.args {
        parts.push(emit_expr_inline(arg, ctx));
    }
    format!(
        "(string/trim (with [f (os/spawn [{}] :p {{:out :pipe}})] (:read (f :out) :all)))",
        parts.join(" ")
    )
}

/// DD#63 regex value intrinsics. `sub`/`gsub`/`match` shell out to the
/// `sash-regex` helper (same vendored Plan 9 regexp9 engine as native/C),
/// so the Janet oracle is byte-identical rather than faking regex with
/// Janet's literal string ops. `sub`/`gsub` take SUBJECT PATTERN REPLACEMENT;
/// `$(match S P)` yields the whole match (resub[0]) on stdout.
fn cmdsub_regex_intrinsic(name: &str, c: &Command, ctx: &EmitCtx) -> Option<String> {
    if (name == "sub" || name == "gsub") && c.args.len() == 3 {
        let subj = emit_expr_inline(&c.args[0], ctx);
        let pat = emit_expr_inline(&c.args[1], ctx);
        let rep = emit_expr_inline(&c.args[2], ctx);
        return Some(format!(
            "(string/trim (with [f (os/spawn [\"sash-regex\" \"{name}\" {subj} {pat} {rep}] :p {{:out :pipe}})] (or (:read (f :out) :all) \"\")))"
        ));
    }
    if name == "match" && c.args.len() == 2 {
        let subj = emit_expr_inline(&c.args[0], ctx);
        let pat = emit_expr_inline(&c.args[1], ctx);
        return Some(format!(
            "(string/trim (with [f (os/spawn [\"sash-regex\" \"match\" {subj} {pat}] :p {{:out :pipe}})] (or (:read (f :out) :all) \"\")))"
        ));
    }
    None
}

/// Data/collection value intrinsics that must NOT fall through to the
/// generic exec fallback: `len` (array length), the DD#60 argv trio
/// (`argc`/`arg`), and DD#41 `join`.
fn cmdsub_data_intrinsic(name: &str, c: &Command, ctx: &EmitCtx) -> Option<String> {
    // `$(len ARR)` → element count of the named array.
    if name == "len" && c.args.len() == 1 {
        let arr = expr_to_literal(&c.args[0]);
        return Some(format!("(string (length {}))", arr));
    }
    // DD#60: `$(argc)` is C argc (program-name slot + user args); the emitted
    // script runs as `janet .out.janet ARGS…`, so `(dyn :args)` = [script, …].
    if name == "argc" {
        return Some("(string (length (dyn :args)))".to_string());
    }
    // `$(arg N)` — 0-based scalar program arg (index 0 = program name);
    // out-of-range aborts at runtime (negative literal is a validator error).
    if name == "arg" && c.args.len() == 1 {
        let idx = emit_expr_as_int(&c.args[0], ctx);
        return Some(format!(
            "(let [_ai {idx}] (if (and (>= _ai 0) (< _ai (length (dyn :args)))) \
             (get (dyn :args) _ai) \
             (do (eprint (get (dyn :args) 0) \": arg: index out of range\") (os/exit 1))))"
        ));
    }
    cmdsub_join_intrinsic(name, c, ctx)
}

/// DD#41: `join @arr` → space-separated; `join SEP @arr` → SEP-separated.
fn cmdsub_join_intrinsic(name: &str, c: &Command, ctx: &EmitCtx) -> Option<String> {
    if name != "join" {
        return None;
    }
    match c.args.len() {
        1 => janet_arg_splat(&c.args[0].kind).map(|a| format!("(string/join {a} \" \")")),
        2 => janet_arg_splat(&c.args[1].kind).map(|array| {
            let sep = emit_expr_inline(&c.args[0], ctx);
            format!("(string/join {array} {sep})")
        }),
        _ => None,
    }
}

fn emit_expr_concat(parts: &[Expr], ctx: &EmitCtx) -> String {
    let ps: Vec<String> = parts.iter().map(|p| emit_expr_inline(p, ctx)).collect();
    format!("(string {})", ps.join(" "))
}

fn emit_expr_array(elems: &[Expr], ctx: &EmitCtx) -> String {
    let es: Vec<String> = elems.iter().map(|e| emit_expr_inline(e, ctx)).collect();
    format!("@[{}]", es.join(" "))
}

fn emit_expr_assoc_array(pairs: &[(String, Expr)], ctx: &EmitCtx) -> String {
    let es: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{:?} {}", k, emit_expr_inline(v, ctx)))
        .collect();
    format!("@{{{}}}", es.join(" "))
}

fn emit_expr_subscript(name: &str, index: &Expr, ctx: &EmitCtx) -> String {
    // An indexed array needs an integer index; a map needs its string key.
    // `${a[$i]}` reads $i as a string var, so coerce with emit_expr_as_int
    // (janet `get` on an array with a string key returns nil → empty).
    if ctx.is_map(name) {
        format!("(get {} {})", name, emit_expr_inline(index, ctx))
    } else {
        format!("(get {} {})", name, emit_expr_as_int(index, ctx))
    }
}

fn emit_expr_test(test: &TestExpr, span: Span, ctx: &EmitCtx) -> String {
    let wrapper = Expr {
        kind: ExprKind::Test(test.clone()),
        span,
    };
    emit_test_expr(&wrapper, ctx)
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

/// Lower an arithmetic `$name` to a numeric Janet expression. `$#` and
/// `$N` read the function-local `_args`; `$?` reads `__last_exit`; a plain
/// name is an in-scope integer Janet binding.
fn arith_dollar_var_to_janet(name: &str) -> String {
    if name == "#" {
        "(length _args)".to_string()
    } else if name == "?" {
        "__last_exit".to_string()
    } else if name != "0" && !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
        let idx = name.parse::<u32>().unwrap_or(1).saturating_sub(1);
        format!("(scan-number (or (get _args {}) \"0\"))", idx)
    } else {
        name.to_string()
    }
}

/// Emit a janet expression for a binary arithmetic op. bash arithmetic
/// comparisons and logicals yield 1/0 **integers** (and bash treats 0 as
/// false), whereas janet comparisons yield booleans and janet treats 0 as
/// truthy — so wrap comparisons/logicals to produce a 1/0 integer.
fn arith_binop_to_janet(op: crate::ir::IrArithOp, lhs: String, rhs: String) -> String {
    use crate::ir::IrArithOp::*;
    match op {
        Eq | Ne | Lt | Le | Gt | Ge => {
            let j = match op {
                Eq => "=",
                Ne => "not=",
                Lt => "<",
                Le => "<=",
                Gt => ">",
                _ => ">=",
            };
            format!("(if ({} {} {}) 1 0)", j, lhs, rhs)
        }
        And => format!("(if (and (not= 0 {}) (not= 0 {})) 1 0)", lhs, rhs),
        Or => format!("(if (or (not= 0 {}) (not= 0 {})) 1 0)", lhs, rhs),
        _ => {
            let j = match op {
                Add => "+",
                Sub => "-",
                Mul => "*",
                Div => "/",
                Mod => "%",
                Pow => "math/pow",
                Shl => "blshift",
                Shr => "brshift",
                BitAnd => "band",
                BitOr => "bor",
                _ => "bxor",
            };
            format!("({} {} {})", j, lhs, rhs)
        }
    }
}

/// Emit a compound arithmetic assignment `var op= value` as janet.
fn arith_compound_assign_to_janet(var: &str, op: crate::ir::IrArithOp, value: String) -> String {
    use crate::ir::IrArithOp::*;
    let op_str = match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Mod => "%",
        _ => "?",
    };
    format!("(set {} ({} {} {}))", var, op_str, var, value)
}

/// Emit a unary arithmetic op as janet.
fn arith_unaryop_to_janet(op: crate::ir::IrUnaryArithOp, operand: String) -> String {
    use crate::ir::IrUnaryArithOp::*;
    let op_str = match op {
        Neg => "-",
        Pos => "+",
        Not => "not",
        BitNot => "bnot",
    };
    format!("({} {})", op_str, operand)
}

/// Convert a structured ArithExpr to Janet code.
fn arith_expr_to_janet(expr: &crate::ir::ArithExpr) -> String {
    use crate::ir::ArithExpr;
    match expr {
        ArithExpr::Literal(n) => n.to_string(),
        ArithExpr::Var(name) => name.clone(),
        ArithExpr::DollarVar(name) => arith_dollar_var_to_janet(name),
        ArithExpr::BinOp { left, op, right } => {
            arith_binop_to_janet(*op, arith_expr_to_janet(left), arith_expr_to_janet(right))
        }
        ArithExpr::UnaryOp { op, operand } => {
            arith_unaryop_to_janet(*op, arith_expr_to_janet(operand))
        }
        ArithExpr::PreIncDec { var, inc } | ArithExpr::PostIncDec { var, inc } => {
            format!("({} {} 1)", if *inc { "+" } else { "-" }, var)
        }
        ArithExpr::Assign { var, value } => {
            format!("(set {} {})", var, arith_expr_to_janet(value))
        }
        ArithExpr::CompoundAssign { var, op, value } => {
            arith_compound_assign_to_janet(var, *op, arith_expr_to_janet(value))
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
