/// safebash type checker (DD#16).
///
/// Operates on the IR after lowering. Reports type errors for:
/// - Cross-type assignment without explicit conversion
/// - Numeric comparison operators on string variables
/// - String comparison operators on int variables
/// - `exit` with unbounded int variable
/// - `if func` where func is `-> int`
use crate::ir::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "type error at {}:{}: {}",
            self.span.line, self.span.col, self.message
        )
    }
}

/// Environment tracking variable and function types.
struct TypeEnv {
    vars: HashMap<String, VarType>,
    funcs: HashMap<String, VarType>,
    /// Variables that are intrinsically int ($#, $?)
    intrinsic_int: HashSet<String>,
    /// Variables declared as array or map (collections)
    collections: HashSet<String>,
    /// Variables declared as map (associative array) — string keys
    assoc_arrays: HashSet<String>,
}

impl TypeEnv {
    fn new() -> Self {
        let mut intrinsic_int = HashSet::new();
        intrinsic_int.insert("#".to_string()); // $#
        intrinsic_int.insert("?".to_string()); // $?
        Self {
            vars: HashMap::new(),
            funcs: HashMap::new(),
            intrinsic_int,
            collections: HashSet::new(),
            assoc_arrays: HashSet::new(),
        }
    }

    fn var_type(&self, name: &str) -> VarType {
        if self.intrinsic_int.contains(name) {
            VarType::Int
        } else {
            self.vars.get(name).copied().unwrap_or(VarType::String)
        }
    }

    fn func_type(&self, name: &str) -> VarType {
        self.funcs.get(name).copied().unwrap_or(VarType::String)
    }

    fn is_collection(&self, name: &str) -> bool {
        self.collections.contains(name)
    }

    fn declare(&mut self, name: &str, vtype: VarType) {
        // Don't downgrade Int to String — once declared Int, it stays Int
        if vtype == VarType::String {
            if let Some(existing) = self.vars.get(name) {
                if *existing == VarType::Int {
                    return; // keep Int
                }
            }
        }
        self.vars.insert(name.to_string(), vtype);
    }
}

/// Infer the type of an expression in context.
fn expr_type(expr: &Expr, env: &TypeEnv) -> VarType {
    match &expr.kind {
        ExprKind::Arithmetic(_) => VarType::Int,
        ExprKind::SimpleExpansion(var) => env.var_type(var),
        ExprKind::CommandSubstitution(parts) => command_substitution_type(parts, env),
        ExprKind::ParameterExpansion(pe) => parameter_expansion_type(pe, env),
        // DD#76: a subscript of an int-valued collection is an Int element
        // (`${a[i]}` / `${m[k]}` on an `int array`/`int map`). The collection
        // name carries the element type (int arrays declare name→Int); a
        // string collection's element stays String.
        ExprKind::Subscript { name, .. } if env.var_type(name) == VarType::Int => VarType::Int,
        // Literal / DoubleQuoted / SingleQuoted / everything else → String.
        // Bash treats bare numeric literals as strings; only declare-int or
        // $(()) makes a value int.
        _ => VarType::String,
    }
}

/// `$(cmd)`: int when `cmd` is one of the compiler intrinsics that
/// returns an int, otherwise the recorded function type (defaults to
/// String for unknown commands).
fn command_substitution_type(parts: &[Statement], env: &TypeEnv) -> VarType {
    let Some(first) = parts.first() else {
        return VarType::String;
    };
    let StmtKind::Command(c) = &first.kind else {
        return VarType::String;
    };
    let name = expr_to_name(&c.name);
    if is_int_returning_intrinsic(&name) {
        return VarType::Int;
    }
    env.func_type(&name)
}

/// A `$(…)` command substitution is int-typed when its command is one of
/// the sash-native value functions that yield an integer: `len`/`argc`
/// (counts), `random` (i64 draw), `dtoi` (fixed-point decimal→int),
/// `epochseconds` (unix time), `signum` (DD#56 last-signal number), `status`
/// (`$?`), and the `getpid`/`getppid`/`getcpid` process ids. String-valued
/// functions (`toupper`, `join`, `basename`, `itod`, …) are not listed, so
/// `int n=$(toupper s)` stays a type error (SB4030).
fn is_int_returning_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "len"
            | "argc"
            | "random"
            | "signum"
            | "dtoi"
            | "epochseconds"
            | "status"
            | "getpid"
            | "getppid"
            | "getcpid"
    )
}

/// DD#58/Option C: reject a string producer used in an arithmetic sexpr. The
/// sexpr `(f …)` position expects an INT producer (an int intrinsic or a user
/// `-> int` function); a string producer there is a type error — it belongs in
/// a `$()` capture. Walks every `$(( ))` expr and bare `(( ))` test in the
/// statement (and its nested bodies).
/// Every `$(( … ))` expression in `stmt`, checked with the DD#93 Part D
/// attest predicates exempted from the disposition rule.
fn check_stmt_arith_exprs(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let attest_preds = crate::validate::attest_arith_predicates(stmt);
    walk_all_exprs(stmt, &mut |e| {
        if let ExprKind::Arithmetic(a) = &e.kind {
            let exempt = attest_preds.contains(&(e as *const Expr));
            check_arith_expr_rules(a, e.span, exempt, env, errors);
        }
    });
}

/// The arithmetic rules that apply to one `$(( … ))` / `(( … ))` tree.
///
/// `disposition_exempt` skips the DD#77 D1 bare-string rule for a DD#93 Part D
/// attest predicate: the bare source needs no `${s:-N}` / `${s:?msg}` there
/// precisely because the `attest` supplies the accountability — that is the
/// whole point of the form, and the conversion is gated on `i?`.
fn check_arith_expr_rules(
    a: &ArithExpr,
    span: Span,
    disposition_exempt: bool,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    check_arith_sexpr(a, span, env, errors);
    if !disposition_exempt {
        check_arith_string_operands(a, span, env, errors);
    }
    check_arith_default_literals(a, errors);
    check_arith_bare_map_subscript(a, span, env, errors);
}

