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
    // Arithmetic operates on integers, but sash variables hold strings
    // (bash semantics). `_num` coerces a string operand to an integer at
    // each arithmetic use site, honoring DD#72 (0x/0o/0b prefixes; a
    // leading-zero decimal like 010 stays base-10). A value that is already
    // a number passes through; a non-numeric string yields 0.
    writeln!(
        out,
        "(defn _num [v] (if (number? v) v (let [s (string v) neg (string/has-prefix? \"-\" s) \
b (if neg (slice s 1) s) r (cond (string/has-prefix? \"0o\" b) (scan-number (string \"8r\" (slice b 2))) \
(string/has-prefix? \"0b\" b) (scan-number (string \"2r\" (slice b 2))) (scan-number b)) \
n (or r 0)] (if neg (- n) n))))"
    )
    .unwrap();
    // DD#77: `_isint` — strict integer-validity predicate for `[[ -i s ]]`
    // (matches the runtime sash_is_int grammar: sign + decimal digits, or a
    // 0x/0o/0b prefix with valid digits, whole string, no trailing junk).
    writeln!(
        out,
        "(defn _fmtint [s] (not (nil? (peg/match ~(* (? (set \"+-\")) (+ \
(* \"0\" (set \"xX\") (some (range \"09\" \"af\" \"AF\"))) \
(* \"0\" (set \"oO\") (some (range \"07\"))) \
(* \"0\" (set \"bB\") (some (set \"01\"))) \
(some (range \"09\"))) -1) s))))"
    )
    .unwrap();
    // DD#77 (DG 2026-08-17): `-i`/is_int is range-checked — a well-formed but
    // out-of-range value is false. `_fmtint` validates the strict format (all
    // prefixes); the i64 range is checked via `int/s64` for decimal/hex, with a
    // format-only fallback for `0o`/`0b` (which `int/s64` doesn't parse — an
    // out-of-range octal/binary literal is a negligible, untested edge).
    writeln!(
        out,
        "(defn _isint [v] (def s (string v)) (and (_fmtint s) \
(or (try (do (int/s64 s) true) ([e] false)) \
(not (nil? (peg/match ~(* (? (set \"+-\")) \"0\" (set \"oObB\")) s))))))"
    )
    .unwrap();
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
        Expansion(inner) => _expr_uses_args(inner),
        Literal(_) | Var(_) | PostIncDec { .. } | PreIncDec { .. } => false,
        other => _arith_compound_uses_args(other),
    }
}

/// The operator/compound `ArithExpr` forms for `_arith_uses_args` (split out to
/// keep the dispatcher small).
fn _arith_compound_uses_args(a: &crate::ir::ArithExpr) -> bool {
    use crate::ir::ArithExpr::*;
    match a {
        BinOp { left, right, .. } => _arith_uses_args(left) || _arith_uses_args(right),
        UnaryOp { operand, .. } | Paren(operand) => _arith_uses_args(operand),
        Assign { value, .. } | CompoundAssign { value, .. } => _arith_uses_args(value),
        Ternary {
            cond,
            true_val,
            false_val,
        } => _arith_uses_args(cond) || _arith_uses_args(true_val) || _arith_uses_args(false_val),
        Comma(exprs) | FuncCall { args: exprs, .. } => exprs.iter().any(_arith_uses_args),
        Subscript { index, .. } => _arith_uses_args(index),
        SubscriptAssign { index, value, .. } => _arith_uses_args(index) || _arith_uses_args(value),
        _ => false,
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
    /// True while emitting the body of a `-> int` function: `return N`
    /// yields the numeric value via the `_ret` label (DD#58 nullary calls
    /// read it in arithmetic), rather than a bare `(return …)`.
    in_int_func: bool,
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
            in_int_func: false,
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
        StmtKind::ArithTest(expr) => emit_arith_test(out, ctx, expr, depth),
        StmtKind::ReturnProducer(inner) => emit_return_producer(out, ctx, inner, depth),
    }
}

/// DD#74: `return <producer>` — evaluate the producer as a condition and set
/// `__last_exit` to the resulting status (0 true / 1 false), then return. (The
/// exact code of a `return CMD` is narrowed to 0/1 here; native/c preserve it.)
fn emit_return_producer(out: &mut String, ctx: &mut EmitCtx, inner: &Statement, depth: usize) {
    let mut cond = String::new();
    emit_condition_inline(&mut cond, ctx, inner);
    indent(out, depth);
    writeln!(
        out,
        "(do (set __last_exit (if {} 0 1)) (return _ret nil))",
        cond
    )
    .unwrap();
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
    if ctx.is_declared(&a.name) {
        let val = scalar_assign_value(ctx, a, a.op == AssignOp::Append);
        writeln!(out, "(set {} {})", a.name, val).unwrap();
    } else {
        // `+=` on an unset var is `=` in bash (append to empty) — bind fresh.
        let val = scalar_assign_value(ctx, a, false);
        ctx.declare(&a.name);
        writeln!(out, "(var {} {})", a.name, val).unwrap();
    }
}

