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

/// Environment tracking variable and function types.
struct TypeEnv {
    vars: HashMap<String, VarType>,
    funcs: HashMap<String, VarType>,
    /// Variables that are intrinsically int ($#, $?)
    intrinsic_int: HashSet<String>,
    /// Variables declared as array or map (collections)
    collections: HashSet<String>,
    /// DD#100: per-function collection-slot kinds, inferred from call sites.
    slot_kinds: HashMap<String, SlotKinds>,
    /// The slots whose kind is known while checking the current body. A slot
    /// outside this set is genuinely unknown (the function is never called
    /// with one, or the argument was undeclared), so kind checks skip it.
    known_slots: HashSet<String>,
    /// Variables declared as map (associative array) — string keys
    assoc_arrays: HashSet<String>,
    /// Scalars the author explicitly declared `str`. Distinct from
    /// `vars[name] == String`, which is also what an unrecorded name returns.
    declared_str: HashSet<String>,
}

impl TypeEnv {
    fn new() -> Self {
        let mut intrinsic_int = HashSet::new();
        intrinsic_int.insert("#".to_string()); // $#
        intrinsic_int.insert("?".to_string()); // $?
                                               // DD#100: `A`..`E` are the collection-parameter slots. They are always
                                               // collections, but their *kind* is per-call-site (Part B: array and
                                               // map are both usable), and this pass runs before inlining — so they
                                               // are seeded as collections and deliberately left out of
                                               // `assoc_arrays`, which would commit them to one kind program-wide.
                                               // SB5065 has already rejected any top-level use.
        let collections = (0..crate::ir::MAX_COLLECTION_SLOTS)
            .map(crate::ir::collection_slot_name)
            .collect();
        Self {
            vars: HashMap::new(),
            funcs: HashMap::new(),
            intrinsic_int,
            collections,
            slot_kinds: HashMap::new(),
            known_slots: HashSet::new(),
            assoc_arrays: HashSet::new(),
            declared_str: HashSet::new(),
        }
    }

    fn var_type(&self, name: &str) -> VarType {
        if self.intrinsic_int.contains(name) {
            VarType::Int
        } else {
            self.vars.get(name).copied().unwrap_or(VarType::String)
        }
    }

    /// Is `name` a scalar the author *declared* `str`?
    ///
    /// `var_type` cannot answer this: it ends in `unwrap_or(VarType::String)`,
    /// so "declared a string" and "never recorded" are the same answer. That
    /// conflation is why the subscript check below can only trust a positive
    /// declaration — a bare `i=1` records `String` exactly like `str s=abc`,
    /// because sash follows bash in typing a bare numeric literal as a string
    /// (only `int` or `$(( ))` makes a value int).
    fn is_declared_str(&self, name: &str) -> bool {
        self.declared_str.contains(name)
    }

    /// Record a `str` *scalar* declaration. `type_check` and `infer_types` are
    /// separate passes that each build their own env, so both call this — the
    /// condition lives here rather than being restated at either site.
    fn note_declared_str(&mut self, d: &Declaration) {
        if d.var_type == VarType::Int || is_collection_decl(d) {
            return;
        }
        for a in &d.assignments {
            self.declared_str.insert(a.name.clone());
        }
    }

    fn func_type(&self, name: &str) -> VarType {
        self.funcs.get(name).copied().unwrap_or(VarType::String)
    }

    fn is_collection(&self, name: &str) -> bool {
        self.collections.contains(name)
    }