fn check_sexpr_string_producers(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    // DD#93 Part D: an attest's `(( TARGET = SOURCE ))` predicate is exempt
    // from the DD#77 D1 disposition rule. The bare source needs no `${s:-N}` /
    // `${s:?msg}` precisely because the `attest` supplies the accountability —
    // that is the whole point of the form, and the conversion is gated on `i?`
    // so it cannot misread or overflow.
    check_stmt_arith_exprs(stmt, env, errors);
    if let StmtKind::ArithTest(a) = &stmt.kind {
        check_arith_expr_rules(a, stmt.span, false, env, errors);
    }
    // arith-subscript model B: `(( m[k] = … ))` lowers to an assignment with an
    // *arithmetic* index; a map target there is the bare-`m[k]` write form.
    if let StmtKind::Assignment(a) = &stmt.kind {
        check_arith_indexed_map_assign(a, stmt.span, env, errors);
    }
    for body in stmt.child_bodies() {
        for s in body {
            check_sexpr_string_producers(s, env, errors);
        }
    }
}

/// Reject `(( m[k] = … ))` on a map: its lowered assignment carries an
/// `Arithmetic` index (vs the statement `m[k]=…`, whose index is a normal
/// word) — the bare-subscript write form, invalid for a string-keyed map.
fn check_arith_indexed_map_assign(
    a: &Assignment,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    let Some(idx) = &a.index else { return };
    if matches!(idx.kind, ExprKind::Arithmetic(_)) && env.assoc_arrays.contains(&a.name) {
        errors.push(bare_map_subscript_error(&a.name, span));
    }
}

/// arith-subscript model B / DD#77: a **bare** subscript cell in arithmetic.
/// A map cell (`m[k]`) is rejected — a string key isn't expressible under arith
/// rules. A **string-array** cell (`a[i]` where `a`'s elements are strings) is
/// rejected like a bare string scalar (DD#77 D1): the string→int crossing must
/// carry a disposition. An **int-array** cell reads its raw i64 and is fine.
fn check_arith_bare_map_subscript(
    a: &ArithExpr,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    for_each_arith_subscript(a, &mut |name| {
        if env.assoc_arrays.contains(name) {
            errors.push(bare_map_subscript_error(name, span));
        } else if env.is_collection(name) && env.var_type(name) == VarType::String {
            errors.push(bare_string_cell_error(name, span));
        }
    });
}

/// DD#77 D1 for a string-array cell: the same discipline as a bare string
/// scalar — the cell value is a string, so its use as an arithmetic operand
/// must carry a disposition. The `${a[i]}` read + `:-`/`:?` disposition already
/// works, so this directs there.
fn bare_string_cell_error(name: &str, span: Span) -> TypeError {
    TypeError {
        message: format!(
            "bare string cell `{name}[…]` in arithmetic — `{name}` holds strings, so \
             (like a bare string variable, DD#77) the string→int conversion must carry \
             a disposition: `${{{name}[i]:-N}}` (default N if not an integer) or \
             `${{{name}[i]:?msg}}` (abort if not an integer)."
        ),
        span,
    }
}

fn bare_map_subscript_error(name: &str, span: Span) -> TypeError {
    TypeError {
        message: format!(
            "bare `{name}[…]` in arithmetic — `{name}` is a map, and a string key \
             isn't expressible under arithmetic rules. Read with `${{{name}[k]}}` \
             (imports the value) and write with `{name}[k]=…` (statement form)."
        ),
        span,
    }
}