/// The janet value expression for a scalar assignment. `x = v` overwrites;
/// `x += v` (accumulate) adds arithmetically for an int target and concatenates
/// for a string target — mirroring the shared transform's `append_scalar`.
fn scalar_assign_value(ctx: &EmitCtx, a: &Assignment, accumulate: bool) -> String {
    let is_int = ctx.is_int(&a.name);
    let v = if is_int {
        emit_expr_as_int(&a.value, ctx)
    } else {
        emit_expr_inline(&a.value, ctx)
    };
    if !accumulate {
        return v;
    }
    if is_int {
        format!("(+ {} {})", a.name, v)
    } else {
        format!("(string {} {})", a.name, v)
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
/// array), an empty array/map literal for a bare `array a;` / `map m;`,
/// or a plain inline value.
fn emit_decl_value(d: &Declaration, a: &Assignment, ctx: &EmitCtx) -> String {
    if d.var_type == VarType::Int {
        emit_expr_as_int(&a.value, ctx)
    } else if let Some(start) = argv_start_index(&a.value, ctx) {
        format!("(array/slice (dyn :args) (min {start} (length (dyn :args))))")
    } else if is_empty_literal(&a.value) && is_map_decl(d) {
        "@{}".to_string()
    } else if is_empty_literal(&a.value) && is_array_decl(d) {
        "@[]".to_string()
    } else {
        emit_expr_inline(&a.value, ctx)
    }
}

/// A bare declaration (`array a;`, `map m;`, `int x`) carries an empty
/// literal value (parser: `ExprKind::Literal("")`).
fn is_empty_literal(e: &Expr) -> bool {
    matches!(&e.kind, ExprKind::Literal(s) if s.is_empty())
}

/// `array NAME` → parser flag `-a`; `map NAME` → `-A`.
fn is_array_decl(d: &Declaration) -> bool {
    d.flags.iter().any(|f| f == "-a")
}
fn is_map_decl(d: &Declaration) -> bool {
    d.flags.iter().any(|f| f == "-A")
}

fn declare_decl_var(ctx: &mut EmitCtx, d: &Declaration, a: &Assignment) {
    if d.var_type == VarType::Int {
        ctx.declare_int(&a.name);
    } else if is_map_decl(d) || matches!(a.value.kind, ExprKind::AssocArray(_)) {
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
        emit_case_item(out, ctx, item, &c.op, depth);
    }
    indent(out, depth);
    writeln!(out, ")").unwrap();
}

fn emit_case_item(
    out: &mut String,
    ctx: &mut EmitCtx,
    item: &crate::ir::CaseItem,
    op: &str,
    depth: usize,
) {
    indent(out, depth + 1);
    let pats: Vec<String> = item
        .patterns
        .iter()
        .map(|p| emit_expr_inline(p, ctx))
        .collect();
    if item.is_default {
        // the `|| { … }` default arm matches anything (DD#71 — `*` is a
        // literal, not a wildcard).
        write!(out, "_ ").unwrap();
    } else if op == "=~" {
        // DD#66 regex dispatch: a guard that matches via the sash-regex
        // helper (same engine as native/C), unanchored.
        let alts: Vec<String> = pats
            .iter()
            .map(|p| format!("(zero? (os/execute [\"sash-regex\" \"matchq\" _casev {p}] :p))"))
            .collect();
        write!(out, "(_casev (or {})) ", alts.join(" ")).unwrap();
    } else if pats.len() == 1 {
        write!(out, "{} ", pats[0]).unwrap();
    } else {
        // Multi-pattern exact arm `a|b|…` → a janet `match` guard alternation
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
    let is_int = f.return_type == VarType::Int;
    if is_int {
        ctx.int_funcs.insert(f.name.clone());
    }
    indent(out, depth);
    if needs_args_preamble(&f.body) {
        writeln!(out, "(defn {} [& _args]", f.name).unwrap();
    } else {
        writeln!(out, "(defn {} []", f.name).unwrap();
    }
    // Every function body is wrapped in a `_ret` label so `return` is
    // catchable: a `-> int` function yields its numeric value (`return _ret N`);
    // a predicate/void function returns nil after setting __last_exit (DD#74).
    indent(out, depth + 1);
    writeln!(out, "(label _ret").unwrap();
    let saved = ctx.in_int_func;
    ctx.in_int_func = is_int;
    for s in &f.body {
        emit_stmt(out, ctx, s, depth + 1);
    }
    ctx.in_int_func = saved;
    indent(out, depth + 1);
    writeln!(out, ")").unwrap();
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

fn emit_arith_test(out: &mut String, ctx: &EmitCtx, expr: &ArithExpr, depth: usize) {
    indent(out, depth);
    writeln!(out, "(not= 0 {})", arith_expr_to_janet(expr, ctx)).unwrap();
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
    // DD#20 lastpipe: `… | read VAR` / `… | mapfile ARR` run the consumer
    // in-process (reading the producers' combined output) rather than
    // os/spawning a nonexistent `read`/`mapfile` executable.
    if let Some(lp) = detect_lastpipe(&p.commands) {
        emit_lastpipe_pipeline(out, ctx, p, &lp, depth);
        return;
    }
    // Statement position: run the pipeline (sets __last_exit to the DD#55
    // aggregate), then abort on a nonzero aggregate when checked.
    let checked = ctx.checked;
    emit_pipeline_do(out, ctx, p, depth);
    writeln!(out).unwrap();
    if checked {
        indent(out, depth);
        writeln!(out, "(when (not= 0 __last_exit) (os/exit __last_exit))").unwrap();
    }
}

/// Emit a `(do …)` block that runs every stage (a `-`-suppressed stage still
/// runs — its status is just excluded), prints the last stage's output, sets
/// `__last_exit` to the DD#55 aggregate (first-nonzero over the non-suppressed
/// stages), and *returns* `__last_exit`. Used both as a statement and, via
/// `(zero? …)`, as a condition.
fn emit_pipeline_do(out: &mut String, ctx: &mut EmitCtx, p: &Pipeline, depth: usize) {
    let last = p.commands.len() - 1;
    let suppressed: Vec<bool> = p
        .commands
        .iter()
        .map(|c| matches!(c.kind, StmtKind::Suppressed(_)))
        .collect();
    indent(out, depth);
    writeln!(out, "(do").unwrap();
    for (i, cmd) in p.commands.iter().enumerate() {
        emit_pipeline_stage(out, ctx, cmd, i, last, depth + 1);
    }
    emit_pipeline_close_pipes(out, last, depth + 1);
    indent(out, depth + 1);
    writeln!(out, "(def result (or (:read (p{} :out) :all) \"\"))", last).unwrap();
    indent(out, depth + 1);
    writeln!(out, "(set __last_exit 0)").unwrap();
    for (i, &sup) in suppressed.iter().enumerate() {
        indent(out, depth + 1);
        if sup {
            // Suppressed stage: reap only, status excluded from the aggregate.
            writeln!(out, "(os/proc-wait p{})", i).unwrap();
        } else {
            writeln!(
                out,
                "(let [_s (os/proc-wait p{})] (when (and (zero? __last_exit) (not= 0 _s)) (set __last_exit _s)))",
                i
            )
            .unwrap();
        }
    }
    indent(out, depth + 1);
    // Pass the last stage's output through verbatim (`prin`, no added newline)
    // so an empty pipeline prints nothing rather than a spurious blank line.
    writeln!(out, "(prin result)").unwrap();
    indent(out, depth + 1);
    writeln!(out, "__last_exit)").unwrap();
}

/// A pipeline whose last stage is an in-process builtin consumer
/// (`read` / `mapfile` / `readarray`) — DD#20 lastpipe.
enum Lastpipe {
    /// `read VAR…` (with optional `-f<sep>` field separator).
    Read {
        vars: Vec<String>,
        field_sep: Option<u8>,
    },
    /// `mapfile -t ARR` — one array element per input line.
    MapfileArray { name: String },
    /// `mapfile -m MAP` — split each line KEY<sep>VALUE into a map.
    MapfileMap { name: String, sep: u8 },
}

/// Classify a pipeline's final stage as an in-process `read`/`mapfile`
/// consumer, or `None` if it is an ordinary spawned command / compound.
fn detect_lastpipe(commands: &[Statement]) -> Option<Lastpipe> {
    if commands.len() < 2 {
        return None;
    }
    let StmtKind::Command(c) = &commands.last()?.kind else {
        return None;
    };
    match BuiltinCmd::from_name(&expr_to_literal(&c.name)) {
        BuiltinCmd::Read => Some(parse_read_consumer(c)),
        BuiltinCmd::Mapfile => Some(parse_mapfile_consumer(c)),
        _ => None,
    }
}

/// Parse `read [-f<sep>] VAR…` target vars + field separator.
fn parse_read_consumer(c: &Command) -> Lastpipe {
    let args: Vec<String> = c.args.iter().map(expr_to_literal).collect();
    let field_sep = field_sep_of(&args);
    let mut vars = read_target_vars(&args);
    if vars.is_empty() {
        vars.push("REPLY".to_string());
    }
    Lastpipe::Read { vars, field_sep }
}

/// `read`'s target variable names: non-flag args, skipping the argument
/// after a separate-form `-f`/`-d` (its separator/delimiter).
fn read_target_vars(args: &[String]) -> Vec<String> {
    let mut vars = vec![];
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
        } else if a == "-f" || a == "-d" {
            skip = true;
        } else if !a.starts_with('-') {
            vars.push(a.clone());
        }
    }
    vars
}

/// `-f<sep>` (joined) or `-f <sep>` (separate) → the field-separator byte.
fn field_sep_of(args: &[String]) -> Option<u8> {
    let i = args.iter().position(|a| a.starts_with("-f"))?;
    let a = &args[i];
    if a.len() > 2 {
        Some(a.as_bytes()[2])
    } else {
        args.get(i + 1).and_then(|n| n.bytes().next())
    }
}

/// Parse `mapfile [-m] [-f<sep>] NAME` mode + separator + target.
fn parse_mapfile_consumer(c: &Command) -> Lastpipe {
    let args: Vec<String> = c.args.iter().map(expr_to_literal).collect();
    let map_mode = args.iter().any(|a| a == "-m");
    let sep = field_sep_of(&args).unwrap_or(b'=');
    let name = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_default();
    if map_mode {
        Lastpipe::MapfileMap { name, sep }
    } else {
        Lastpipe::MapfileArray { name }
    }
}

/// Emit a lastpipe pipeline: run producer stages (all but the last) as
/// os/spawn children, capture their combined stdout, then bind the
/// consumer variable(s) in-process. Consumer targets are pre-declared at
/// the enclosing scope so later statements can see them (janet `def`/`var`
/// inside the `(do …)` would not escape it).
fn emit_lastpipe_pipeline(
    out: &mut String,
    ctx: &mut EmitCtx,
    p: &Pipeline,
    lp: &Lastpipe,
    depth: usize,
) {
    let plast = p.commands.len() - 2;
    predeclare_lastpipe_targets(out, ctx, lp, depth);
    indent(out, depth);
    writeln!(out, "(do").unwrap();
    for i in 0..=plast {
        emit_pipeline_stage(out, ctx, &p.commands[i], i, plast, depth + 1);
    }
    emit_pipeline_close_pipes(out, plast, depth + 1);
    indent(out, depth + 1);
    writeln!(out, "(def _cap (or (:read (p{} :out) :all) \"\"))", plast).unwrap();
    for i in 0..=plast {
        indent(out, depth + 1);
        writeln!(out, "(os/proc-wait p{})", i).unwrap();
    }
    emit_lastpipe_bind(out, lp, depth + 1);
    indent(out, depth + 1);
    writeln!(out, "(set __last_exit 0))").unwrap();
}

/// Declare each consumer target (empty string / array / map) at the
/// enclosing scope, if not already declared. A map target is registered
/// as a map so later `${m[key]}` subscripts use string keys.
fn predeclare_lastpipe_targets(out: &mut String, ctx: &mut EmitCtx, lp: &Lastpipe, depth: usize) {
    match lp {
        Lastpipe::Read { vars, .. } => {
            for v in vars {
                predeclare_one(out, ctx, v, "\"\"", false, depth);
            }
        }
        Lastpipe::MapfileArray { name } => predeclare_one(out, ctx, name, "@[]", false, depth),
        Lastpipe::MapfileMap { name, .. } => predeclare_one(out, ctx, name, "@{}", true, depth),
    }
}

fn predeclare_one(
    out: &mut String,
    ctx: &mut EmitCtx,
    name: &str,
    init: &str,
    is_map: bool,
    depth: usize,
) {
    if ctx.is_declared(name) {
        return;
    }
    if is_map {
        ctx.declare_map(name);
    } else {
        ctx.declare(name);
    }
    indent(out, depth);
    writeln!(out, "(var {} {})", name, init).unwrap();
}

/// Bind the consumer target(s) from the captured producer output `_cap`.
fn emit_lastpipe_bind(out: &mut String, lp: &Lastpipe, depth: usize) {
    match lp {
        Lastpipe::Read { vars, field_sep } => emit_lastpipe_read(out, vars, *field_sep, depth),
        Lastpipe::MapfileArray { name } => {
            indent(out, depth);
            writeln!(
                out,
                "(set {} (let [_ls (string/split \"\\n\" _cap)] \
                 (if (and (> (length _ls) 0) (= \"\" (array/peek _ls))) \
                 (array/slice _ls 0 -2) _ls)))",
                name
            )
            .unwrap();
        }
        Lastpipe::MapfileMap { name, sep } => {
            indent(out, depth);
            writeln!(out, "(set {} @{{}})", name).unwrap();
            indent(out, depth);
            writeln!(
                out,
                "(each _l (string/split \"\\n\" _cap) (when (not (empty? _l)) \
                 (def _e (string/find (string/from-bytes {sep}) _l)) \
                 (when _e (put {name} (string/slice _l 0 _e) (string/slice _l (+ _e 1))))))",
                sep = sep,
                name = name
            )
            .unwrap();
        }
    }
}

/// Bind `read VAR…`: the single-var case takes the whole first line; the
/// multi-var case splits the first line, one field per var, the last var
/// taking the remainder (mirrors codegen's split_first_word / split_field).
fn emit_lastpipe_read(out: &mut String, vars: &[String], field_sep: Option<u8>, depth: usize) {
    if vars.len() == 1 {
        indent(out, depth);
        writeln!(
            out,
            "(set {} (string/trimr (get (string/split \"\\n\" _cap) 0 \"\")))",
            vars[0]
        )
        .unwrap();
        return;
    }
    let sep_str = janet_sep_str(field_sep);
    indent(out, depth);
    writeln!(out, "(def _line (get (string/split \"\\n\" _cap) 0 \"\"))").unwrap();
    indent(out, depth);
    writeln!(out, "(def _flds {})", janet_split_fields(field_sep)).unwrap();
    let last = vars.len() - 1;
    for (i, v) in vars.iter().enumerate() {
        indent(out, depth);
        writeln!(out, "{}", janet_read_bind_field(v, i, last, &sep_str)).unwrap();
    }
}

/// The janet string form of a field separator: a byte for `-f<sep>`, else a
/// literal space (whitespace splitting).
fn janet_sep_str(field_sep: Option<u8>) -> String {
    match field_sep {
        Some(b) => format!("(string/from-bytes {})", b),
        None => "\" \"".to_string(),
    }
}

/// The janet expression splitting `_line` into `_flds` — by the field
/// separator, or on whitespace (dropping empty runs).
fn janet_split_fields(field_sep: Option<u8>) -> String {
    match field_sep {
        Some(b) => format!("(string/split (string/from-bytes {}) _line)", b),
        None => "(filter (fn [_x] (not (empty? _x))) (string/split \" \" _line))".to_string(),
    }
}

/// Bind one `read` target: field `i`, or the joined remainder for the last.
fn janet_read_bind_field(v: &str, i: usize, last: usize, sep: &str) -> String {
    if i == last {
        format!(
            "(set {} (string/join (array/slice _flds {}) {}))",
            v, last, sep
        )
    } else {
        format!("(set {} (get _flds {} \"\"))", v, i)
    }
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
    // DD#55: a `-`-suppressed stage still runs — unwrap to the inner command
    // (its status is excluded from the aggregate in emit_pipeline_do).
    let cmd = match &cmd.kind {
        StmtKind::Suppressed(inner) => inner.as_ref(),
        _ => cmd,
    };
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

fn emit_command(out: &mut String, ctx: &mut EmitCtx, c: &Command, _stmt: &Statement, depth: usize) {
    let cmd_name = expr_to_literal(&c.name);
    let kind = BuiltinCmd::from_name(&cmd_name);
    let defaults_success = janet_cmd_defaults_success(&kind, &cmd_name);
    indent(out, depth);
    match kind {
        BuiltinCmd::Echo => emit_janet_echo(out, ctx, c),
        // `:` no-op — succeeds silently (the canonical `|| :` discard).
        BuiltinCmd::Noop => emit_janet_const_bool(out, true),
        BuiltinCmd::Exit => emit_janet_exit(out, ctx, c),
        BuiltinCmd::Test => emit_janet_test_cmd(out, ctx, c),
        BuiltinCmd::Match => emit_janet_match(out, ctx, c),
        BuiltinCmd::Break | BuiltinCmd::Continue => {
            // Janet's (each) doesn't support continue; use (break) as approximation
            writeln!(out, "(break)").unwrap();
        }
        BuiltinCmd::Return => emit_janet_return(out, ctx, c),
        BuiltinCmd::Read => emit_janet_read(out, ctx, c),
        BuiltinCmd::Argv => emit_janet_argv(out, ctx, c),
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
        BuiltinCmd::Echo | BuiltinCmd::Read | BuiltinCmd::Unset | BuiltinCmd::Bless
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
    // Inside a `-> int` function, `return N` yields N's numeric value out of
    // the `_ret` label so DD#58 nullary calls read it as an integer.
    if ctx.in_int_func {
        let val = match c.args.first() {
            Some(code) => emit_expr_as_int(code, ctx),
            None => "0".to_string(),
        };
        writeln!(out, "(return _ret {})", val).unwrap();
        return;
    }
    if let Some(code) = c.args.first() {
        // DD#74: a plain (predicate) function's `return N` sets its exit
        // status so an enclosing `if f`/`while f` (which checks __last_exit)
        // sees it — janet's bare `(return …)` value is not used for status.
        let val = emit_expr_as_int(code, ctx);
        writeln!(out, "(do (set __last_exit {}) (return _ret nil))", val).unwrap();
    } else {
        writeln!(out, "(do (set __last_exit 0) (return _ret nil))").unwrap();
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
/// `@a` / `@{a:…}` splat to a `;` splice (DD#20/DD#22 — one element per
/// value; DD#69 empty-disposition).
fn janet_command_parts(cmd_name: &str, c: &Command, ctx: &mut EmitCtx) -> Vec<String> {
    let mut parts = vec![format!("\"{}\"", cmd_name)];
    for arg in &c.args {
        match janet_splat_splice(arg, ctx) {
            Some(splice) => parts.push(splice),
            None => parts.push(emit_expr_inline(arg, ctx)),
        }
    }
    parts
}

/// The `;`-splice for a command-arg `@a` / `@{a:-WORD}` / `@{a:?WORD}`
/// splat: non-empty → the array; empty → the WORD (`:-`, one element),
/// zero args (bare `@{a:-}`), or an abort (`:?`).
fn janet_splat_splice(arg: &Expr, ctx: &mut EmitCtx) -> Option<String> {
    let pe = splat_pe(arg)?;
    let n = &pe.name;
    match pe.operator.as_deref() {
        Some("arrvals") => Some(format!(";{}", n)),
        Some("arrvals_default") => Some(match &pe.operand {
            Some(w) => format!(
                ";(if (empty? {n}) @[{}] {n})",
                emit_expr_inline(w, ctx),
                n = n
            ),
            None => format!(";{}", n),
        }),
        Some("arrvals_abort") => {
            let w = pe
                .operand
                .as_ref()
                .map(|w| emit_expr_inline(w, ctx))
                .unwrap_or_else(|| "\"\"".to_string());
            Some(format!(";(if (empty? {n}) (error {}) {n})", w, n = n))
        }
        _ => None,
    }
}

/// The `ParameterExpansion` behind a `@a` / single-part `"${a[@]}"` arg.
fn splat_pe(arg: &Expr) -> Option<&ParameterExpansion> {
    match &arg.kind {
        ExprKind::ParameterExpansion(pe) => Some(pe),
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => match &parts[0].kind {
            ExprKind::ParameterExpansion(pe) => Some(pe),
            _ => None,
        },
        _ => None,
    }
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

/// DD#60: `argv [N] a` fills the trailing array with the program argv
/// slice `(dyn :args)[start:]` (clamped so start >= argc → empty). The
/// trailing positional is the array; an optional leading integer is the
/// slice start. Auto-declares the array (like `read`/mapfile).
fn emit_janet_argv(out: &mut String, ctx: &mut EmitCtx, c: &Command) {
    let args: Vec<String> = c.args.iter().map(expr_to_literal).collect();
    let Some(array) = args.last().cloned() else {
        return;
    };
    let start = match args.len() {
        1 => "0".to_string(),
        2 => args[0].clone(),
        _ => return,
    };
    let slice = format!("(array/slice (dyn :args) (min {start} (length (dyn :args))))");
    if !ctx.is_declared(&array) {
        ctx.declare(&array);
        writeln!(out, "(var {array} {slice})").unwrap();
    } else {
        writeln!(out, "(set {array} {slice})").unwrap();
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
fn emit_condition_inline(out: &mut String, ctx: &mut EmitCtx, stmt: &Statement) {
    match &stmt.kind {
        StmtKind::Command(c) => emit_command_condition(out, ctx, c),
        // `(( … ))` condition: true iff the arith value is non-zero. Arith
        // comparisons/logicals now yield 1/0 integers (arith_binop_to_janet),
        // so this is correct for both comparison and value forms.
        StmtKind::ArithTest(a) => write!(out, "(not= 0 {})", arith_expr_to_janet(a, ctx)).unwrap(),
        // DD#55: a pipeline condition is true iff its aggregate (pipefail over
        // the non-suppressed segments) is zero. The pipeline still runs and
        // prints; only its status feeds the branch.
        StmtKind::Pipeline(p) if p.commands.len() > 1 => {
            write!(out, "(zero? ").unwrap();
            emit_pipeline_do(out, ctx, p, 0);
            write!(out, ")").unwrap();
        }
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
    // DD#63: `match S P [ARR]` boolean command → the `sash-regex` helper.
    if try_emit_match_condition(out, ctx, &name, c) {
        return;
    }
    // DD#74: a user function used as a condition — call it (which sets
    // __last_exit, incl. via `return N`), then branch on that status. Without
    // this, the name would be os/execute'd as an external program.
    if ctx.is_function(&name) {
        emit_func_call_condition(out, ctx, &name, c);
        return;
    }
    let mut parts = vec![format!("\"{}\"", name)];
    for arg in &c.args {
        parts.push(emit_expr_inline(arg, ctx));
    }
    write!(out, "(zero? (os/execute [{}] :p))", parts.join(" ")).unwrap();
}

/// A user function call used as an if/while condition: call it (its body,
/// incl. a DD#74 `return N`, sets `__last_exit`), then the condition is
/// `__last_exit == 0`.
fn emit_func_call_condition(out: &mut String, ctx: &EmitCtx, name: &str, c: &Command) {
    let mut args_str = String::new();
    for arg in &c.args {
        args_str.push(' ');
        args_str.push_str(&emit_expr_inline(arg, ctx));
    }
    write!(out, "(do ({}{}) (zero? __last_exit))", name, args_str).unwrap();
}

/// DD#63: `match S P` / `match S P ARR` as an if/while condition. Returns
/// true (and emits) when handled. `matchq` yields exit-status only (no stdout
/// leak); the out-array form populates ARR and yields the boolean.
fn try_emit_match_condition(out: &mut String, ctx: &EmitCtx, name: &str, c: &Command) -> bool {
    if name == "match" && c.args.len() == 2 {
        let s = emit_expr_inline(&c.args[0], ctx);
        let p = emit_expr_inline(&c.args[1], ctx);
        write!(
            out,
            "(zero? (os/execute [\"sash-regex\" \"matchq\" {} {}] :p))",
            s, p
        )
        .unwrap();
        return true;
    }
    if name == "match" && c.args.len() == 3 {
        let s = emit_expr_inline(&c.args[0], ctx);
        let p = emit_expr_inline(&c.args[1], ctx);
        let arr = expr_to_literal(&c.args[2]);
        write!(out, "{}", emit_janet_match_into_expr(&s, &p, &arr)).unwrap();
        return true;
    }
    false
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
        // DD#77: `-i` — strict integer-validity of the operand's value.
        "-i" => format!("(_isint {})", val),
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
        // DD#63: `[[ s =~ pat ]]` — regex match via the sash-regex helper
        // (same vendored regexp9 engine as native/C, so `\w`/`\d`/`\s`
        // shorthand classes work); quiet matchq form (exit 0 = match).
        "=~" => format!(
            "(zero? (os/execute [\"sash-regex\" \"matchq\" {} {}] :p))",
            l, r
        ),
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
        ExprKind::Arithmetic(a) => arith_expr_to_janet(a, ctx),
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
        ExprKind::Arithmetic(a) => format!("(string {})", arith_expr_to_janet(a, ctx)),
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

/// Option C: `$(f)` on an `-> int` function yields the return value (like
/// `$((f))`), not stdout. Stringify the call so it flows through janet's
/// existing cmdsub consumers uniformly (echo prints it; an int assignment's
/// `scan-number (string/trim …)` wrapper parses it back).
fn cmdsub_int_func(name: &str, c: &Command, ctx: &EmitCtx) -> Option<String> {
    if !ctx.is_int_func(name) {
        return None;
    }
    let arg_strs: Vec<String> = c.args.iter().map(|a| emit_expr_inline(a, ctx)).collect();
    let call = if arg_strs.is_empty() {
        format!("({})", name)
    } else {
        format!("({} {})", name, arg_strs.join(" "))
    };
    Some(format!("(string {})", call))
}

fn emit_expr_cmdsub_command(c: &Command, ctx: &EmitCtx) -> String {
    let name = expr_to_literal(&c.name);
    if let Some(s) = cmdsub_int_func(&name, c, ctx) {
        return s;
    }
    if let Some(s) = cmdsub_regex_intrinsic(&name, c, ctx) {
        return s;
    }
    if let Some(s) = cmdsub_case_intrinsic(&name, c, ctx) {
        return s;
    }
    if let Some(s) = cmdsub_path_intrinsic(&name, c, ctx) {
        return s;
    }
    if let Some(s) = cmdsub_data_intrinsic(&name, c, ctx) {
        return s;
    }
    if let Some(s) = cmdsub_num_intrinsic(&name, c, ctx) {
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

/// DD#65: `$(toupper s)` / `$(tolower s)` — ASCII case conversion.
fn cmdsub_case_intrinsic(name: &str, c: &Command, ctx: &EmitCtx) -> Option<String> {
    if (name == "toupper" || name == "tolower") && c.args.len() == 1 {
        let subj = emit_expr_inline(&c.args[0], ctx);
        let f = if name == "toupper" {
            "string/ascii-upper"
        } else {
            "string/ascii-lower"
        };
        return Some(format!("({f} {subj})"));
    }
    None
}

/// DD#65: `$(basename p)` / `$(dirname p)` — path sugar over a fixed-pattern
/// `sub` (same sash-regex helper as `$(sub …)`, byte-identical to native/C).
fn cmdsub_path_intrinsic(name: &str, c: &Command, ctx: &EmitCtx) -> Option<String> {
    if (name != "basename" && name != "dirname") || c.args.len() != 1 {
        return None;
    }
    let subj = emit_expr_inline(&c.args[0], ctx);
    let pat = if name == "basename" { ".*/" } else { "/[^/]*$" };
    Some(format!(
        "(string/trim (with [f (os/spawn [\"sash-regex\" \"sub\" {subj} \"{pat}\" \"\"] :p {{:out :pipe}})] (or (:read (f :out) :all) \"\")))"
    ))
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
    // `$(arg N DEFAULT)` (DD#60 addendum) — argv[N], or DEFAULT if out of range.
    if name == "arg" && c.args.len() == 2 {
        let idx = emit_expr_as_int(&c.args[0], ctx);
        let def = emit_expr_inline(&c.args[1], ctx);
        return Some(format!(
            "(let [_ai {idx}] (if (and (>= _ai 0) (< _ai (length (dyn :args)))) \
             (get (dyn :args) _ai) {def}))"
        ));
    }
    cmdsub_join_intrinsic(name, c, ctx)
}

/// DD (fixed-point) numeric intrinsics. `$(dtoi DIGITS DECIMAL)` parses a
/// fixed-point decimal to a scaled integer; `$(itod DIGITS VALUE)` renders
/// it back. Janet is illustration (not the numeric oracle), so this uses
/// idiomatic float arithmetic rather than the byte-exact runtime routines.
fn cmdsub_num_intrinsic(name: &str, c: &Command, ctx: &EmitCtx) -> Option<String> {
    if name == "dtoi" && c.args.len() == 2 {
        let digits = emit_expr_as_int(&c.args[0], ctx);
        let dec = emit_expr_inline(&c.args[1], ctx);
        return Some(format!(
            "(let [_dg {digits}] (string (math/round (* (scan-number {dec}) (math/pow 10 _dg)))))"
        ));
    }
    if name == "itod" && c.args.len() == 2 {
        let digits = emit_expr_as_int(&c.args[0], ctx);
        let value = emit_expr_as_int(&c.args[1], ctx);
        return Some(format!(
            "(let [_dg {digits}] (string/format (string \"%.\" _dg \"f\") (/ {value} (math/pow 10 _dg))))"
        ));
    }
    None
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
        ":?" => emit_pe_require(pe, ctx),
        "strlen" | "arrlen" => format!("(string (length (or {} \"\")))", var_ref),
        // `${a[i]:o:l}` substrings the cell read; the scalar `${s:o:l}` uses the
        // plain var_ref. Edge 4.
        "substring" => {
            let src = match &pe.index {
                Some(idx) => emit_expr_subscript(&pe.name, idx, ctx),
                None => var_ref.clone(),
            };
            emit_pe_substring(&src, &pe_operand_literal(pe))
        }
        // DD#65: `:s/` sed op keeps the raw `/`//` operator; the dropped
        // `/`-family glob-replace was remapped to `glob_replace` (rejected
        // at parse, SB4034). All other `${var:OP}` ops are dropped.
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
    // `${a[i]:-DEF}` — the subject is the cell read (nil when out of range /
    // absent key), so coerce with `(or … "")` before the emptiness test.
    if let Some(index) = &pe.index {
        let cell = emit_expr_subscript(&pe.name, index, ctx);
        return format!(
            "(let [_c (or {} \"\")] (if (= (length _c) 0) {} _c))",
            cell, default
        );
    }
    if ctx.is_declared(&pe.name) {
        if ctx.is_int(&pe.name) {
            // `:-` triggers on the additive identity `0` for ints (DG), not
            // string emptiness — `(length int)` would error
            // (bug-cjanet-default-on-int-var). Yield strings on both arms (like
            // the string branch) so the int-context `scan-number` wrapper and a
            // bare string context both consume the result uniformly.
            format!("(if (= {0} 0) (string {1}) (string {0}))", pe.name, default)
        } else {
            format!("(if (= (length {}) 0) {} {})", pe.name, default, pe.name)
        }
    } else {
        format!("(or (os/getenv \"{}\") {})", pe.name, default)
    }
}

/// `${var:?MESSAGE}` — abort (`<prog>: MESSAGE` on stderr, exit 1) when var is
/// unset/empty, else the value. DD#77. Mirrors native/c's `sash_require_value`.
fn emit_pe_require(pe: &ParameterExpansion, ctx: &EmitCtx) -> String {
    let msg = pe
        .operand
        .as_ref()
        .map(|o| emit_expr_inline(o, ctx))
        .unwrap_or_else(|| "\"\"".to_string());
    // `${a[i]:?msg}` — the subject is the cell read (nil handled by the
    // `(nil? _v)` guard below).
    let var_ref = if let Some(index) = &pe.index {
        emit_expr_subscript(&pe.name, index, ctx)
    } else if ctx.is_declared(&pe.name) {
        pe.name.clone()
    } else {
        format!("(or (os/getenv \"{}\") \"\")", pe.name)
    };
    format!(
        "(let [_v {v}] (if (or (nil? _v) (empty? _v)) \
(do (eprint (get (dyn :args) 0) \": \" {m}) (os/exit 1)) _v))",
        v = var_ref,
        m = msg
    )
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
        ExprKind::DoubleQuoted(parts) => parts
            .iter()
            .map(expr_to_literal)
            .collect::<Vec<_>>()
            .join(""),
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
        format!("(_num (or (get _args {}) 0))", idx)
    } else {
        format!("(_num {})", name)
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
    format!("(set {} ({} (_num {}) {}))", var, op_str, var, value)
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
fn arith_expr_to_janet(expr: &crate::ir::ArithExpr, ctx: &EmitCtx) -> String {
    use crate::ir::ArithExpr;
    match expr {
        ArithExpr::Literal(n) => n.to_string(),
        // DD#58: a bare name that is a known `-> int` function is a nullary
        // call (`$((f))` → `(f)`), mirroring the transform's var_to_ssa
        // (transform/expr_handlers.rs). Otherwise it's a variable reference.
        ArithExpr::Var(name) => arith_var_to_janet(name, ctx),
        ArithExpr::DollarVar(name) => arith_dollar_var_to_janet(name),
        ArithExpr::PreIncDec { var, inc } | ArithExpr::PostIncDec { var, inc } => {
            format!("({} (_num {}) 1)", if *inc { "+" } else { "-" }, var)
        }
        ArithExpr::FuncCall { name, args } => arith_funccall_to_janet(name, args, ctx),
        ArithExpr::Paren(inner) => arith_expr_to_janet(inner, ctx),
        // A `${…}` embedded as an arith operand (e.g. `${s:-0xff}`): coerce to an
        // int via `_num`, which handles decimal + `0x`/`0o`/`0b` (matching the
        // native/c lowering). Arith-scoped, so a string-context `${s:-0xff}`
        // keeps its literal spelling. Without this arm the operand emitted empty
        // (bug surfaced by arith-disposition-hex-octal-default).
        ArithExpr::Expansion(inner) => format!("(_num {})", emit_expr_inline(inner, ctx)),
        _ => arith_op_expr_to_janet(expr, ctx),
    }
}

/// The operator / assignment / control ArithExpr arms (split out of
/// arith_expr_to_janet to keep each dispatcher narrow).
fn arith_op_expr_to_janet(expr: &crate::ir::ArithExpr, ctx: &EmitCtx) -> String {
    use crate::ir::ArithExpr;
    match expr {
        ArithExpr::BinOp { left, op, right } => arith_binop_to_janet(
            *op,
            arith_expr_to_janet(left, ctx),
            arith_expr_to_janet(right, ctx),
        ),
        ArithExpr::UnaryOp { op, operand } => {
            arith_unaryop_to_janet(*op, arith_expr_to_janet(operand, ctx))
        }
        ArithExpr::Assign { var, value } => {
            format!("(set {} {})", var, arith_expr_to_janet(value, ctx))
        }
        ArithExpr::CompoundAssign { var, op, value } => {
            arith_compound_assign_to_janet(var, *op, arith_expr_to_janet(value, ctx))
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => format!(
            "(if {} {} {})",
            arith_expr_to_janet(cond, ctx),
            arith_expr_to_janet(true_val, ctx),
            arith_expr_to_janet(false_val, ctx)
        ),
        ArithExpr::Comma(exprs) => {
            let parts: Vec<String> = exprs.iter().map(|e| arith_expr_to_janet(e, ctx)).collect();
            format!("(do {})", parts.join(" "))
        }
        _ => String::new(),
    }
}

/// A bare arith name: a nullary `-> int` function call, else a var ref
/// coerced to an integer (`_num`) for arithmetic.
fn arith_var_to_janet(name: &str, ctx: &EmitCtx) -> String {
    if ctx.is_int_func(name) {
        format!("({})", name)
    } else {
        format!("(_num {})", name)
    }
}

/// A DD#58 arith sexpr call `(name args…)`.
fn arith_funccall_to_janet(name: &str, args: &[crate::ir::ArithExpr], ctx: &EmitCtx) -> String {
    let arg_strs: Vec<String> = args.iter().map(|e| arith_expr_to_janet(e, ctx)).collect();
    if arg_strs.is_empty() {
        format!("({})", name)
    } else {
        format!("({} {})", name, arg_strs.join(" "))
    }
}