    /// DD#100 Part B: a collection-parameter slot's *kind* is whatever the
    /// call site passed, and this pass runs before inlining, so no
    /// program-wide answer exists. Kind-specific checks skip slots.
    fn is_slot(&self, name: &str) -> bool {
        crate::ir::is_collection_slot(name) && !self.known_slots.contains(name)
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
/// (counts), `random` (i64 draw), `epochseconds` (unix time), `signum`
/// (DD#56 last-signal number), `status` (`$?`), and the
/// `getpid`/`getppid`/`getcpid` process ids. String-valued functions
/// (`toupper`, `join`, `basename`, …) are not listed, so
/// `int n=$(toupper s)` stays a type error (SB4030).
fn is_int_returning_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "len"
            | "strlen"
            | "argc"
            | "random"
            | "signum"
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
    if env.is_slot(&a.name) {
        return;
    }
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
    crate::ir::walk_arith_subscripts(a, &mut |name| {
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

/// Walk an `ArithExpr` tree; for every sexpr `FuncCall` whose callee produces a
/// string, emit a type error.
fn check_arith_sexpr(a: &ArithExpr, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    walk_arith_funccalls(a, &mut |name, args| {
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
        check_sexpr_intrinsic_operands(name, args, span, env, errors);
    });
}

/// DD#75 §2 / DD#102: the `len` / `strlen` operand rules apply to the DD#58
/// sexpr spelling too.
///
/// `check_len_operand` and `check_string_intrinsic_operands` both take a
/// `Command`, so they only ever saw the `$(len …)` form. The transform
/// reconstructs the very same `Command` for `(len …)` — but that runs *after*
/// type check, which left the sexpr operand slot as the one place in the
/// language where these rules did not apply: `(len s)` on a scalar returned
/// the byte length `$(len s)` rejects, `(strlen $n)` measured an int's decimal
/// rendering, and `(len n)` on an int reached codegen and ICEd
/// ("expected string var, got int").
///
/// Reusing the transform's own `reconstruct_intrinsic_command` — rather than
/// re-deriving the operand shapes here — is what makes the two spellings
/// answer to one rule by construction instead of by review. Same reason
/// `validate::const_eval_arith` shares `fold_arith_op`.
fn check_sexpr_intrinsic_operands(
    name: &str,
    args: &[ArithExpr],
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    if !crate::transform::expr_handlers::is_arith_int_intrinsic(name) {
        return;
    }
    let Some(cmd) = crate::transform::expr_handlers::reconstruct_intrinsic_command(name, args)
    else {
        return;
    };
    check_len_operand(&cmd, span, env, errors);
    check_string_intrinsic_operands(&cmd, span, env, errors);
}

/// Invoke `f(name, args)` for each `FuncCall` in an `ArithExpr` tree.
fn walk_arith_funccalls(a: &ArithExpr, f: &mut impl FnMut(&str, &[ArithExpr])) {
    match a {
        ArithExpr::FuncCall { name, args } => {
            f(name, args);
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
    crate::ir::walk_arith_var_reads(a, &mut |name| {
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

/// A sexpr callee that is NOT a valid int producer: a string-returning intrinsic,
/// or a user string/void function. Int
/// intrinsics (`len`, `getpid`, `argc`, …), user `-> int` functions, and a bare
/// variable `(x)` (arithmetic grouping) are all valid and return `false`.
fn is_sexpr_string_producer(name: &str, env: &TypeEnv) -> bool {
    const STRING_INTRINSICS: &[&str] = &[
        "toupper", "tolower", "join", "keys", "basename", "dirname", "getcwd", "getenv", "arg",
        "sub", "gsub", "match",
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

/// DD#100 Part B: the collection kind bound to a function's slot. A slot is
/// filled by the caller, so its kind comes from the call sites, not the body.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotKind {
    Unbound,
    Array,
    Map,
}

/// Per-function slot kinds, inferred from every call site.
///
/// This pass runs before inlining, so a slot has no kind of its own — yet the
/// kind-specific checks (`keys` is map-only, a string key needs a map, an
/// integer key needs an array) must still apply, or they silently produce
/// wrong answers: `keys A` on an array printed garbage and `${A[x]}` on an
/// array returned element 0.
///
/// Call sites that disagree on a slot's kind are rejected. Starting strict is
/// the forward-compatible order (as in Part C): per-instantiation checking can
/// be allowed later without breaking any accepted program, whereas tightening
/// later would break them.
fn infer_slot_kinds(program: &Program, errors: &mut Vec<TypeError>) -> HashMap<String, SlotKinds> {
    let mut decls = HashMap::new();
    collect_collection_kinds(&program.statements, &mut decls);
    let mut out: HashMap<String, SlotKinds> = HashMap::new();
    collect_slot_kind_calls(&program.statements, &decls, &mut out, errors);
    out
}

type SlotKinds = [SlotKind; crate::ir::MAX_COLLECTION_SLOTS];

/// Every collection declaration in the program → whether it is a map.
pub(crate) fn collect_collection_kinds(stmts: &[Statement], out: &mut HashMap<String, bool>) {
    crate::ir::walk_stmt_tree(stmts, &mut |stmt| {
        let StmtKind::Declaration(d) = &stmt.kind else {
            return;
        };
        let Some(is_assoc) = declared_collection_kind(d) else {
            return;
        };
        for a in &d.assignments {
            out.insert(a.name.clone(), is_assoc);
        }
    });
}

/// `Some(is_map)` if this declaration introduces a collection.
fn declared_collection_kind(d: &Declaration) -> Option<bool> {
    let is_assoc = d.flags.iter().any(|f| f == "-A");
    if is_assoc || d.flags.iter().any(|f| f == "-a") {
        Some(is_assoc)
    } else {
        None
    }
}

fn collect_slot_kind_calls(
    stmts: &[Statement],
    decls: &HashMap<String, bool>,
    out: &mut HashMap<String, SlotKinds>,
    errors: &mut Vec<TypeError>,
) {
    crate::ir::walk_stmt_tree(stmts, &mut |stmt| {
        if let StmtKind::Command(c) = &stmt.kind {
            record_call_slot_kinds(c, stmt.span, decls, out, errors);
        }
    });
}

fn record_call_slot_kinds(
    c: &Command,
    span: Span,
    decls: &HashMap<String, bool>,
    out: &mut HashMap<String, SlotKinds>,
    errors: &mut Vec<TypeError>,
) {
    let fname = expr_to_name(&c.name);
    let passed = crate::ir::collection_args(&c.args);
    if passed.is_empty() {
        return;
    }
    let slots = out
        .entry(fname.clone())
        .or_insert([SlotKind::Unbound; crate::ir::MAX_COLLECTION_SLOTS]);
    for (j, name) in passed
        .iter()
        .enumerate()
        .take(crate::ir::MAX_COLLECTION_SLOTS)
    {
        let Some(kind) =
            decls
                .get(name.as_str())
                .map(|&m| if m { SlotKind::Map } else { SlotKind::Array })
        else {
            continue; // undeclared, or itself a slot — reported elsewhere
        };
        merge_slot_kind(&mut slots[j], kind, &fname, j, span, errors);
    }
}

fn merge_slot_kind(
    slot: &mut SlotKind,
    kind: SlotKind,
    fname: &str,
    j: usize,
    span: Span,
    errors: &mut Vec<TypeError>,
) {
    if *slot == SlotKind::Unbound {
        *slot = kind;
    } else if *slot != kind {
        errors.push(TypeError {
            message: format!(
                "function `{fname}` is called with both an array and a map in \
                 collection slot `{}` — a slot's kind comes from its call sites, \
                 and the body is checked against one kind. Split the function, or \
                 pass the same kind everywhere",
                crate::ir::collection_slot_name(j)
            ),
            span,
        });
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

    // DD#100 Part B: a slot's kind comes from the call sites, so infer it
    // before checking any body (a call may appear after the definition).
    env.slot_kinds = infer_slot_kinds(program, &mut errors);

    // Second pass: check statements
    for stmt in &program.statements {
        check_stmt(stmt, &mut env, &mut errors);
    }

    // Third pass (DD#58/Option C): a string producer used in an arithmetic
    // sexpr `(f …)` is a type error — it belongs in a `$()` capture, not
    // `$(( ))`. Covers the string intrinsics + user string
    // functions; closes the getenv/arg validator-bypass (they silently compiled
    // to 0 in a sexpr, sidestepping SB5020). bug-arith-sexpr-intrinsics-zero.
    run_stmt_pass(program, &env, &mut errors, check_sexpr_string_producers);

    // Fourth pass (DG 2026-08-29): a value-context `${int_x:-D}` default must be
    // an int literal (SB4030) — see check_int_value_defaults. Separate from the
    // arith pass, which owns `$(( … ))` defaults.
    run_stmt_pass(program, &env, &mut errors, check_int_value_defaults);

    // Fifth pass: a string operation on an int variable. Codegen asks the slot
    // for a string pointer and panics on the Int arm (`codegen/types.rs:29`),
    // whose comment asserts that arm is "not user-reachable input" — it was.
    run_stmt_pass(program, &env, &mut errors, check_int_string_ops);

    // Sixth pass (DD#74): an `-> int` function used as a predicate. Its own
    // pass rather than a hook in `check_if`, because the six boolean contexts
    // are spread across If/While/List/Pipeline and a single traversal is the
    // only way to apply the tail rule consistently.
    check_int_fn_predicates(program, &env, &mut errors);

    errors
}

/// Run one whole-program check over each top-level statement. Each pass
/// recurses into nested bodies itself, so this stays a flat loop.
fn run_stmt_pass(
    program: &Program,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
    pass: fn(&Statement, &TypeEnv, &mut Vec<TypeError>),
) {
    for stmt in &program.statements {
        pass(stmt, env, errors);
    }
}

/// Substring (`${n:1:2}`) is a string operation, so an int-typed subject is a
/// type error. The other string ops (`${n#…}`, `${n^^}`) are already rejected
/// at parse (SB4034, DD#65 pruned operators), which is why substring was the
/// one that reached codegen and panicked.
fn check_int_string_ops(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    crate::ir::walk_all_exprs(stmt, &mut |e| {
        let ExprKind::ParameterExpansion(pe) = &e.kind else {
            return;
        };
        if pe.operator.as_deref() == Some("substring") && env.var_type(&pe.name) == VarType::Int {
            errors.push(TypeError {
                message: format!(
                    "substring of int variable `{0}` — a substring is a string \
                     operation. Convert first: `s=\"${{{0}}}\"` then `${{s:1:2}}`",
                    pe.name
                ),
                span: e.span,
            });
        }
    });
    check_int_redirect_target(stmt, env, errors);
    check_substring_operand(stmt, errors);
}

/// A substring's offset and length are lowered as integer constants, so a part
/// that is not a decimal integer silently became **0**: `${s:1 2}`, `${s:1x}`
/// and `${s:1+1}` all yielded the whole string, and `${s:0:3x}` the empty one,
/// where bash reports a syntax error. Reject them instead (SB4030).
///
/// Two spellings are legitimate and must keep working: the parenthesised form
/// carries a negative offset (`${s:(-2)}` — the bare `${s:-2}` is the `:-`
/// disposition, so parens are the only way to write it), and an empty length
/// (`${s:1:}`) is 0 in bash too. `try_substring_op` strips the parens, so the
/// operand seen here is already `-2`.
fn check_substring_operand(stmt: &Statement, errors: &mut Vec<TypeError>) {
    crate::ir::walk_all_exprs(stmt, &mut |e| {
        let ExprKind::ParameterExpansion(pe) = &e.kind else {
            return;
        };
        if pe.operator.as_deref() != Some("substring") {
            return;
        }
        let Some(operand) = pe.operand.as_deref() else {
            return;
        };
        let ExprKind::Literal(text) = &operand.kind else {
            return;
        };
        if let Some(bad) = bad_substring_part(text) {
            errors.push(TypeError {
                message: format!(
                    "substring offset/length must be a decimal integer — `{bad}` is not. \
                     A substring is lowered as an integer constant, so it evaluates no \
                     arithmetic: write `${{{0}:2}}`, not `${{{0}:1+1}}`. Use `${{{0}:(-2)}}` \
                     for an offset from the end.",
                    pe.name
                ),
                span: e.span,
            });
        }
    });
}

/// The first part of a substring operand that is not a decimal integer, or
/// `None` if every part is fine. An empty *length* is allowed (bash reads it
/// as 0); an empty offset cannot occur, since `try_substring_op` only accepts
/// an operand starting with a digit, `-` or `(`. A third part was silently
/// ignored, so the whole operand is reported for it.
fn bad_substring_part(text: &str) -> Option<String> {
    let parts: Vec<&str> = text.split(':').collect();
    match parts.as_slice() {
        [offset] | [offset, ""] => not_an_int(offset),
        [offset, length] => not_an_int(offset).or_else(|| not_an_int(length)),
        _ => Some(text.to_string()),
    }
}

/// `Some(part)` when `part` is not a decimal integer.
fn not_an_int(part: &str) -> Option<String> {
    part.parse::<i64>().err().map(|_| part.to_string())
}

/// How many **leading** arguments of a string intrinsic are string operands.
///
/// Every string slot is a leading argument, so a count is enough: the only
/// trailing operands are the array targets of the 3-arg `match`/`matchall`
/// forms, which a count of 2 excludes.
///
/// `join` is absent deliberately — its first operand is a collection
/// (SB4030 via `check_join_operand`) and its separator must already be a
/// literal (SB5074), so neither slot can hold an int.
fn string_operand_slots(name: &str) -> Option<usize> {
    match name {
        "toupper" | "tolower" | "strlen" | "basename" | "dirname" | "getenv" => Some(1),
        "match" | "matchall" => Some(2),
        "sub" | "gsub" => Some(3),
        _ => None,
    }
}

/// A string intrinsic applied to an int-typed operand is a type error.
///
/// `$(toupper $n)` on an `int n=42` used to yield `"42"` — the int was
/// silently rendered as text, exactly the implicit coercion the type system
/// exists to prevent, and `$(strlen $n)` answered a question about a decimal
/// rendering the source never asked for. `join`/`len` already rejected a
/// mistyped operand (SB4030); these did not.
///
/// The test is the operand's *type*, not its spelling, so it also catches
/// `$(( … ))`, an int-returning intrinsic like `$(getpid)`, and an element of
/// an `int array`. A double-quoted operand is String by construction, which
/// keeps `"$n"` available as the explicit conversion — the same escape the
/// substring diagnostic above recommends.
fn check_string_intrinsic_operands(
    c: &Command,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    let name = expr_to_name(&c.name);
    let Some(slots) = string_operand_slots(&name) else {
        return;
    };
    for arg in c.args.iter().take(slots) {
        if expr_type(arg, env) != VarType::Int {
            continue;
        }
        errors.push(TypeError {
            message: int_operand_message(&name, arg),
            span,
        });
    }
}

/// The SB4030 message for an int operand of a string intrinsic, naming the
/// variable when the operand is a plain expansion.
fn int_operand_message(name: &str, arg: &Expr) -> String {
    let operand = match &arg.kind {
        ExprKind::SimpleExpansion(v) => format!("`{v}` is an int variable"),
        ExprKind::Subscript { .. } => "that element is int-typed".to_string(),
        ExprKind::Arithmetic(_) => "an arithmetic expansion is int-typed".to_string(),
        _ => "that operand is int-typed".to_string(),
    };
    format!(
        "`{name}` is a string operation but {operand} — an int is not \
         implicitly text. Convert explicitly by double-quoting the operand, \
         e.g. `$({name} \"${{…}}\" …)`"
    )
}

/// A redirect target is a filesystem path, so an int variable there is a type
/// error rather than a codegen panic.
fn check_int_redirect_target(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    if let StmtKind::Redirected(r) = &stmt.kind {
        for redir in &r.redirects {
            check_one_redirect_target(&redir.target, env, errors);
        }
    }
    for body in stmt.child_bodies() {
        for s in body {
            check_int_redirect_target(s, env, errors);
        }
    }
}

/// One redirect target: an int-typed variable is a type error rather than the
/// codegen panic it used to be.
fn check_one_redirect_target(target: &Expr, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let ExprKind::SimpleExpansion(name) = &target.kind else {
        return;
    };
    if env.var_type(name) != VarType::Int {
        return;
    }
    errors.push(TypeError {
        message: format!(
            "redirect target `${0}` is an int variable — a target is a path \
             (string). Convert with `\"${{{0}}}\"`",
            name
        ),
        span: target.span,
    });
}

/// DD#33: Check expressions for subscript key type mismatches.
fn check_subscript_types(expr: &Expr, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    match &expr.kind {
        ExprKind::Subscript { name, index } => {
            check_subscript_key_kind(name, index, expr.span, KeySurface::Read, env, errors);
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

/// Which surface a subscript was written on. The two give different advice:
/// the read path can direct to a DD#77 disposition, but an assignment LHS
/// still only parses a bare `$var`/`${var}` index (DD#33 —
/// `parse_split_subscript_index`, SB4014), so recommending `a[${s:?m}]=…`
/// there would send the author from SB4030 straight into SB4014.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KeySurface {
    Read,
    Write,
}

/// Reject integer keys on maps (looks like indexed access) and string
/// keys on indexed arrays.
fn check_subscript_key_kind(
    name: &str,
    index: &Expr,
    span: Span,
    surface: KeySurface,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    // Subscripting a non-collection is SB5068 (read and write paths alike), so
    // `name` is always a collection by the time this runs.
    if env.is_slot(name) {
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
    } else if !is_map && is_string_key(index, env) {
        errors.push(TypeError {
            message: string_key_on_array_message(name, index, surface),
            span,
        });
    }
}

/// The SB4030 text for a string key on an int-indexed array.
///
/// A *variable* index gets different advice from a literal one. `conversion_hint`
/// learned this lesson already: recommending `${a[$i]}` to someone who wrote
/// `${a[$s]}` sends them from one SB4030 to the same SB4030, because the
/// spelling was never the problem — the operand's type was. A string variable
/// needs the DD#77 disposition, so name it.
fn string_key_on_array_message(name: &str, index: &Expr, surface: KeySurface) -> String {
    let var = match &index.kind {
        ExprKind::SimpleExpansion(v) => Some(v.as_str()),
        ExprKind::ParameterExpansion(pe) if pe.operator.is_none() => Some(pe.name.as_str()),
        _ => None,
    };
    match (var, surface) {
        // An assignment LHS parses only a bare `$var` index (DD#33/SB4014), so
        // the disposition spellings the read path recommends do not compile
        // here. Name what does.
        (Some(v), KeySurface::Write) => format!(
            "string key on indexed array '{name}' — `${v}` is string-typed, and an \
             int-indexed array is indexed by number (DD#80). An assignment index must \
             already be a number: use an integer literal, or an `int` variable \
             (`int i=…; {name}[$i]=…`). A disposition (`${{{v}:?msg}}`) is not yet \
             accepted on an assignment index; convert first. Declare `map {name}` for \
             string keys"
        ),
        (Some(v), KeySurface::Read) => format!(
            "string key on indexed array '{name}' — `${v}` is string-typed, and an \
             int-indexed array is indexed by number (DD#80). A string→int crossing must \
             carry a disposition: `${{{name}[${{{v}:-0}}]}}` (0 if it isn't an integer) or \
             `${{{name}[${{{v}:?msg}}]}}` (abort if it isn't); declare `map {name}` for \
             string keys"
        ),
        (None, _) => format!(
            "string key on indexed array '{name}' — an int-indexed array is indexed by \
             number (DD#80). Use an integer, `${{{name}[$i]}}` with an int `i`, or \
             `$(( … ))`; declare `map {name}` for string keys"
        ),
    }
}

fn check_subscript_in_stmt(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            check_subscript_types(&c.name, env, errors);
            check_join_operand(c, stmt.span, env, errors);
            check_keys_operand(c, stmt.span, env, errors);
            check_len_operand(c, stmt.span, env, errors);
            check_string_intrinsic_operands(c, stmt.span, env, errors);
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
    // A non-name operand is SB5062 and an unpassed slot is SB5066, both from
    // validate, so `name` is always a real name here and a passed slot is
    // always in `assoc_arrays` or `collections`.
    if env.assoc_arrays.contains(&name) {
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

/// DD#75 §2 amendment: `len` is collection-only. It used to dispatch on the
/// operand's type — *bytes* for a string, *elements* for a collection — so two
/// different questions shared one spelling with nothing at the call site to
/// say which was being asked. The compiler could tell them apart; a reader
/// looking at `$(len x)`, with `x` declared far away, could not. `strlen`
/// names the byte-length question, and `len` now answers only the count one.
fn check_len_operand(c: &Command, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    if expr_to_name(&c.name) != "len" {
        return;
    }
    let Some(arg) = c.args.first() else {
        return;
    };
    let name = join_operand_name(arg);
    // Fire only on a name known to be a scalar. "Absent from `collections`"
    // is not the same question: an undeclared name is SB5039's or SB5055's to
    // report, and answering it here masked the real diagnostic with a
    // misleading one ("`f` is a scalar" when `f` was never declared at all).
    //
    // An `|| env.is_slot(&name)` clause here was unreachable: a DD#100 slot
    // letter can be neither a declared variable (SB5065) nor a loop variable
    // (SB5080), and an unpassed slot is SB5066 — validate rejects every route
    // before type check runs. An `|| env.loop_vars.contains(&name)` clause
    // went with it: `check_for` types every loop variable into `vars`, so the
    // first disjunct already answers for them.
    if !env.vars.contains_key(&name) || env.collections.contains(&name) || name == "args" {
        return;
    }
    errors.push(TypeError {
        message: format!(
            "`len` needs an array or map — `{name}` is a scalar; use `$(strlen ${name})` \
             for its byte length"
        ),
        span,
    });
}

/// The variable name an operand refers to: `@a` (arrvals splat) → `a`; a bare
/// name → itself; `$a` → `a`; anything else → empty (not a simple name
/// operand).
///
/// `$a` resolves to the same name as `a` because the operand slot of
/// `len`/`strlen`/`join` resolves a *name*, and both spellings name the same
/// variable. Leaving `$a` unresolved here let `len $s` past the
/// collection-only check while `len s` was rejected — the sigil silently
/// decided whether the guard ran.
pub(crate) fn join_operand_name(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            pe.name.clone()
        }
        ExprKind::Literal(s) => s.clone(),
        ExprKind::SimpleExpansion(s) => s.clone(),
        _ => String::new(),
    }
}

/// Check if an expression is a string key — i.e. a key that does **not** index
/// an int-indexed array.
///
/// DD#80 enumerates the valid `${a[K]}` index forms, and its type-aware clause
/// names a "string key" as "a clean bareword or a quoted string"; its SB4030
/// bullet adds "a non-numeric expansion result". For a `$var` that last one is
/// a **runtime** property, which is why the variable arms here long read
/// `SimpleExpansion(_) => false, // could be either`.
///
/// It is not always "either". A scalar the author declared `str` is a string by
/// construction, and `${a[$s]}` / `a[$s]=…` on it silently indexed 0 — the same
/// string→int crossing DD#77 requires a disposition for, and which the bareword
/// `${a[s]}`, the quoted `${a["abc"]}` and the arithmetic `(( a[s] ))` all
/// already reject. Only that positively-declared case is caught here: a bare
/// `i=1` is indistinguishable from `str s=abc` in `vars` (both `String`), and
/// rejecting it would break the documented `attest i i?` and `${arr[$1]}`
/// idioms, which have no declared type at all.
fn is_string_key(expr: &Expr, env: &TypeEnv) -> bool {
    match &expr.kind {
        // a non-numeric bareword literal is a string key
        ExprKind::Literal(s) => !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit()),
        // an explicitly quoted key is a string — even a quoted numeric `"5"`
        // (DD#80: use the unquoted integer `${a[5]}` to index an int array)
        ExprKind::DoubleQuoted(_) | ExprKind::SingleQuoted(_) => true,
        // `$s` where `s` was declared `str` (DD#80 case 2, type-aware).
        ExprKind::SimpleExpansion(v) => env.is_declared_str(v),
        // `${s}` is `$s`. A DD#77 disposition (`:-`/`:?`) is the sanctioned
        // string→int crossing — it converts under a strict "is it an integer"
        // trigger — so it is never a string key. It is what the diagnostic
        // directs the author to.
        ExprKind::ParameterExpansion(pe) if pe.operator.is_none() => env.is_declared_str(&pe.name),
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
    record_bare_collection_value(a, env);
    // DD#75 §4: a subscript index is a numeric literal or a `$`-sigilled
    // value; a bare word is a string key (map-only). Enforce on the write
    // path as on the read path (check_subscript_key_kind), so `a[i]=…` on an
    // indexed array is SB4030 just like the read `${a[i]}`.
    if let Some(idx) = &a.index {
        check_subscript_key_kind(&a.name, idx, span, KeySurface::Write, env, errors);
    }
    check_assign_type_match(a, span, env, errors);
}

/// A bare `f=(…)` is a collection VALUE on an undeclared name. The keyword is
/// what makes a collection, so the transform rejects it (SB5055) — but the env
/// must still record what it is, or a later collection check reads `f` as a
/// scalar and reports something untrue ("`len` needs an array — `f` is a
/// scalar") ahead of the diagnostic that names the real problem.
fn record_bare_collection_value(a: &Assignment, env: &mut TypeEnv) {
    if a.index.is_none() && matches!(a.value.kind, ExprKind::Array(_)) {
        env.collections.insert(a.name.clone());
    }
}

/// Bare assignment: a new variable gets the RHS type on first use. An
/// already-typed variable must match — except (DD#30 refinement) an already-int
/// LHS accepts a bare numeric literal (`int x=0; x=5` aligns with `int x=5`;
/// `int array a=(…); a[i]=99` matches int-init), mirroring the
/// declaration-init `is_numeric_literal` exception. The variable keeps its
/// declared int type (no re-declare).
/// DD#86: a `for` header BINDS its variable, exactly as an assignment does,
/// so it is type-checked exactly as an assignment is. Without this, a loop
/// over string words into an `int` variable was accepted and every iteration
/// silently bound **0** — while the equivalent bare assignment was SB4030.
///
/// An *undeclared* loop variable is introduced by the header as a **string**,
/// because a word list binds words. That is the same instantiation a bare
/// assignment performs (`x=5` holds `"5"`, not 5), so it earns the same
/// treatment in arithmetic: `(( sum += x ))` is SB4030 and must carry a
/// disposition, `(( sum += ${x:-0} ))`.
///
/// This is not a restriction on counting loops — it is a redirection to the
/// form that means one. `for (( i = 10; i < 40; i += 10 ))` binds a genuine
/// int and needs no disposition (DG, 2026-09-17). The word-list header is for
/// words; the arithmetic header is for numbers.
fn check_for_binding_type(f: &For, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    if !env.vars.contains_key(&f.variable) {
        env.declare(&f.variable, VarType::String);
        return;
    }
    let ltype = env.var_type(&f.variable);
    let Some((word, rtype)) = first_binding_mismatch(&f.words, ltype, env) else {
        return;
    };
    errors.push(TypeError {
        message: format!(
            "cannot bind {} value to {} loop variable `{}` — a `for` header \
             binds its variable like an assignment, and a variable does not \
             change type (DD#86){}",
            type_name(rtype),
            type_name(ltype),
            f.variable,
            conversion_hint(rtype, ltype),
        ),
        // The offending word, not the whole header: with several words only
        // one may be wrong.
        span: word.span,
    });
}

/// The first header word whose type the loop variable cannot take, with that
/// type. An `int` variable accepts a numeric literal, exactly as an
/// assignment does.
fn first_binding_mismatch<'a>(
    words: &'a [Expr],
    ltype: VarType,
    env: &TypeEnv,
) -> Option<(&'a Expr, VarType)> {
    words.iter().find_map(|w| {
        let rtype = expr_type(w, env);
        let takes = ltype == rtype || (ltype == VarType::Int && is_numeric_literal(w));
        (!takes).then_some((w, rtype))
    })
}

fn check_assign_type_match(
    a: &Assignment,
    span: Span,
    env: &mut TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    if !env.vars.contains_key(&a.name) {
        let rtype = expr_type(&a.value, env);
        env.declare(&a.name, rtype);
        return;
    }
    let ltype = env.var_type(&a.name);
    check_type_match(ltype, &a.value, &a.name, span, env, errors);
}

/// The single place a scalar string↔int mismatch is diagnosed.
///
/// Declarations (`str s = $n`) and assignments (`s=$v`) both land here. They
/// used to reach the same sentence through two independent `format!`s — one
/// parameterised over `type_name()`, one spelling each direction out in full —
/// so grepping the finished message found only one copy, and the declaration
/// side was reached by no spec at all. `VarType` has two variants, so the
/// specialised pair was never more than this. See journal 397.
fn check_type_match(
    ltype: VarType,
    value: &Expr,
    name: &str,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    let rtype = expr_type(value, env);
    // A bare numeral is not an int *value*: `str s = 5` declares a string.
    if ltype == rtype || (ltype == VarType::Int && is_numeric_literal(value)) {
        return;
    }
    errors.push(TypeError {
        message: format!(
            "cannot assign {} value to {} variable `{}`{}",
            type_name(rtype),
            type_name(ltype),
            name,
            conversion_hint(rtype, ltype),
        ),
        span,
    });
}

fn check_declaration(d: &Declaration, span: Span, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    let is_collection = is_collection_decl(d);
    env.note_declared_str(d);
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
    a: &crate::ir::DeclAssignment,
    is_collection: bool,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    // A bare declaration has no initializer, so there is nothing to type.
    let Some(value) = &a.value else {
        return;
    };
    if !is_collection {
        check_type_match(d.var_type, value, &a.name, span, env, errors);
    } else if d.var_type == VarType::Int {
        check_int_collection_elements(value, &a.name, span, env, errors);
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
    // DD#60/DD#61: argv/read auto-declare their target collection —
    // register it so a later attest-all / len / element read sees a collection,
    // not a scalar (the front-end analogue of the transform's auto-declare).
    match expr_to_name(&c.name).as_str() {
        "unset" => check_unset_args(c, span, env, errors),
        "attest" => check_attest_form(c, span, env, errors),
        "argv" => register_argv_target(c, env),
        // DD#99: `read [SRC] ARRAY` — the target is always the last arg.
        "read" => register_newread_target(c, env),
        // DD#63: `match S P ARR` / `matchall S P ARR` fill (and auto-declare)
        // the trailing array; the two-arg `match S P` binds nothing.
        "match" | "matchall" if c.args.len() == 3 => register_newread_target(c, env),
        "exit" => check_exit_status_arg(c, errors),
        _ => {}
    }

    // DD#33: check subscript key types in command args
    check_join_operand(c, span, env, errors);
    check_keys_operand(c, span, env, errors);
    check_string_intrinsic_operands(c, span, env, errors);
    for arg in &c.args {
        check_subscript_types(arg, env, errors);
    }
}

/// `exit N` lowers `N` as an integer constant, so a non-numeric argument
/// silently became **0** — `exit 1x` reported *success*, where bash refuses
/// with "numeric argument required" and exits 2. A script that meant to fail
/// would have claimed to succeed, so reject it (SB4030).
///
/// Only a bare literal is checked. `exit $x` and `exit $(f)` are lowered
/// through the runtime's atoi, not this constant path.
fn check_exit_status_arg(c: &Command, errors: &mut Vec<TypeError>) {
    for arg in &c.args {
        let ExprKind::Literal(text) = &arg.kind else {
            continue;
        };
        if text.is_empty() || text.parse::<i64>().is_ok() {
            continue;
        }
        errors.push(TypeError {
            message: format!(
                "exit status `{text}` is not a decimal integer — `exit` takes a \
                 number, and a non-numeric argument would otherwise be lowered as 0, \
                 reporting success for a script that meant to fail."
            ),
            span: arg.span,
        });
    }
}

/// DD#60: `argv [N] a` fills (and auto-declares) the trailing array.
fn register_argv_target(c: &Command, env: &mut TypeEnv) {
    if let Some(arr) = c.args.last() {
        env.collections.insert(expr_to_name(arr));
    }
}

/// DD#99: `read [SRC] ARRAY` fills an indexed array named by its LAST
/// argument (the optional first one is the scalar VALUE being split).
fn register_newread_target(c: &Command, env: &mut TypeEnv) {
    if let Some(last) = c.args.last() {
        env.collections.insert(expr_to_name(last));
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

/// DD#61: the attested name is introduced nowhere at all.
///
/// "Absent from `collections`" and "is a scalar" are not the same question.
/// Answering the first with the second told the user `'zz' is a scalar`
/// about a name that was never declared, and then advised the scalar form —
/// which fails too. `check_len_operand` draws the same distinction for the
/// same reason; the wording matches SB5039's.
///
/// Unlike `len`, this cannot simply stay silent and defer: nothing else
/// rejects an attest-all over an undeclared name, so the program would
/// compile and iterate a collection that does not exist.
fn attest_unknown_name_error(name: &str, env: &TypeEnv) -> Option<String> {
    if env.vars.contains_key(name) || env.collections.contains(name) {
        return None;
    }
    Some(format!(
        "`attest … in {name}` — `{name}` is introduced nowhere (not a variable, \
         array, or map); declare it, or fix the typo"
    ))
}

fn attest_indexed_form_error(binder: &str, array: &str, env: &TypeEnv) -> Option<String> {
    if let Some(unknown) = attest_unknown_name_error(array, env) {
        Some(unknown)
    } else if !env.collections.contains(array) {
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
    if let Some(unknown) = attest_unknown_name_error(map, env) {
        return Some(unknown);
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
    check_for_binding_type(f, env, errors);
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
    // DD#100: seed the map slots this function is called with, so the
    // kind-specific checks (`keys` map-only, string vs integer key) apply
    // inside the body exactly as they would on the caller's collection.
    let map_slots = enter_slot_kinds(&f.name, env);
    for s in &f.body {
        check_stmt(s, env, errors);
    }
    for name in map_slots {
        env.assoc_arrays.remove(&name);
        env.known_slots.remove(&name);
    }
    if !had_args {
        env.collections.remove("args");
    }
}

/// Bind this function's slot kinds for the duration of its body check,
/// returning the names to clear afterwards. A **map** slot is also marked
/// associative, which is what the kind checks read; an **array** slot only
/// needs to be *known*. An unbound slot stays unknown, so `TypeEnv::is_slot`
/// keeps it out of the kind checks — there is nothing to check it against.
fn enter_slot_kinds(fname: &str, env: &mut TypeEnv) -> Vec<String> {
    let Some(kinds) = env.slot_kinds.get(fname).copied() else {
        return Vec::new();
    };
    let mut added = Vec::new();
    for (j, k) in kinds.iter().enumerate() {
        if *k == SlotKind::Unbound {
            continue;
        }
        let name = crate::ir::collection_slot_name(j);
        if *k == SlotKind::Map {
            env.assoc_arrays.insert(name.clone());
        }
        env.known_slots.insert(name.clone());
        added.push(name);
    }
    added
}

fn check_if(if_stmt: &If, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
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

/// DD#74 enumerates the boolean contexts: `if f`, `elif`, `while f`,
/// `until f`, `! f`, and `f` as an operand of `&&`/`||`. A predicate is a
/// function whose *status* something branches on; an `-> int` function's
/// `return N` is a **value**, and a nonzero value is a **false** status — so
/// `while f` never loops and `if ! f` takes the true branch, the opposite of
/// what the integer suggests.
///
/// This used to run from a single caller (`check_if`, direct condition only),
/// so five of the six contexts accepted an int function silently.
///
/// Collect-then-emit, because a statement inside a condition is reached both
/// as a condition operand and again by the descent, and both visits are
/// correct — they would simply report it twice.
fn check_int_fn_predicates(program: &Program, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let mut found: Vec<(Span, String)> = Vec::new();
    walk_int_fn_predicates(&program.statements, env, &mut found);
    for (span, name) in found {
        errors.push(TypeError {
            message: format!(
                "int function `{}` used in boolean context — \
                 int functions return values, not success/failure",
                name,
            ),
            span,
        });
    }
}

fn walk_int_fn_predicates(stmts: &[Statement], env: &TypeEnv, found: &mut Vec<(Span, String)>) {
    for stmt in stmts {
        stmt_predicate_positions(stmt, env, found);
        for body in stmt.child_bodies() {
            walk_int_fn_predicates(body, env, found);
        }
    }
}

/// The predicate positions *this* statement introduces.
fn stmt_predicate_positions(stmt: &Statement, env: &TypeEnv, found: &mut Vec<(Span, String)>) {
    match &stmt.kind {
        StmtKind::If(i) => if_predicate_positions(i, env, found),
        // Covers `until` too — it is a `While` with `until: true`.
        StmtKind::While(w) => condition_predicate(&w.condition, env, found),
        // DD#74 addendum: the TAIL of a bare `&&`/`||` *statement* is not a
        // predicate — nothing branches on it, it is the statement's own
        // status. The left operand is, and by left-associativity so is every
        // middle clause (`a && b && c` is `(a && b) && c`, so recursing into
        // the left operand reaches both `a` and `b`).
        StmtKind::List(l) if matches!(l.op, ListOp::And | ListOp::Or) => {
            predicate_operand(&l.left, env, found);
        }
        StmtKind::Pipeline(p) if p.negated && p.commands.len() == 1 => {
            predicate_operand(&p.commands[0], env, found);
        }
        _ => {}
    }
}

fn if_predicate_positions(i: &If, env: &TypeEnv, found: &mut Vec<(Span, String)>) {
    condition_predicate(&i.condition, env, found);
    for elif in &i.elif_clauses {
        condition_predicate(&elif.condition, env, found);
    }
}

/// The last statement of a condition supplies its truth value. Inside a
/// condition the whole list is a predicate, **last clause included** (DD#74):
/// a condition is consumed as a boolean, so its final status decides the
/// branch. That is the one place the tail rule above does not apply.
fn condition_predicate(cond: &[Statement], env: &TypeEnv, found: &mut Vec<(Span, String)>) {
    if let Some(last) = cond.last() {
        predicate_operand(last, env, found);
    }
}

/// Record `stmt` if it is (or resolves through negation / `&&` / `||` to) a
/// bare call of an `-> int` function. Mirrors `validate::mark_predicate_operand`,
/// which is the same DD#74 traversal for the definite-return rule.
fn predicate_operand(stmt: &Statement, env: &TypeEnv, found: &mut Vec<(Span, String)>) {
    match &stmt.kind {
        StmtKind::Command(c) => record_int_fn_predicate(c, stmt.span, env, found),
        StmtKind::Pipeline(p) if p.negated && p.commands.len() == 1 => {
            predicate_operand(&p.commands[0], env, found);
        }
        StmtKind::List(l) if matches!(l.op, ListOp::And | ListOp::Or) => {
            predicate_operand(&l.left, env, found);
            predicate_operand(&l.right, env, found);
        }
        _ => {}
    }
}

fn record_int_fn_predicate(
    c: &Command,
    span: Span,
    env: &TypeEnv,
    found: &mut Vec<(Span, String)>,
) {
    let name = expr_to_name(&c.name);
    if env.func_type(&name) != VarType::Int {
        return;
    }
    if already_recorded(found, span, &name) {
        return;
    }
    found.push((span, name));
}

fn already_recorded(found: &[(Span, String)], span: Span, name: &str) -> bool {
    found
        .iter()
        .any(|(s, n)| s.line == span.line && s.col == span.col && n == name)
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

/// The hint appended to a type-mismatch error.
///
/// `VAR` is a placeholder, not a variable name: the right-hand side here is an
/// arbitrary expression, and this function is not given its spelling.
///
/// The string→int direction must name a **disposition**. Plain `$(($x))` is
/// not a conversion — it is SB4030 in its own right ("a string→int conversion
/// must carry a disposition"), so recommending it sent the user from one
/// SB4030 to another.
fn conversion_hint(from: VarType, to: VarType) -> String {
    match (from, to) {
        (VarType::Int, VarType::String) => {
            " — quote it to convert int to string: `\"$VAR\"`".to_string()
        }
        (VarType::String, VarType::Int) => " — a string→int conversion goes through \
             arithmetic and must carry a disposition: `$(( ${VAR:-0} ))` (0 if it \
             isn't an integer) or `$(( ${VAR:?msg} ))` (abort if it isn't)"
            .to_string(),
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
/// A non-`int` declaration takes each name's type from its initializer. A
/// bare `str s` has none to infer from and is a string by declaration —
/// exactly as the old in-band `Literal("")` sentinel typed it.
fn infer_from_initializers(d: &crate::ir::Declaration, env: &mut TypeEnv) {
    for a in &d.assignments {
        let t = a
            .value
            .as_ref()
            .map_or(VarType::String, |v| expr_type(v, env));
        env.declare(&a.name, t);
    }
}

fn infer_declaration_types(d: &crate::ir::Declaration, env: &mut TypeEnv) {
    if d.var_type == VarType::Int {
        for a in &d.assignments {
            env.declare(&a.name, VarType::Int);
        }
    } else {
        // A `str` *scalar* declaration is the one positive signal that a
        // variable holds a string — record it so the subscript check can tell
        // it apart from a bare `i=1` (see `is_declared_str`).
        env.note_declared_str(d);
        infer_from_initializers(d, env);
    }
}