/// Invoke `f(name)` for every bare subscript (`ArithExpr::Subscript` /
/// `SubscriptAssign`) name in an arith tree, descending into every sub-expr.
fn for_each_arith_subscript(a: &ArithExpr, f: &mut impl FnMut(&str)) {
    match a {
        ArithExpr::Subscript { name, index } => {
            f(name);
            for_each_arith_subscript(index, f);
        }
        ArithExpr::SubscriptAssign {
            name, index, value, ..
        } => {
            f(name);
            for_each_arith_subscript(index, f);
            for_each_arith_subscript(value, f);
        }
        ArithExpr::BinOp { left, right, .. } => {
            for_each_arith_subscript(left, f);
            for_each_arith_subscript(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            for_each_arith_subscript(operand, f)
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            for_each_arith_subscript(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            for_each_arith_subscript(cond, f);
            for_each_arith_subscript(true_val, f);
            for_each_arith_subscript(false_val, f);
        }
        ArithExpr::Comma(exprs) | ArithExpr::FuncCall { args: exprs, .. } => {
            for e in exprs {
                for_each_arith_subscript(e, f);
            }
        }
        _ => {}
    }
}

/// Walk an `ArithExpr` tree; for every sexpr `FuncCall` whose callee produces a
/// string (or is dtoi/itod — Q2), emit a type error.
fn check_arith_sexpr(a: &ArithExpr, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    walk_arith_funccalls(a, &mut |name| {
        if is_sexpr_string_producer(name, env) {
            errors.push(TypeError {
                message: format!(
                    "`{name}` can't be called in an arithmetic sexpr — it yields a \
                     string, not an int. Capture it first with `$()`: `s=$({name} …)`, \
                     then use `s` (or an `int` of it) in `$(( … ))`."
                ),
                span,
            });
        }
    });
}

/// Invoke `f(name)` for each `FuncCall` name in an `ArithExpr` tree.
fn walk_arith_funccalls(a: &ArithExpr, f: &mut impl FnMut(&str)) {
    match a {
        ArithExpr::FuncCall { name, args } => {
            f(name);
            for arg in args {
                walk_arith_funccalls(arg, f);
            }
        }
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_funccalls(left, f);
            walk_arith_funccalls(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } => walk_arith_funccalls(operand, f),
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_funccalls(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_funccalls(cond, f);
            walk_arith_funccalls(true_val, f);
            walk_arith_funccalls(false_val, f);
        }
        ArithExpr::Comma(exprs) => {
            for e in exprs {
                walk_arith_funccalls(e, f);
            }
        }
        ArithExpr::Paren(inner) => walk_arith_funccalls(inner, f),
        // Leaves (Literal, Var, DollarVar, Pre/PostIncDec) hold no FuncCall.
        _ => {}
    }
}

/// DD#77 D1: invoke `f(name)` for each bare variable *read* (`x` / `$x`) in an
/// `ArithExpr` tree. Does NOT descend into `Expansion` (a `${…}` disposition or
/// subscript is not a bare operand) and does NOT visit assignment/inc-dec
/// targets (those are mutation lvalues, tracked as arith-native ints).
fn walk_arith_var_reads(a: &ArithExpr, f: &mut impl FnMut(&str)) {
    match a {
        ArithExpr::Var(name) | ArithExpr::DollarVar(name) => f(name),
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_var_reads(left, f);
            walk_arith_var_reads(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            walk_arith_var_reads(operand, f)
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_var_reads(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_var_reads(cond, f);
            walk_arith_var_reads(true_val, f);
            walk_arith_var_reads(false_val, f);
        }
        ArithExpr::Comma(exprs) => {
            for e in exprs {
                walk_arith_var_reads(e, f);
            }
        }
        // Descend into a bare subscript's *index* (and a subscript-assign's
        // index/value): the index is itself an arithmetic operand, so a bare
        // string used there (`a[s]`) is the same string→int crossing (DD#77 D1).
        // The array *name* is handled by the bare-subscript cell check, not
        // here.
        ArithExpr::Subscript { index, .. } => walk_arith_var_reads(index, f),
        ArithExpr::SubscriptAssign { index, value, .. } => {
            walk_arith_var_reads(index, f);
            walk_arith_var_reads(value, f);
        }
        // NOT descended: `FuncCall` args (e.g. `${#s}` = `len(s)` passes the
        // *name*, not an int operand — flagging it would be a false positive);
        // `Expansion` (a `${…}` disposition/subscript); `Literal` /
        // Pre/PostIncDec (no bare read to flag).
        _ => {}
    }
}

/// DD#77 D1: a bare string-typed variable used as an arithmetic operand is a
/// compile error — the string→int crossing must carry a disposition
/// (`${s:-N}` default / `${s:?msg}` abort). Only KNOWN-string vars are flagged;
/// arith-native counters (`for (( i=0; … ))`, `(( x = 5 ))`) and undeclared
/// names are untyped and pass.
fn check_arith_string_operands(
    a: &ArithExpr,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    walk_arith_var_reads(a, &mut |name| {
        if env.vars.contains_key(name) && env.var_type(name) == VarType::String {
            errors.push(TypeError {
                message: format!(
                    "bare string `{name}` in arithmetic — a string→int conversion must \
                     carry a disposition: `${{{name}:-N}}` (default N if not an integer) \
                     or `${{{name}:?msg}}` (abort if not an integer)."
                ),
                span,
            });
        }
    });
}

/// DD#77 D4: invoke `f(inner)` for every `${…}` expansion (`ArithExpr::
/// Expansion`) in an `ArithExpr` tree. Complements `walk_arith_var_reads`,
/// which deliberately skips these — here we want them, to validate a `:-`
/// disposition's default.
fn walk_arith_expansions(a: &ArithExpr, f: &mut impl FnMut(&Expr)) {
    match a {
        ArithExpr::Expansion(inner) => f(inner),
        ArithExpr::FuncCall { args, .. } => {
            for arg in args {
                walk_arith_expansions(arg, f);
            }
        }
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_expansions(left, f);
            walk_arith_expansions(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            walk_arith_expansions(operand, f)
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_expansions(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_expansions(cond, f);
            walk_arith_expansions(true_val, f);
            walk_arith_expansions(false_val, f);
        }
        ArithExpr::Comma(exprs) => {
            for e in exprs {
                walk_arith_expansions(e, f);
            }
        }
        _ => {}
    }
}

/// DD#77 D4: in arithmetic, a `${s:-DEFAULT}` disposition's DEFAULT must be a
/// decimal integer literal. The default is lowered by a plain decimal atoi, so
/// a non-decimal default (`hello`, `0xff`, `5+3`, an interpolation, or empty)
/// silently became `0` — reject it instead (SB4030). `:?` carries a message,
/// not a numeric default, so it is never checked here.
fn check_arith_default_literals(a: &ArithExpr, errors: &mut Vec<TypeError>) {
    walk_arith_expansions(a, &mut |inner| {
        let ExprKind::ParameterExpansion(pe) = &inner.kind else {
            return;
        };
        if pe.operator.as_deref() != Some(":-") {
            return;
        }
        if !default_is_arith_literal(pe.operand.as_deref()) {
            let name = &pe.name;
            errors.push(TypeError {
                message: format!(
                    "arithmetic default for `{name}` must be an integer literal \
                     (decimal or `0x`/`0o`/`0b`, e.g. `${{{name}:-0}}` / `${{{name}:-0xff}}`) — \
                     it is lowered as an arithmetic constant, so a non-literal default \
                     (an interpolation, `base#digits`, or leading-zero octal) is not allowed."
                ),
                span: inner.span,
            });
        }
    });
}

/// The `${var:-DEFAULT}` default word (a `DoubleQuoted`, per DD#65) is a
/// decimal integer literal iff it is a single literal part matching an optional
/// `-` then a canonical decimal (`0`, or a non-zero leading digit). Rejects
/// leading zeros (DD#72), radix prefixes, interpolations, and empty.
fn default_is_arith_literal(operand: Option<&Expr>) -> bool {
    let Some(op) = operand else {
        return false;
    };
    let lit = match &op.kind {
        ExprKind::Literal(s) => Some(s.as_str()),
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => match &parts[0].kind {
            ExprKind::Literal(s) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    };
    // DD#72: accept the full arith literal grammar (decimal + 0x/0o/0b), shared
    // with the lowerer via `parse_dd72_literal`, not just decimal.
    lit.and_then(crate::parse::parse_dd72_literal).is_some()
}

/// DG (2026-08-29): a `${x:-D}` default in *value* context, where the subject
/// `x` is int-typed (an `int` scalar or an int-array cell), must be an int
/// literal — the same discipline as an arithmetic default
/// (`check_arith_default_literals`). An int `:-` triggers on the additive
/// identity `0` (the int analogue of `""`), lowering to `x == 0 ? D : x`; a
/// non-literal `D` (a string `hello`, an interpolation `$y`, `0xZZ`, or an
/// empty default) would make the disposition mixed-type, so reject it (SB4030).
/// Arithmetic-context PEs are owned by `check_arith_default_literals`, so this
/// walk treats `$(( … ))` as opaque (no duplicate diagnostic).
fn check_int_value_defaults(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    stmt.direct_exprs(&mut |e| walk_value_default(e, env, errors));
    for body in stmt.child_bodies() {
        for s in body {
            check_int_value_defaults(s, env, errors);
        }
    }
}

/// Walk a value-context expression tree, flagging each int-subject `${x:-D}`
/// with a non-literal default. Mirrors `walk_expr_tree` but stops at
/// `ExprKind::Arithmetic` (owned by `check_arith_default_literals`). A thin
/// dispatcher — each arm delegates so the traversal stays flat.
fn walk_value_default(expr: &Expr, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    match &expr.kind {
        ExprKind::ParameterExpansion(pe) => walk_pe_default(pe, expr.span, env, errors),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            walk_value_slice(parts, env, errors)
        }
        ExprKind::AssocArray(pairs) => walk_value_pairs(pairs, env, errors),
        ExprKind::CommandSubstitution(stmts) => walk_value_cmdsub(stmts, env, errors),
        ExprKind::Subscript { index, .. } => walk_value_default(index, env, errors),
        ExprKind::Test(t) => walk_value_test(t, env, errors),
        // `$(( … ))` defaults are owned by check_arith_default_literals.
        ExprKind::Arithmetic(_) => {}
        _ => {}
    }
}

/// Check a `${…}` node, then descend into its default operand.
fn walk_pe_default(
    pe: &ParameterExpansion,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    check_int_pe_default(pe, span, env, errors);
    if let Some(op) = pe.operand.as_deref() {
        walk_value_default(op, env, errors);
    }
}

/// Recurse into each expr of a slice (`DoubleQuoted` / `Concat` / `Array`).
fn walk_value_slice(parts: &[Expr], env: &TypeEnv, errors: &mut Vec<TypeError>) {
    for p in parts {
        walk_value_default(p, env, errors);
    }
}

/// Recurse into each value of an assoc-array literal.
fn walk_value_pairs(pairs: &[(String, Expr)], env: &TypeEnv, errors: &mut Vec<TypeError>) {
    for (_, v) in pairs {
        walk_value_default(v, env, errors);
    }
}

/// Recurse into each statement of a `$(…)` command substitution.
fn walk_value_cmdsub(stmts: &[Statement], env: &TypeEnv, errors: &mut Vec<TypeError>) {
    for s in stmts {
        check_int_value_defaults(s, env, errors);
    }
}

/// Descend a `[[ … ]]` test tree into its operand expressions.
fn walk_value_test(t: &TestExpr, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    match t {
        TestExpr::Unary { operand, .. } => walk_value_default(operand, env, errors),
        TestExpr::Binary { left, right, .. } => {
            walk_value_default(left, env, errors);
            walk_value_default(right, env, errors);
        }
        TestExpr::Not(inner) => walk_value_test(inner, env, errors),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            walk_value_test(l, env, errors);
            walk_value_test(r, env, errors);
        }
    }
}

/// Flag a single `${x:-D}` whose int subject carries a non-literal default.
fn check_int_pe_default(
    pe: &ParameterExpansion,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    if pe.operator.as_deref() != Some(":-") {
        return;
    }
    if env.var_type(&pe.name) != VarType::Int {
        return;
    }
    if default_is_arith_literal(pe.operand.as_deref()) {
        return;
    }
    let name = &pe.name;
    errors.push(TypeError {
        message: format!(
            "default for int `{name}` must be an integer literal (decimal or \
             `0x`/`0o`/`0b`, e.g. `${{{name}:-0}}`) — an int `:-` default triggers on \
             the identity element `0`, so the fallback must itself be an int, not a \
             string, interpolation, or empty default."
        ),
        span,
    });
}

/// A sexpr callee that is NOT a valid int producer: a string-returning intrinsic
/// (or dtoi/itod — Q2: capture via `$()`), or a user string/void function. Int
/// intrinsics (`len`, `getpid`, `argc`, …), user `-> int` functions, and a bare
/// variable `(x)` (arithmetic grouping) are all valid and return `false`.
fn is_sexpr_string_producer(name: &str, env: &TypeEnv) -> bool {
    const STRING_INTRINSICS: &[&str] = &[
        "toupper", "tolower", "join", "keys", "basename", "dirname", "getcwd", "getenv", "arg",
        "sub", "gsub", "match", "itod", "dtoi",
    ];
    STRING_INTRINSICS.contains(&name) || matches!(env.funcs.get(name), Some(VarType::String))
}

/// `${var:-default}` / `${var-default}` preserve the variable's type;
/// all other strip ops (`#`, `%`, `/`, etc.) produce strings.
fn parameter_expansion_type(pe: &ParameterExpansion, env: &TypeEnv) -> VarType {
    let Some(op) = pe.operator.as_deref() else {
        return env.var_type(&pe.name);
    };
    match op {
        // `${var:-default}` / `${var-default}` preserve the variable's type.
        ":-" | "-" => env.var_type(&pe.name),
        // DD#75 §2: a length (`${#x}` → strlen/arrlen) is an int.
        "strlen" | "arrlen" => VarType::Int,
        // All other strip ops (`%`, `#`, `/`, …) produce strings.
        _ => VarType::String,
    }
}

fn expr_to_name(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => s.clone(),
        ExprKind::SimpleExpansion(v) => v.clone(),
        _ => String::new(),
    }
}

/// Type-check a program. Returns a list of type errors (empty = OK).
pub fn type_check(program: &Program) -> Vec<TypeError> {
    let mut errors = Vec::new();
    let mut env = TypeEnv::new();

    // First pass: collect function types
    for stmt in &program.statements {
        if let StmtKind::Function(f) = &stmt.kind {
            env.funcs.insert(f.name.clone(), f.return_type);
        }
    }

    // Second pass: check statements
    for stmt in &program.statements {
        check_stmt(stmt, &mut env, &mut errors);
    }

    // Third pass (DD#58/Option C): a string producer used in an arithmetic
    // sexpr `(f …)` is a type error — it belongs in a `$()` capture, not
    // `$(( ))`. Covers the string intrinsics + dtoi/itod (Q2) + user string
    // functions; closes the getenv/arg validator-bypass (they silently compiled
    // to 0 in a sexpr, sidestepping SB5020). bug-arith-sexpr-intrinsics-zero.
    for stmt in &program.statements {
        check_sexpr_string_producers(stmt, &env, &mut errors);
    }

    // Fourth pass (DG 2026-08-29): a value-context `${int_x:-D}` default must be
    // an int literal (SB4030) — see check_int_value_defaults. Separate from the
    // arith pass, which owns `$(( … ))` defaults.
    for stmt in &program.statements {
        check_int_value_defaults(stmt, &env, &mut errors);
    }

    errors
}

/// DD#33: Check expressions for subscript key type mismatches.
fn check_subscript_types(expr: &Expr, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    match &expr.kind {
        ExprKind::Subscript { name, index } => {
            check_subscript_key_kind(name, index, expr.span, env, errors);
            check_subscript_types(index, env, errors);
        }
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            check_subscripts_in_parts(parts, env, errors);
        }
        ExprKind::CommandSubstitution(stmts) => {
            for s in stmts {
                check_subscript_in_stmt(s, env, errors);
            }
        }
        _ => {}
    }
}

fn check_subscripts_in_parts(parts: &[Expr], env: &TypeEnv, errors: &mut Vec<TypeError>) {
    for p in parts {
        check_subscript_types(p, env, errors);
    }
}

/// Reject integer keys on maps (looks like indexed access) and string
/// keys on indexed arrays.
fn check_subscript_key_kind(
    name: &str,
    index: &Expr,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    if !env.collections.contains(name) {
        return;
    }
    let is_map = env.assoc_arrays.contains(name);
    if is_map && is_integer_literal(index) {
        errors.push(TypeError {
            message: format!(
                "integer key on map '{}' — maps use string keys. Use ${{{}[\"key\"]}}",
                name, name,
            ),
            span,
        });
    } else if !is_map && is_string_key(index) {
        errors.push(TypeError {
            message: format!(
                "string key on indexed array '{name}' — an int-indexed array is indexed by \
                 number (DD#80). Use an integer, `${{{name}[$i]}}`, or `$(( … ))`; declare \
                 `map {name}` for string keys",
            ),
            span,
        });
    }
}

fn check_subscript_in_stmt(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            check_subscript_types(&c.name, env, errors);
            check_join_operand(c, stmt.span, env, errors);
            check_keys_operand(c, stmt.span, env, errors);
            for arg in &c.args {
                check_subscript_types(arg, env, errors);
            }
        }
        StmtKind::Assignment(a) => {
            check_subscript_types(&a.value, env, errors);
        }
        _ => {}
    }
}

/// DD#75 §3: `keys` is map-only. An indexed array's keys are trivially
/// `0..len-1` and a scalar has none, so `keys <non-map>` is a compile error
/// (was blank/null output for an indexed array). Only maps carry string keys.
fn check_keys_operand(c: &Command, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    if expr_to_name(&c.name) != "keys" {
        return;
    }
    let Some(arg) = c.args.first() else {
        return;
    };
    let name = join_operand_name(arg);
    if name.is_empty() || env.assoc_arrays.contains(&name) {
        return;
    }
    let what = if env.collections.contains(&name) {
        "an indexed array — its keys are 0..len-1; iterate with \
         `for (( i=0; i < $(len a); i++ ))` or values with `@a`"
    } else {
        "not a map — only maps have keys"
    };
    errors.push(TypeError {
        message: format!("`keys` needs a map — `{name}` is {what}"),
        span,
    });
}

/// DD#75: `join`'s array operand (the first argument — `join arr [sep]`)
/// must be a declared array/map. A scalar / undefined / literal operand
/// would segfault in codegen (`arrjoin` on a non-table), so reject it here.
fn check_join_operand(c: &Command, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    if expr_to_name(&c.name) != "join" {
        return;
    }
    let Some(arr) = c.args.first() else {
        return;
    };
    let name = join_operand_name(arr);
    if name.is_empty() || env.collections.contains(&name) {
        return;
    }
    errors.push(TypeError {
        message: format!(
            "`join` needs an array or map as its first operand — `{name}` is not a \
             collection; declare it with `array`/`map` (or splat a collection with `@{name}`)"
        ),
        span,
    });
}

/// The collection name a `join` operand refers to: `@a` (arrvals splat) → `a`;
/// a bare name → itself; anything else → empty (not a simple name operand).
pub(crate) fn join_operand_name(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            pe.name.clone()
        }
        ExprKind::Literal(s) => s.clone(),
        _ => String::new(),
    }
}

/// Check if an expression is a plain string literal (non-numeric)
fn is_string_key(expr: &Expr) -> bool {
    match &expr.kind {
        // a non-numeric bareword literal is a string key
        ExprKind::Literal(s) => !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit()),
        // an explicitly quoted key is a string — even a quoted numeric `"5"`
        // (DD#80: use the unquoted integer `${a[5]}` to index an int array)
        ExprKind::DoubleQuoted(_) | ExprKind::SingleQuoted(_) => true,
        ExprKind::SimpleExpansion(_) => false, // variable — could be either
        _ => false,
    }
}

/// Check if an expression is an integer literal
fn is_integer_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(s) => !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

fn check_stmt(stmt: &Statement, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    match &stmt.kind {
        StmtKind::Assignment(a) => check_assignment(a, stmt.span, env, errors),
        StmtKind::Declaration(d) => check_declaration(d, stmt.span, env, errors),
        StmtKind::Command(c) => check_command(c, stmt.span, env, errors),
        StmtKind::If(if_stmt) => check_if(if_stmt, env, errors),
        StmtKind::For(f) => check_for(f, env, errors),
        StmtKind::Function(f) => check_function(f, env, errors),
        // Default: descend into every nested statement body via
        // child_bodies(). Covers For, While, Function, Case (no
        // per-arm pre-work) and the wrapping StmtKinds Pipeline,
        // Group, List, Subshell, Redirected, Suppressed — so
        // wrapping a statement in grouping syntax doesn't silently
        // bypass type checking (journal 056). child_bodies()
        // returns empty for leaf StmtKinds (Assignment,
        // Declaration, Command, Comment, ArithTest), which all
        // have their own arm above.
        _ => {
            for body in stmt.child_bodies() {
                for s in body {
                    check_stmt(s, env, errors);
                }
            }
        }
    }
}

fn check_assignment(a: &Assignment, span: Span, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    // DD#75 §4: a subscript index is a numeric literal or a `$`-sigilled
    // value; a bare word is a string key (map-only). Enforce on the write
    // path as on the read path (check_subscript_key_kind), so `a[i]=…` on an
    // indexed array is SB4030 just like the read `${a[i]}`.
    if let Some(idx) = &a.index {
        check_subscript_key_kind(&a.name, idx, span, env, errors);
    }
    let rtype = expr_type(&a.value, env);
    let ltype = env.var_type(&a.name);
    let known = env.vars.contains_key(&a.name);
    // Bare assignment: a new variable gets the RHS type on first use.
    // An already-typed variable must match — except (DD#30 refinement) an
    // already-int LHS accepts a bare numeric literal (`int x=0; x=5` aligns
    // with `int x=5`; `int array a=(…); a[i]=99` matches int-init), mirroring
    // the declaration-init `is_numeric_literal` exception. The variable keeps
    // its declared int type (no re-declare).
    if known && ltype != rtype {
        let int_literal_ok = ltype == VarType::Int && is_numeric_literal(&a.value);
        if !int_literal_ok {
            errors.push(TypeError {
                message: format!(
                    "cannot assign {} value to {} variable `{}`{}",
                    type_name(rtype),
                    type_name(ltype),
                    a.name,
                    conversion_hint(rtype, ltype),
                ),
                span,
            });
        }
    } else if !known {
        env.declare(&a.name, rtype);
    }
}

fn check_declaration(d: &Declaration, span: Span, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    let is_collection = is_collection_decl(d);
    for a in &d.assignments {
        check_one_decl_assignment(d, a, is_collection, span, env, errors);
        env.declare(&a.name, d.var_type);
    }
    if is_collection {
        track_collection_names(d, env);
    }
}

fn is_collection_decl(d: &Declaration) -> bool {
    d.flags.iter().any(|f| f == "-a" || f == "-A")
}

/// Type-check one assignment of a declaration: an int-collection element
/// (numeric), or a scalar (int↔string). A string collection's elements are
/// unconstrained strings.
fn check_one_decl_assignment(
    d: &Declaration,
    a: &crate::ir::Assignment,
    is_collection: bool,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    if !is_collection {
        check_scalar_assignment(d, a, span, env, errors);
    } else if d.var_type == VarType::Int {
        check_int_collection_elements(&a.value, &a.name, span, env, errors);
    }
}

/// Register a collection declaration's names (and their assoc-ness) in the env.
fn track_collection_names(d: &Declaration, env: &mut TypeEnv) {
    let is_assoc = d.flags.iter().any(|f| f == "-A");
    for a in &d.assignments {
        env.collections.insert(a.name.clone());
        if is_assoc {
            env.assoc_arrays.insert(a.name.clone());
        }
    }
}

/// Scalar `int x=…` / `string x=…` type check (string↔int mismatch).
fn check_scalar_assignment(
    d: &Declaration,
    a: &crate::ir::Assignment,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    let rtype = expr_type(&a.value, env);
    if d.var_type == VarType::Int && rtype == VarType::String {
        // String assigned to int variable — need conversion.
        // Exception: literal numbers in quotes are OK (common pattern).
        if !is_numeric_literal(&a.value) {
            errors.push(TypeError {
                message: format!(
                    "cannot assign string value to int variable `{}`{}",
                    a.name,
                    conversion_hint(VarType::String, VarType::Int),
                ),
                span,
            });
        }
    } else if d.var_type == VarType::String && rtype == VarType::Int {
        errors.push(TypeError {
            message: format!(
                "cannot assign int value to string variable `{}`{}",
                a.name,
                conversion_hint(VarType::Int, VarType::String),
            ),
            span,
        });
    }
}

/// DD#76: every element of an `int array`/`int map` literal must be an integer
/// (a numeric literal, an int var, `$(( … ))`, or an int-returning `$(f)`).
fn check_int_collection_elements(
    value: &Expr,
    name: &str,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    let elems: Vec<&Expr> = match &value.kind {
        ExprKind::Array(es) => es.iter().collect(),
        ExprKind::AssocArray(pairs) => pairs.iter().map(|(_, v)| v).collect(),
        _ => return,
    };
    for e in elems {
        if !is_numeric_literal(e) && expr_type(e, env) != VarType::Int {
            errors.push(TypeError {
                message: format!(
                    "non-numeric element in int collection `{}` — an int array/map holds integers",
                    name,
                ),
                span,
            });
        }
    }
}

fn check_command(c: &Command, span: Span, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    // DD#60/DD#61: argv/mapfile/read auto-declare their target collection —
    // register it so a later attest-all / len / element read sees a collection,
    // not a scalar (the front-end analogue of the transform's auto-declare).
    match expr_to_name(&c.name).as_str() {
        "unset" => check_unset_args(c, span, env, errors),
        "attest" => check_attest_form(c, span, env, errors),
        "argv" => register_argv_target(c, env),
        "mapfile" | "readarray" => register_bulk_read_target(c, env),
        "read" => register_read_array_target(c, env),
        _ => {}
    }

    // DD#33: check subscript key types in command args
    check_join_operand(c, span, env, errors);
    check_keys_operand(c, span, env, errors);
    for arg in &c.args {
        check_subscript_types(arg, env, errors);
    }
}

/// DD#60: `argv [N] a` fills (and auto-declares) the trailing array.
fn register_argv_target(c: &Command, env: &mut TypeEnv) {
    if let Some(arr) = c.args.last() {
        env.collections.insert(expr_to_name(arr));
    }
}

/// `read -a a` fills (auto-declares) an indexed array target.
fn register_read_array_target(c: &Command, env: &mut TypeEnv) {
    if let Some(name) = crate::ir::classify_read_flags(&c.args).array_name {
        env.collections.insert(name);
    }
}

/// Register a `mapfile`/`readarray` target as a collection (indexed array, or
/// an assoc array for `-m`) — the type-check analogue of the transform's
/// `autodeclare_bulk_target`. Without this, an auto-declared bulk-read array
/// (no prior `array a=()`) is seen as a scalar by attest-all / len / element
/// reads (SB4030), while `argv` — which does register — works.
fn register_bulk_read_target(c: &Command, env: &mut TypeEnv) {
    let args: Vec<String> = c.args.iter().map(expr_to_name).collect();
    let Some(name) = args.iter().find(|a| !a.starts_with('-')) else {
        return;
    };
    env.collections.insert(name.clone());
    if args.iter().any(|a| a == "-m") {
        env.assoc_arrays.insert(name.clone());
    }
}

/// DD#61: the attest form must match the attested entity's kind.
/// - scalar `attest v [[…]]` requires a scalar (arrays/maps need attest-all);
/// - one-binder `attest x in a [[…]]` requires an indexed array;
/// - two-binder `attest k v in m [[…]]` requires a map.
fn check_attest_form(c: &Command, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let Some((form, _)) = classify_attest(&c.args) else {
        return;
    };
    if let Some(message) = attest_form_error(&form, env) {
        errors.push(TypeError { message, span });
    }
}

/// Returns the form/kind-mismatch error message for a attest, or `None`
/// when the form matches the entity's declared kind.
fn attest_form_error(form: &AttestForm, env: &TypeEnv) -> Option<String> {
    match form {
        AttestForm::Scalar { var } => attest_scalar_form_error(var, env),
        AttestForm::Indexed { binder, array } => attest_indexed_form_error(binder, array, env),
        AttestForm::Map { key, val, map } => attest_map_form_error(key, val, map, env),
    }
}

fn attest_scalar_form_error(var: &str, env: &TypeEnv) -> Option<String> {
    if !env.collections.contains(var) {
        return None;
    }
    let (kind, binders) = if env.assoc_arrays.contains(var) {
        ("map", "k v")
    } else {
        ("array", "x")
    };
    Some(format!(
        "scalar attest on {kind} '{var}' — use attest-all: `attest {binders} in {var} [[ … ]]`"
    ))
}

fn attest_indexed_form_error(binder: &str, array: &str, env: &TypeEnv) -> Option<String> {
    if !env.collections.contains(array) {
        Some(format!(
            "`attest {binder} in {array}` requires an array or map — '{array}' is a scalar; \
             use scalar attest `attest {array} [[ … ]]`"
        ))
    } else if env.assoc_arrays.contains(array) {
        Some(format!(
            "'{array}' is a map — attest-all needs two binders: `attest {binder} v in {array} [[ … ]]`"
        ))
    } else {
        None
    }
}

fn attest_map_form_error(key: &str, val: &str, map: &str, env: &TypeEnv) -> Option<String> {
    if env.assoc_arrays.contains(map) {
        return None;
    }
    let what = if env.collections.contains(map) {
        "an indexed array — use one binder"
    } else {
        "a scalar — use scalar attest"
    };
    Some(format!(
        "`attest {key} {val} in {map}` requires a map — '{map}' is {what}: \
         `attest {key} in {map} [[ … ]]`"
    ))
}

/// DD#30: unset on a scalar variable (int or string, not declared
/// as -a/-A) is rejected. Subscript forms like `unset name[i]` are
/// allowed.
fn check_unset_args(c: &Command, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    for arg in &c.args {
        let name = expr_to_name(arg);
        // Names containing '[' are subscript forms — allowed.
        if name.contains('[') || name.is_empty() {
            continue;
        }
        let vtype = env.var_type(&name);
        if vtype != VarType::String && vtype != VarType::Int {
            continue;
        }
        // Declared as array/map → OK
        if env.is_collection(&name) {
            continue;
        }
        errors.push(TypeError {
            message: format!(
                "unset on scalar '{}' is not allowed — use {}=\"\" to clear",
                name, name,
            ),
            span,
        });
    }
}

fn check_for(f: &For, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    // The for-in header words are expressions, not a child body — visit them so
    // a `$(keys …)` / `$(join …)` splat source (or a bad subscript) in the
    // header is type-checked (DD#75 §3), then descend into the loop body.
    for w in &f.words {
        check_subscript_types(w, env, errors);
    }
    for s in &f.body {
        check_stmt(s, env, errors);
    }
}

/// DD#75 §5: inside a function body the implicit `args` array is a live
/// collection (so `len`/`join args`, `@args`, `${args[i]}` type-check as an
/// indexed array). Register it for the body, then restore — a nested/sibling
/// scope or top level must not see it (top-level `args` is a separate reject).
fn check_function(f: &Function, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    let had_args = env.collections.contains("args");
    env.collections.insert("args".to_string());
    for s in &f.body {
        check_stmt(s, env, errors);
    }
    if !had_args {
        env.collections.remove("args");
    }
}

fn check_if(if_stmt: &If, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    // Check condition for int-function-in-boolean-context
    check_condition_type(&if_stmt.condition, env, errors);
    for s in &if_stmt.condition {
        check_stmt(s, env, errors);
    }
    for s in &if_stmt.then_body {
        check_stmt(s, env, errors);
    }
    for elif in &if_stmt.elif_clauses {
        for s in &elif.condition {
            check_stmt(s, env, errors);
        }
        for s in &elif.body {
            check_stmt(s, env, errors);
        }
    }
    if let Some(else_body) = &if_stmt.else_body {
        for s in else_body {
            check_stmt(s, env, errors);
        }
    }
}

/// Check if a condition block uses an int function in boolean context.
fn check_condition_type(condition: &[Statement], env: &TypeEnv, errors: &mut Vec<TypeError>) {
    for stmt in condition {
        if let StmtKind::Command(c) = &stmt.kind {
            let name = expr_to_name(&c.name);
            if env.func_type(&name) == VarType::Int {
                errors.push(TypeError {
                    message: format!(
                        "int function `{}` used in boolean context — \
                         int functions return values, not success/failure",
                        name,
                    ),
                    span: stmt.span,
                });
            }
        }
    }
}

fn is_numeric_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(s) => s.chars().all(|c| c.is_ascii_digit() || c == '-'),
        _ => false,
    }
}

fn type_name(t: VarType) -> &'static str {
    match t {
        VarType::Int => "int",
        VarType::String => "string",
    }
}

fn conversion_hint(from: VarType, to: VarType) -> String {
    match (from, to) {
        (VarType::Int, VarType::String) => " — use `\"$var\"` to convert int to string".to_string(),
        (VarType::String, VarType::Int) => {
            " — use `$(($var))` to convert string to int".to_string()
        }
        _ => String::new(),
    }
}

/// Run type inference and return the variable type map.
pub fn infer_types(program: &Program) -> HashMap<String, VarType> {
    let mut env = TypeEnv::new();

    // Collect function types
    for stmt in &program.statements {
        if let StmtKind::Function(f) = &stmt.kind {
            env.funcs.insert(f.name.clone(), f.return_type);
        }
    }

    // Walk statements to discover variable types
    for stmt in &program.statements {
        infer_stmt(stmt, &mut env);
    }

    env.vars
}

/// DD#30 refinement: a bare numeric literal assigned to an already-int variable
/// keeps the int type (mirrors the error pass in check_assignment); otherwise
/// it would be re-typed String and mis-slotted in codegen.
fn infer_assignment_type(a: &crate::ir::Assignment, env: &TypeEnv) -> VarType {
    if env.var_type(&a.name) == VarType::Int && is_numeric_literal(&a.value) {
        VarType::Int
    } else {
        expr_type(&a.value, env)
    }
}

fn infer_stmt(stmt: &Statement, env: &mut TypeEnv) {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            env.declare(&a.name, infer_assignment_type(a, env));
        }
        StmtKind::Declaration(d) => infer_declaration_types(d, env),
        // Default: walk all child bodies. Covers For, While, Function
        // (no per-arm pre-work in this pass), plus every wrapping
        // StmtKind (Pipeline, Group, List, Subshell, Redirected,
        // Suppressed, If, Case) so a `int x=5` declared inside any
        // of them gets type-tracked. Bug parallel to journal 056 for
        // check_stmt: the original `_ => {}` silently dropped them.
        // child_bodies() returns empty for the leaves (Assignment,
        // Declaration, Command, Comment, ArithTest) which all have
        // explicit arms or no-effect semantics.
        _ => {
            for body in stmt.child_bodies() {
                for s in body {
                    infer_stmt(s, env);
                }
            }
        }
    }
}

/// `int x=…` declares with the explicit type; otherwise the RHS
/// expression is type-inferred per-assignment.
fn infer_declaration_types(d: &crate::ir::Declaration, env: &mut TypeEnv) {
    if d.var_type == VarType::Int {
        for a in &d.assignments {
            env.declare(&a.name, VarType::Int);
        }
    } else {
        for a in &d.assignments {
            let t = expr_type(&a.value, env);
            env.declare(&a.name, t);
        }
    }
}
