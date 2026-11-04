/// safebash type checker (DD#16).
///
/// Operates on the IR after lowering. Reports type errors for:
/// - Cross-type assignment without explicit conversion
/// - An int operand in a `[[ ]]` test, dispatch or `attest` predicate
/// - `exit` with unbounded int variable
/// - `if func` where func is `-> int`
use crate::ir::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
}

/// Checked scalar/element types and collection kinds, including body locals.
pub struct CheckedTypes {
    pub(crate) vars: HashMap<String, VarType>,
    /// Collection identity -> whether it is a map.
    pub(crate) collections: HashMap<String, bool>,
}

/// Environment tracking variable and function types.
#[derive(Clone, PartialEq, Eq)]
struct TypeEnv {
    vars: HashMap<String, VarType>,
    /// Actual bindings in this pass, distinct from the previous pass's seeds.
    bound: HashSet<String>,
    funcs: HashMap<String, VarType>,
    /// Variables that are intrinsically int ($#, $?)
    intrinsic_int: HashSet<String>,
    /// Variables declared as array or map (collections)
    collections: HashSet<String>,
    /// DD#100: one collection signature per function, inferred from all calls.
    slot_signatures: HashMap<String, CollectionSignature>,
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
        // Neither `$!` nor `$?` is seeded here any more. Both were, because the
        // punctuation forms were String `SimpleExpansion`s while the intrinsics
        // they are sugar for — `$(getcpid)`, `$(status)` — were already
        // int-returning, so `wait $!` failed a rule `wait $(getcpid)` passed.
        // DD#111 made the punctuation *be* the intrinsic by the time this pass
        // runs, typed by `is_int_returning_intrinsic`, and
        // `assert_no_magic_sugar` guarantees no `SimpleExpansion("!"|"?")`
        // survives parse — so the seeds could never be consulted again. (The
        // only other way to name them is `${!}` / `${?}`, rejected by SB4003 as
        // an indirect expansion and SB4034 as an unsupported parameter operator
        // respectively.)
        //
        // `#` stays: `$#` is still a genuine `SimpleExpansion` with no
        // intrinsic spelling.
        let mut intrinsic_int = HashSet::new();
        intrinsic_int.insert("#".to_string()); // $#
                                               // DD#100: slots are always collections. Their full type is installed
                                               // per function from its one inferred signature before checking its body.
        let collections = (0..crate::ir::MAX_COLLECTION_SLOTS)
            .map(crate::ir::collection_slot_name)
            .collect();
        Self {
            vars: HashMap::new(),
            bound: HashSet::new(),
            funcs: HashMap::new(),
            intrinsic_int,
            collections,
            slot_signatures: HashMap::new(),
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

    /// Record a `str` scalar declaration, not just an inferred string type.
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

    /// An uncalled function has no call-site type for its slots.
    fn is_slot(&self, name: &str) -> bool {
        crate::ir::is_collection_slot(name) && !self.known_slots.contains(name)
    }

    fn declare(&mut self, name: &str, vtype: VarType) {
        if self.bound.contains(name) {
            assert_eq!(
                self.vars.get(name),
                Some(&vtype),
                "checked binding cannot change type"
            );
        }
        self.bound.insert(name.to_string());
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
///
/// A *statement* builtin — `$(echo …)`, `$(unset x)` — keeps the default
/// String: it is a capture of the command's stdout, not an intrinsic call,
/// which is why `resolve::ArithDenotation::Statement` is about arithmetic
/// only and says nothing about `$( )`.
fn command_substitution_type(parts: &[Statement], env: &TypeEnv) -> VarType {
    let Some(c) = crate::ir::single_value_command(parts) else {
        return VarType::String;
    };
    let name = expr_to_name(&c.name);
    if crate::resolve::is_int_valued_intrinsic(&name) {
        return VarType::Int;
    }
    env.func_type(&name)
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
    walk_own_exprs(stmt, &mut |e| {
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
    check_arith_expansion_string_cell(a, span, env, errors);
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
        errors.push(bare_map_subscript_error(
            &a.name,
            span,
            env.var_type(&a.name) == VarType::String,
        ));
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
        check_bare_cell(name, span, env, errors);
    });
}

fn check_bare_cell(name: &str, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    if env.is_slot(name) {
        return;
    }
    if env.assoc_arrays.contains(name) {
        errors.push(bare_map_subscript_error(
            name,
            span,
            env.var_type(name) == VarType::String,
        ));
    } else if env.is_collection(name) && env.var_type(name) == VarType::String {
        errors.push(string_cell_error(name, span));
    }
}

/// DD#77 D1 for a cell read through the `${…}` **value-import** — the other
/// half of [`check_arith_bare_map_subscript`], and the reason that one is not
/// enough on its own.
///
/// "A string cell behaves like a string variable" (DG), so a cell whose
/// elements are strings owes the same disposition a bare string scalar does.
/// The *bare* form `a[i]` has been checked since 2026-08-19; the braced form
/// never was, because it arrives as an [`ArithExpr::Expansion`] and
/// `walk_arith_subscripts` only yields bare cells. So `$(( ${s[0]} ))` on
/// `array s=( hi )` silently evaluated to **0** — precisely the string→int
/// crossing DD#77 exists to stop, reached through the one spelling that
/// skipped the check, while `s[0]` and `$s` both rejected.
///
/// This is also the only place a string **map** cell can be caught at all: a
/// bare `m[k]` is inexpressible under arith rules (model B), so `${m[k]}` is
/// the sole spelling, and before this it was unchecked.
///
/// A disposition-carrying cell (`${s[0]:-7}`, `${s[0]:?msg}`) is exempt, but
/// `${s[0]:!}` only changes trust and still owes the conversion. A cell passed
/// to a sexpr (`(strlen ${a[1]})`) is not an int operand and is not visited.
fn check_arith_expansion_string_cell(
    a: &ArithExpr,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    crate::ir::walk_arith_imported_operands(a, &mut |e| {
        check_imported_cell(e, span, env, errors);
    });
}

fn check_imported_cell(e: &Expr, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let Some(name) = undisposed_cell_name(e) else {
        return;
    };
    // Only an uncalled function lacks a call-site signature.
    if env.is_slot(name) {
        return;
    }
    if env.is_collection(name) && env.var_type(name) == VarType::String {
        errors.push(string_cell_error(name, span));
    }
}

fn undisposed_cell_name(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Subscript { name, .. } => Some(name),
        ExprKind::ParameterExpansion(pe) if pe.index.is_some() && pe.operator.is_none() => {
            Some(&pe.name)
        }
        _ => None,
    }
}

/// DD#77 D1 for a string-valued cell: the same discipline as a string scalar —
/// the cell value is a string, so its use as an arithmetic operand must carry a
/// disposition. Raised for both spellings (`a[i]` bare and `${a[i]}` imported),
/// which is why the text names neither.
fn string_cell_error(name: &str, span: Span) -> TypeError {
    TypeError {
        message: format!(
            "string cell `{name}[…]` in arithmetic — `{name}` holds strings, so \
             (like a string variable, DD#77) the string→int conversion must carry \
             a disposition: `${{{name}[i]:-N}}` (default N if not an integer) or \
             `${{{name}[i]:?msg}}` (abort if not an integer)."
        ),
        span,
    }
}

/// `string_valued` picks the read hint. A map's cells are strings unless it was
/// declared `int map`, and for a string map `${m[k]}` alone is *not* a legal
/// read — it owes a disposition like any other string cell. Sending the author
/// there would just cost them a second compile.
fn bare_map_subscript_error(name: &str, span: Span, string_valued: bool) -> TypeError {
    let read = if string_valued {
        format!("`${{{name}[k]:-N}}` (a string cell owes a disposition, DD#77)")
    } else {
        format!("`${{{name}[k]}}` (imports the value)")
    };
    TypeError {
        message: format!(
            "bare `{name}[…]` in arithmetic — `{name}` is a map, and a string key \
             isn't expressible under arithmetic rules. Read with {read} \
             and write with `{name}[k]=…` (statement form)."
        ),
        span,
    }
}

/// Walk an `ArithExpr` tree; for every sexpr `FuncCall` whose callee produces a
/// string, emit a type error.
fn check_arith_sexpr(a: &ArithExpr, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    walk_arith_funccalls(a, &mut |name, args| {
        if let Some(e) = sexpr_callee_error(name, span, env) {
            errors.push(e);
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
        check_string_scalar_operand(name, span, env, errors);
    });
    crate::ir::walk_arith_imported_operands(a, &mut |e| {
        check_scalar_import_operand(e, span, env, errors);
    });
}

fn check_scalar_import_operand(e: &Expr, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let ExprKind::ParameterExpansion(pe) = &e.kind else {
        return;
    };
    if pe.operator.is_some() || pe.index.is_some() {
        return;
    }
    check_string_scalar_operand(&pe.name, span, env, errors);
}

fn check_string_scalar_operand(name: &str, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
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
}

/// DD#77 D4: in arithmetic, a `${s:-DEFAULT}` disposition's DEFAULT must be a
/// decimal integer literal. The default is lowered by a plain decimal atoi, so
/// a non-decimal default (`hello`, `0xff`, `5+3`, an interpolation, or empty)
/// silently became `0` — reject it instead (SB4030). `:?` carries a message,
/// not a numeric default, so it is never checked here.
fn check_arith_default_literals(a: &ArithExpr, errors: &mut Vec<TypeError>) {
    // The canonical walker, not a local copy. This pass had its own, and that
    // copy's `_ => {}` dropped `Subscript`/`SubscriptAssign` — so a `:-`
    // default inside a subscript index (`$(( a[${n:-abc}] ))`) was never
    // checked, and `abc` reached int lowering: an ICE natively, and
    // `sash_atoi("abc")` in the C backend. `walk_arith_expansion_roots` is
    // total and already carries the fix; `_roots` (not `walk_arith_expansions`)
    // because this callback does not recurse, which is the semantics the local
    // copy had.
    crate::ir::walk_arith_expansion_roots(a, &mut |inner| {
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

/// Is a `${var:-DEFAULT}` default a DD#72 integer literal (decimal, or an
/// `0x`/`0o`/`0b` radix prefix)? Rejects leading-zero octal, `base#digits`,
/// interpolations and empty.
///
/// A present operand is an `ExprKind::Literal`. DD#65's original 2026-07-04
/// model made it an interpolating `DoubleQuoted`, and this function still
/// carried an arm for that shape — dead since the **2026-09-30 follow-on**
/// ("all three disposition defaults take a literal operand"), which routed
/// `:-`/`:?` through `parse_disposition_literal`. That function returns
/// `ExprKind::Literal` unconditionally, and every other branch of
/// `parse_disposition_operator` builds one directly, so no other kind can
/// reach here. Confirmed cold by the coverage sweep before removal.
fn default_is_arith_literal(operand: Option<&Expr>) -> bool {
    let Some(op) = operand else {
        return false;
    };
    let ExprKind::Literal(s) = &op.kind else {
        return false;
    };
    crate::parse::parse_dd72_literal(s).is_some()
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
        // Statement recursion owns command substitutions and their slot scope.
        ExprKind::CommandSubstitution(_) => {}
        ExprKind::Subscript { index, .. } => walk_value_default(index, env, errors),
        ExprKind::Test(t) => walk_value_test(t, env, errors),
        // `$(( … ))` defaults are owned by check_arith_default_literals.
        ExprKind::Arithmetic(_) => {}
        _ => {}
    }
}

/// Check a `${…}` node, then descend into its cell index and default operand.
fn walk_pe_default(
    pe: &ParameterExpansion,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    check_int_pe_default(pe, span, env, errors);
    if let Some(index) = pe.index.as_deref() {
        walk_value_default(index, env, errors);
    }
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

/// A sexpr callee that is not a valid int producer, as a type error — or
/// `None` when the callee is legal.
///
/// The three answers are the three ways a `(name …)` can be wrong, and the
/// message has to differ because the advice does: a string intrinsic should
/// be captured with `$( )`, while a command (`echo`, `cd`, `break`) has no
/// value to capture in arithmetic at all. Both used to be the same silent
/// `0` — `is_sexpr_string_producer` carried its own list of twelve string
/// intrinsics and let every other compiler name through as an int producer.
///
/// Int intrinsics, user `-> int` functions, and a bare variable `(x)`
/// (arithmetic grouping, not a call) are all legal and return `None`.
fn sexpr_callee_error(name: &str, span: Span, env: &TypeEnv) -> Option<TypeError> {
    use crate::resolve::ArithDenotation as D;
    match crate::resolve::arith_denotation(name) {
        Some(D::Int) => None,
        Some(D::Str) => Some(string_callee_error(name, span)),
        Some(D::Statement) => Some(statement_callee_error(name, span)),
        // Not a name the compiler owns: a variable or a user function. Only a
        // user *string* function is wrong here.
        None if matches!(env.funcs.get(name), Some(VarType::String)) => {
            Some(string_callee_error(name, span))
        }
        None => None,
    }
}

/// The SB4030 text for a sexpr callee that yields a string.
fn string_callee_error(name: &str, span: Span) -> TypeError {
    TypeError {
        message: format!(
            "`{name}` can't be called in an arithmetic sexpr — it yields a \
             string, not an int. Capture it first with `$()`: `s=$({name} …)`, \
             then use `s` (or an `int` of it) in `$(( … ))`."
        ),
        span,
    }
}

/// The SB4030 text for a sexpr callee that is a statement — it denotes no
/// value at all, so there is nothing to capture and no conversion to offer.
fn statement_callee_error(name: &str, span: Span) -> TypeError {
    TypeError {
        message: format!(
            "`{name}` is a command, not a value — it denotes nothing in \
             arithmetic. Only an int intrinsic (`len`, `argc`, `getpid`, …) \
             or a `-> int` function can be called in `$(( … ))`."
        ),
        span,
    }
}

/// `${var:-default}` / `${var-default}` preserve the variable's type;
/// all other strip ops (`#`, `%`, `/`, etc.) produce strings.
///
/// DD#116: a `"strlen" | "arrlen" => VarType::Int` arm was deleted here
/// (2026-10-04) as a *rival*. It answered "a length is an int" a second time,
/// keyed on the `${#x}` sugar that DD#75 §4 retired; the live spellings
/// `$(len x)` / `$(strlen s)` are intrinsics, and `is_int_returning_intrinsic`
/// is where that rule lives. No *parsed* program can carry either operator —
/// `try_strlen_or_arrlen` emits `len_retired`, and the only AST producer of
/// `arrlen` is `transform/pipelines.rs`, downstream of this pass.
fn parameter_expansion_type(pe: &ParameterExpansion, env: &TypeEnv) -> VarType {
    let Some(op) = pe.operator.as_deref() else {
        return env.var_type(&pe.name);
    };
    match op {
        // `${var:-default}` / `${var-default}` preserve the variable's type.
        ":-" | "-" => env.var_type(&pe.name),
        // All other strip ops (`%`, `#`, `/`, …) produce strings.
        _ => VarType::String,
    }
}

/// A literal name or `$name`, not a display renderer or a splat decoder.
/// DD#100 Part D rejects `@name` in name operands before type checking.
pub(crate) fn expr_to_name(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => s.clone(),
        ExprKind::SimpleExpansion(v) => v.clone(),
        _ => String::new(),
    }
}

/// DD#100: structure and element type are both part of the signature.
#[derive(Clone, Copy, PartialEq, Eq)]
struct CollectionType {
    is_map: bool,
    element: VarType,
}

impl CollectionType {
    fn description(self) -> &'static str {
        match (self.is_map, self.element) {
            (false, VarType::String) => "string array",
            (false, VarType::Int) => "int array",
            (true, VarType::String) => "string map",
            (true, VarType::Int) => "int map",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct CollectionSignature {
    arity: usize,
    slots: [Option<CollectionType>; MAX_COLLECTION_SLOTS],
}

#[derive(Clone, Copy)]
enum SlotBinding {
    Collection(CollectionType),
    Forwarded(usize),
}

struct CollectionCall {
    caller: String,
    callee: String,
    args: Vec<Option<SlotBinding>>,
    span: Span,
}

fn infer_collection_signatures(
    program: &Program,
    errors: &mut Vec<TypeError>,
) -> HashMap<String, CollectionSignature> {
    let functions: HashSet<_> = program.function_defs().map(|f| f.name.clone()).collect();
    // Main and END share one global scope, including for earlier function bodies.
    let mut globals = scope_collections(&program.statements, "", &HashMap::new());
    if let Some(end) = &program.end_block {
        globals = scope_collections(end, "", &globals);
    }
    let mut calls = Vec::new();
    for stmts in std::iter::once(program.statements.as_slice()).chain(program.end_block.as_deref())
    {
        collect_collection_calls(stmts, "", &globals, &functions, &mut calls);
    }
    let signatures = propagate_collection_signatures(&calls);
    for call in &calls {
        check_collection_call(call, &signatures, errors);
    }
    signatures
}

fn propagate_collection_signatures(
    calls: &[CollectionCall],
) -> HashMap<String, CollectionSignature> {
    let mut signatures = HashMap::new();
    let mut changed = true;
    while changed {
        let previous = signatures.clone();
        for call in calls {
            propagate_collection_call(call, &previous, &mut signatures);
        }
        changed = signatures != previous;
    }
    signatures
}

fn scope_collections(
    stmts: &[Statement],
    owner: &str,
    inherited: &HashMap<String, CollectionType>,
) -> HashMap<String, CollectionType> {
    let mut visible = inherited.clone();
    walk_scoped_structural_stmts(stmts, owner, &mut |stmt, in_fn| {
        if in_fn == owner {
            record_collection_types(stmt, &mut visible);
        }
    });
    if !owner.is_empty() {
        visible.insert(
            "args".to_string(),
            CollectionType {
                is_map: false,
                element: VarType::String,
            },
        );
    }
    visible
}

fn record_collection_types(stmt: &Statement, out: &mut HashMap<String, CollectionType>) {
    if let StmtKind::Declaration(d) = &stmt.kind {
        record_declared_collection_types(d, out);
    }
    let StmtKind::Command(c) = &stmt.kind else {
        return;
    };
    let Some(target) = collection_fill_target(c) else {
        return;
    };
    let name = expr_to_name(target);
    if !name.is_empty() && !is_collection_slot(&name) {
        out.entry(name).or_insert(CollectionType {
            is_map: false,
            element: VarType::String,
        });
    }
}

fn record_declared_collection_types(d: &Declaration, out: &mut HashMap<String, CollectionType>) {
    let Some(is_map) = declared_collection_kind(d) else {
        return;
    };
    for a in &d.assignments {
        out.insert(
            a.name.clone(),
            CollectionType {
                is_map,
                element: d.var_type,
            },
        );
    }
}

fn collect_collection_calls(
    stmts: &[Statement],
    owner: &str,
    inherited: &HashMap<String, CollectionType>,
    functions: &HashSet<String>,
    out: &mut Vec<CollectionCall>,
) {
    let visible = scope_collections(stmts, owner, inherited);
    walk_scoped_structural_stmts(stmts, owner, &mut |stmt, in_fn| {
        if in_fn != owner {
            return;
        }
        collect_nested_collection_calls(stmt, owner, &visible, functions, out);
        record_collection_call(stmt, owner, &visible, functions, out);
        record_arith_collection_calls(stmt, owner, functions, out);
    });
}

fn collect_nested_collection_calls(
    stmt: &Statement,
    owner: &str,
    visible: &HashMap<String, CollectionType>,
    functions: &HashSet<String>,
    out: &mut Vec<CollectionCall>,
) {
    if let StmtKind::Function(f) = &stmt.kind {
        collect_collection_calls(&f.body, &f.name, visible, functions, out);
    }
    for body in stmt.cmdsub_bodies() {
        collect_collection_calls(body, owner, visible, functions, out);
    }
}

fn record_collection_call(
    stmt: &Statement,
    owner: &str,
    visible: &HashMap<String, CollectionType>,
    functions: &HashSet<String>,
    out: &mut Vec<CollectionCall>,
) {
    let StmtKind::Command(c) = &stmt.kind else {
        return;
    };
    let callee = expr_to_name(&c.name);
    if !functions.contains(&callee) {
        return;
    }
    let args = collection_args(&c.args)
        .iter()
        .map(|name| {
            visible
                .get(name)
                .copied()
                .map(SlotBinding::Collection)
                .or_else(|| {
                    (0..MAX_COLLECTION_SLOTS)
                        .position(|j| collection_slot_name(j) == *name)
                        .map(SlotBinding::Forwarded)
                })
        })
        .collect();
    out.push(CollectionCall {
        caller: owner.to_string(),
        callee,
        args,
        span: stmt.span,
    });
}

fn record_arith_collection_calls(
    stmt: &Statement,
    owner: &str,
    functions: &HashSet<String>,
    out: &mut Vec<CollectionCall>,
) {
    let mut seen = HashSet::new();
    let mut record = |name: &str, _: &[ArithExpr]| {
        if functions.contains(name) && seen.insert(name.to_string()) {
            out.push(CollectionCall {
                caller: owner.to_string(),
                callee: name.to_string(),
                args: vec![],
                span: stmt.span,
            });
        }
    };
    walk_stmt_arith_calls(stmt, &mut record);
}

fn walk_stmt_arith_calls(stmt: &Statement, record: &mut impl FnMut(&str, &[ArithExpr])) {
    walk_own_exprs(stmt, &mut |e| {
        if let ExprKind::Arithmetic(a) = &e.kind {
            walk_arith_funccalls(a, record);
        }
    });
    if let StmtKind::ArithTest(a) = &stmt.kind {
        walk_arith_funccalls(a, record);
    }
}

fn collection_call_types(
    call: &CollectionCall,
    signatures: &HashMap<String, CollectionSignature>,
) -> Vec<Option<CollectionType>> {
    call.args
        .iter()
        .map(|binding| match binding {
            Some(SlotBinding::Collection(kind)) => Some(*kind),
            Some(SlotBinding::Forwarded(j)) => {
                signatures.get(&call.caller).and_then(|s| s.slots[*j])
            }
            None => None,
        })
        .collect()
}

fn propagate_collection_call(
    call: &CollectionCall,
    previous: &HashMap<String, CollectionSignature>,
    out: &mut HashMap<String, CollectionSignature>,
) {
    let signature = out
        .entry(call.callee.clone())
        .or_insert(CollectionSignature {
            arity: call.args.len(),
            slots: [None; MAX_COLLECTION_SLOTS],
        });
    for (slot, kind) in signature
        .slots
        .iter_mut()
        .zip(collection_call_types(call, previous))
    {
        if slot.is_none() {
            *slot = kind;
        }
    }
}

fn check_collection_call(
    call: &CollectionCall,
    signatures: &HashMap<String, CollectionSignature>,
    errors: &mut Vec<TypeError>,
) {
    let signature = &signatures[&call.callee];
    if signature.arity != call.args.len() {
        errors.push(TypeError {
            message: format!(
                "function `{}` is called with both {} and {} collection arguments — \
                 a function has one inferred signature",
                call.callee,
                signature.arity,
                call.args.len()
            ),
            span: call.span,
        });
    }
    for (j, (expected, actual)) in signature
        .slots
        .iter()
        .zip(collection_call_types(call, signatures))
        .enumerate()
    {
        if let (Some(expected), Some(actual)) = (expected, actual) {
            check_collection_slot(*expected, actual, &call.callee, j, call.span, errors);
        }
    }
}

fn check_collection_slot(
    expected: CollectionType,
    actual: CollectionType,
    callee: &str,
    j: usize,
    span: Span,
    errors: &mut Vec<TypeError>,
) {
    if expected != actual {
        errors.push(TypeError {
            message: format!(
                "function `{callee}` collection slot `{}` has conflicting types: {} \
                 and {} — a function has one inferred signature; every call must agree",
                collection_slot_name(j),
                expected.description(),
                actual.description()
            ),
            span,
        });
    }
}

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

/// Type-check every body and return the checked types used by lowering.
pub fn type_check(program: &Program) -> Result<CheckedTypes, Vec<TypeError>> {
    let mut errors = Vec::new();
    let mut env = TypeEnv::new();

    for f in program.function_defs() {
        env.funcs.insert(f.name.clone(), f.return_type);
    }

    // DD#100: the full signature comes from the call sites, so infer it
    // before checking any body (a call may appear after the definition).
    env.slot_signatures = infer_collection_signatures(program, &mut errors);

    check_body_types(program, &mut env, &mut errors);

    // DD#58/Option C: a string producer used in an arithmetic
    // sexpr `(f …)` is a type error — it belongs in a `$()` capture, not
    // `$(( ))`. Covers the string intrinsics + user string
    // functions; closes the getenv/arg validator-bypass (they silently compiled
    // to 0 in a sexpr, sidestepping SB5020). bug-arith-sexpr-intrinsics-zero.
    run_stmt_pass(program, &env, &mut errors, check_sexpr_string_producers);

    // DG 2026-08-29: a value-context `${int_x:-D}` default must be
    // an int literal (SB4030) — see check_int_value_defaults. Separate from the
    // arith pass, which owns `$(( … ))` defaults.
    run_stmt_pass(program, &env, &mut errors, check_int_value_defaults);

    // A string operation on an int variable. Codegen asks the slot
    // for a string pointer and panics on the Int arm (`codegen/types.rs:29`),
    // whose comment asserts that arm is "not user-reachable input" — it was.
    run_stmt_pass(program, &env, &mut errors, check_int_string_ops);

    // DD#30: `[[ ]]` compares strings, so an int operand is an
    // implicit conversion — see check_test_operand_types.
    run_stmt_pass(program, &env, &mut errors, check_test_operand_types);

    // DD#74: an `-> int` function used as a predicate. Its own
    // pass rather than a hook in `check_if`, because the six boolean contexts
    // are spread across If/While/List/Pipeline and a single traversal is the
    // only way to apply the tail rule consistently.
    check_int_fn_predicates(program, &env, &mut errors);

    if errors.is_empty() {
        let collections = env
            .collections
            .into_iter()
            .filter(|name| env.vars.contains_key(name))
            .map(|name| {
                let is_map = env.assoc_arrays.contains(&name);
                (name, is_map)
            })
            .collect();
        Ok(CheckedTypes {
            vars: env.vars,
            collections,
        })
    } else {
        Err(errors)
    }
}

/// Forward references and loop-carried copies may refine a first binding's
/// default String to Int. Keep the prior facts, but not its binding order;
/// only the stable pass's diagnostics describe the completed type map.
fn check_body_types(program: &Program, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    loop {
        let previous = env.clone();
        env.bound.clear();
        let mut pending = Vec::new();
        check_program_bodies(program, env, &mut pending);
        if *env == previous {
            errors.extend(pending);
            return;
        }
    }
}

fn check_program_bodies(program: &Program, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        if !matches!(stmt.kind, StmtKind::Function(_)) {
            check_stmt(stmt, env, errors);
        }
    }
    for f in program.function_defs() {
        check_function(f, env, errors);
    }
}

/// Every pass uses the same function binding and visits each statement once.
fn run_stmt_pass(
    program: &Program,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
    pass: fn(&Statement, &TypeEnv, &mut Vec<TypeError>),
) {
    let mut scopes = HashMap::new();
    let mut visit = |stmt: &Statement, owner: &str| {
        let scope = scopes.entry(owner.to_string()).or_insert_with(|| {
            let mut scope = env.clone();
            if !owner.is_empty() {
                enter_function_slots(owner, &mut scope);
            }
            scope
        });
        pass(stmt, scope, errors);
    };
    for body in std::iter::once(program.statements.as_slice()).chain(program.end_block.as_deref()) {
        walk_scoped_stmts(body, "", &mut visit);
    }
}

/// Substring (`${n:1:2}`) is a string operation, so an int-typed subject is a
/// type error. The other string ops (`${n#…}`, `${n^^}`) are already rejected
/// at parse (SB4034, DD#65 pruned operators), which is why substring was the
/// one that reached codegen and panicked.
fn check_int_string_ops(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    crate::ir::walk_own_exprs(stmt, &mut |e| {
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
    crate::ir::walk_own_exprs(stmt, &mut |e| {
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

/// `[[ ]]` compares strings, so a whole-int operand there is an implicit
/// int→string conversion, which DD#30 rules out: the same SB4030 that
/// `str s=$n` gets. This covers every test operator, the DD#66 dispatch
/// subject and patterns, and `attest` predicates (the DD#93 Part B shorthand
/// `attest n -n` builds the same test). `expr_type` decides, so this agrees
/// with `check_type_match`: `"$n"` and `x$n` are strings and stay legal.
///
/// Only each operand's own type is checked, not its subtree: a capture inside
/// an operand is a body `run_stmt_pass` visits as a statement of its own.
fn check_test_operand_types(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    match &stmt.kind {
        StmtKind::Command(c) => check_command_test_operands(c, env, errors),
        StmtKind::Case(c) => check_dispatch_operands(c, env, errors),
        _ => {}
    }
}

/// The `[[ ]]` arguments of a command: a test, or an `attest` predicate.
fn check_command_test_operands(c: &Command, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let context = TestContext::of_command(c);
    for arg in &c.args {
        if let ExprKind::Test(t) = &arg.kind {
            crate::ir::walk_test_operands(t, &mut |e| check_test_operand(e, context, env, errors));
        }
    }
}

/// A DD#66 dispatch: its subject and every pattern.
fn check_dispatch_operands(c: &Case, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    check_test_operand(&c.value, TestContext::Dispatch, env, errors);
    for pattern in c.items.iter().flat_map(|item| &item.patterns) {
        check_test_operand(pattern, TestContext::Dispatch, env, errors);
    }
}

/// Where a `[[ ]]` operand sits, for the wording of its SB4030.
#[derive(Clone, Copy)]
enum TestContext {
    Test,
    Dispatch,
    Attest,
}

impl TestContext {
    fn of_command(c: &Command) -> Self {
        if expr_to_name(&c.name) == "attest" {
            TestContext::Attest
        } else {
            TestContext::Test
        }
    }
}

fn check_test_operand(
    operand: &Expr,
    context: TestContext,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    if expr_type(operand, env) != VarType::Int {
        return;
    }
    let hint = conversion_hint(VarType::Int, VarType::String);
    let message = match context {
        TestContext::Test => format!(
            "int value in a `[[ ]]` test, which compares strings{hint}, \
             or compare numbers with `(( … ))`"
        ),
        TestContext::Dispatch => {
            format!("int value in a `[[ ]]` dispatch, which matches strings{hint}")
        }
        TestContext::Attest => format!(
            "int value in an `attest` predicate, which tests strings{hint} \
             (a shorthand such as `attest n -n` is written out in full: \
             `attest n [[ -n \"$n\" ]]`)"
        ),
    };
    errors.push(TypeError {
        message,
        span: operand.span,
    });
}

/// `exit`, `wait` and `arg` take a *number*: a status, a pid, a positional
/// index. Each silently coerced a string operand through the runtime's atoi,
/// and `arg` was the worst of the three — `$(arg $s)` on a string yielded
/// **argv[0]**, a wrong answer rather than a diagnostic.
///
/// Only the first operand is an int sink. `arg N default` takes a string
/// default in slot 1, which is left alone.
fn int_sink_slots(name: &str) -> Option<usize> {
    match name {
        "exit" | "wait" | "arg" => Some(1),
        _ => None,
    }
}

/// Does this operand satisfy an int sink? An unquoted decimal literal does
/// (`exit 1`, `arg 2`, and signed forms like `exit -1`), as does anything the
/// type system already calls Int. A quoted number does **not**: quoting is the
/// int→string conversion everywhere else in sash, so `"1"` is a string here
/// for the same reason it is a string in a map key.
fn operand_satisfies_int_sink(arg: &Expr, env: &TypeEnv) -> bool {
    if let ExprKind::Literal(text) = &arg.kind {
        return is_int_sink_literal(text);
    }
    expr_type(arg, env) == VarType::Int
}

/// Reject a non-int operand to `exit` / `wait` / `arg` (SB4030).
fn check_int_sink_operands(c: &Command, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let name = expr_to_name(&c.name);
    let Some(slots) = int_sink_slots(&name) else {
        return;
    };
    for arg in c.args.iter().take(slots) {
        if operand_satisfies_int_sink(arg, env) {
            continue;
        }
        errors.push(TypeError {
            message: int_sink_message(&name, arg),
            span,
        });
    }
}

/// The SB4030 message for a non-int operand of an int sink, naming what the
/// operand actually is and the conversion that makes it accountable.
fn int_sink_message(name: &str, arg: &Expr) -> String {
    let (what, role) = (int_sink_operand_desc(arg), int_sink_role(name));
    format!(
        "`{name}` takes {role}, but {what} — a string is not implicitly a \
         number, and coercing one silently would answer a question the source \
         never asked. Convert explicitly with `$(( … ))`, which makes a \
         non-numeric value abort rather than read as 0."
    )
}

fn int_sink_role(name: &str) -> &'static str {
    match name {
        "exit" => "an exit status",
        "wait" => "a process id",
        _ => "a positional index",
    }
}

fn int_sink_operand_desc(arg: &Expr) -> String {
    match &arg.kind {
        ExprKind::Literal(text) if text.is_empty() => "the operand is empty".to_string(),
        ExprKind::Literal(text) => format!("`{text}` is not a decimal integer"),
        ExprKind::SimpleExpansion(v) => format!("`{v}` is a string variable"),
        ExprKind::SingleQuoted(_) | ExprKind::DoubleQuoted(_) => {
            "a quoted operand is string-typed".to_string()
        }
        ExprKind::CommandSubstitution(_) => "a command substitution is string-typed".to_string(),
        _ => "that operand is string-typed".to_string(),
    }
}

/// A redirect target is a filesystem path, so an int variable there is a type
/// error rather than a codegen panic.
fn check_int_redirect_target(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    if let StmtKind::Redirected(r) = &stmt.kind {
        for redir in &r.redirects {
            check_one_redirect_target(&redir.target, env, errors);
        }
    }
}

/// DD#119: a redirect target must be **provably non-empty** and string-typed.
///
/// The old rule asked "is this a scalar?", which is a syntactic proxy for the
/// property actually wanted and leaks both ways — it admitted an empty `$p`
/// (which reached runtime as `: No such file or directory`) and excluded a
/// `$(getpid)` that cannot possibly be empty. A disposition is the proof
/// mechanism: it is the one construct that makes emptiness accountable at the
/// point of use. `:?` proves non-empty by aborting otherwise; `:-X` proves it
/// when `X` is a non-empty constant. `${p:-}` and `${p:~}` are dispositions
/// that prove *nothing*, so the test is "does the result prove non-empty", not
/// "did you write a disposition".
///
/// An int needs no disposition — it is non-empty by type, always at least one
/// digit — but still needs double quotes, because that is how an int becomes a
/// string everywhere else in sash (DD#80 map keys, string-intrinsic operands).
fn check_one_redirect_target(target: &Expr, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    if let ExprKind::SimpleExpansion(name) = &target.kind {
        if env.var_type(name) == VarType::Int {
            errors.push(TypeError {
                message: format!(
                    "redirect target `${0}` is an int variable — a target is a path \
                     (string). Convert with `\"${{{0}}}\"`",
                    name
                ),
                span: target.span,
            });
            return;
        }
    }
    match redirect_target_proof(target, env, false) {
        Ok(true) => {}
        Ok(false) => errors.push(TypeError {
            message: "redirect target is empty — a redirect needs a filename \
                      (DD#119: a target must be provably non-empty)"
                .to_string(),
            span: target.span,
        }),
        Err(message) => errors.push(TypeError {
            message,
            span: target.span,
        }),
    }
}

/// One part of a redirect target. `Ok(true)` means this part proves the target
/// non-empty, `Ok(false)` means it is well-formed but proves nothing (an empty
/// literal), and `Err` means the part is not admissible at all.
///
/// `quoted` tracks whether we are inside a `"…"`, which is what licenses an int
/// read. A `Concat` does not set it; a `DoubleQuoted` does, and nesting keeps
/// it, so `"$p".log` has its `$p` quoted while `$p.log` does not.
fn redirect_target_proof(e: &Expr, env: &TypeEnv, quoted: bool) -> Result<bool, String> {
    match &e.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => Ok(!s.is_empty()),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            let inner = quoted || matches!(e.kind, ExprKind::DoubleQuoted(_));
            let mut proven = false;
            for p in parts {
                // Strict (DG, 2026-09-26): every variable part carries its own
                // obligation regardless of its literal neighbours. Under the
                // lenient reading `> $p.log` with an empty `p` writes to `.log`
                // — not the empty-filename bug, but a silent hidden file.
                proven |= redirect_target_proof(p, env, inner)?;
            }
            Ok(proven)
        }
        ExprKind::SimpleExpansion(name) => plain_read_proof(name, None, env, quoted),
        ExprKind::ParameterExpansion(pe) => disposition_proof(pe, env, quoted),
        ExprKind::Subscript { name, .. } => Err(cell_read_message(name)),
        // A command substitution, arithmetic or array literal never reaches
        // here: SB4049 rejects it at parse, and DD#119 does not weaken that.
        _ => Ok(false),
    }
}

/// `${name:OP…}` as a redirect target. Only a disposition that *guarantees* a
/// non-empty result discharges the obligation, which is why `:-` with an empty
/// default and `:~` are rejected despite being dispositions: the test is
/// "does the result prove non-empty", not "did you write a disposition".
fn disposition_proof(pe: &ParameterExpansion, env: &TypeEnv, quoted: bool) -> Result<bool, String> {
    match pe.operator.as_deref() {
        None => plain_read_proof(&pe.name, pe.index.as_deref(), env, quoted),
        Some(":?") => Ok(true),
        Some(":-") => match pe.operand.as_deref() {
            Some(d) if operand_is_nonempty_const(d) => Ok(true),
            _ => Err(format!(
                "redirect target `${{{0}:-…}}` has an empty default, so it \
                 proves nothing — a target must be provably non-empty. Use \
                 `${{{0}:?message}}` to abort when it is unset, or give the \
                 default a filename",
                pe.name
            )),
        },
        // `:~` cannot arrive here. An elide is legal only where a token may
        // disappear (DD#91), and a redirect target is never a variadic list
        // position, so SB5046 in `validate` rejects every spelling of it —
        // braced, brace-less, quoted, cell-subject, `>`/`>>`/`<`/`2>`/`exec`
        // — one pass earlier. Reaching this arm would mean that rule silently
        // stopped applying, which is a compiler bug and not a user error, so
        // it halts rather than explaining itself politely.
        Some(":~") => panic!(
            "ICE: `${{{}:~}}` reached DD#119's redirect-target proof; SB5046 \
             should have rejected the elide in validate",
            pe.name
        ),
        // `op` may be an INTERNAL name (`substring`, `glob_replace`, …) rather
        // than anything the author typed, so it must never be pasted into
        // `${…}` as though it were syntax.
        Some(op) => Err(format!(
            "redirect target `${{{0}…}}` may be empty — {1} does not prove a \
             filename. Use `${{{0}:?message}}` or `${{{0}:-default}}`",
            pe.name,
            operator_noun(op)
        )),
    }
}

/// How to name a parameter operator to a user. The parser stores some
/// operators under an internal name (`${s:0:2}` is `"substring"`), so echoing
/// `pe.operator` into a diagnostic invents a spelling nobody wrote. Anything
/// unmapped is quoted rather than inlined, which keeps the worst case a
/// puzzling noun instead of fabricated syntax.
fn operator_noun(op: &str) -> String {
    match op {
        "substring" => "a substring `:off:len`".to_string(),
        _ => format!("`{op}`"),
    }
}

/// The one message for an undispositioned cell read, shared by the bare
/// `$a[i]` and braced `${a[i]}` spellings so they cannot drift apart.
fn cell_read_message(name: &str) -> String {
    format!(
        "redirect target `${0}[…]` is an undispositioned cell read — a cell \
         behaves like a string variable and may be empty. Use \
         `${{{0}[…]:?message}}`",
        name
    )
}

/// A brace-less `$x` or an operator-less `${x}` / `${a[i]}` — a plain read with
/// no disposition. Admissible only for an int, and only double-quoted.
fn plain_read_proof(
    name: &str,
    index: Option<&Expr>,
    env: &TypeEnv,
    quoted: bool,
) -> Result<bool, String> {
    if index.is_some() {
        return Err(cell_read_message(name));
    }
    if env.var_type(name) == VarType::Int {
        return if quoted {
            Ok(true)
        } else {
            Err(format!(
                "redirect target `${0}` is an int — it is non-empty by type but \
                 is not text. Double-quote it to convert: `\"${{{0}}}\"`",
                name
            ))
        };
    }
    Err(format!(
        "redirect target `${0}` may be empty — a redirect to an empty filename \
         fails at runtime. Say what happens when it is absent: `${{{0}:?message}}` \
         to abort, or `${{{0}:-default}}`",
        name
    ))
}

/// Is this `:-` default itself a non-empty constant? A default that is another
/// variable read only moves the obligation, so it does not discharge it.
fn operand_is_nonempty_const(d: &Expr) -> bool {
    match &d.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => !s.is_empty(),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            parts.iter().any(operand_is_nonempty_const)
        }
        _ => false,
    }
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
        // Deliberately does NOT descend into `$( … )`. `check_stmt` walks
        // `cmdsub_bodies()` unconditionally and runs the FULL per-statement
        // check on each inner statement; descending here as well checked every
        // substitution in a command argument twice, so a bad operand reported
        // its diagnostic twice (and two bad arguments, four times).
        _ => {}
    }
}

fn check_subscripts_in_parts(parts: &[Expr], env: &TypeEnv, errors: &mut Vec<TypeError>) {
    for p in parts {
        check_subscript_types(p, env, errors);
    }
}

/// A subscript index must already carry the collection's key type, on the read
/// path and on an assignment LHS alike — the two surfaces now parse the same
/// index grammar (`parse_split_subscript_index`), so a rule enforced on one is
/// enforceable on the other and the advice can be the same sentence. It could
/// not be before: the LHS took only a bare `$var`, so the read path's "convert
/// it with `$(( … ))`" was a recommendation that did not compile there.
///
/// An indexed array is indexed by *number* (DD#80), so the index must be
/// int-typed: an integer literal, an `int`-declared variable, or `$(( … ))`.
/// A map is keyed by *string* (DD#80: "any string is a valid key"), so an
/// int-typed variable must be rendered with quotes — DD#105 requires the
/// crossing to be written, not inferred.
fn check_subscript_key_kind(
    name: &str,
    index: &Expr,
    span: Span,
    env: &TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    // Subscripting a non-collection is SB5068 (read and write paths alike), so
    // `name` is always a collection by the time this runs.
    if env.is_slot(name) {
        return;
    }
    let message = if env.assoc_arrays.contains(name) {
        map_key_error(name, index, env)
    } else {
        (!is_int_index(index, env)).then(|| array_index_error(name, index, env))
    };
    if let Some(message) = message {
        errors.push(TypeError { message, span });
    }
}

/// Is this index already a number? DD#80's type-aware clause says an indexed
/// array's index "must yield a number", and this is the positive form of that
/// test.
///
/// It asks a *positive* question. The predicate it replaced asked the opposite
/// one — "is this a *string* key" — which could only name the forms it happened
/// to know were strings and defaulted the rest to accepted. That let a bare
/// `i=1` index an array while `str s=abc` did not, though neither is int-typed:
/// `var_type` ends in `unwrap_or(String)`, so "declared a string" and "never
/// recorded" are the same answer and no negative test can tell them apart.
/// Asking for Int is a fact rather than an absence, so the ambiguity stops
/// mattering.
///
/// The typing itself is `expr_type`'s job, not this function's. Delegating
/// picks up every rule already written there — `$(( … ))` is Int (the
/// conversion DD#80 names), a cell of an `int array` is Int (DD#76, so the
/// nested `${a[${a[0]}]}` still indexes), a `:-`/`:?` disposition keeps its
/// *subject's* type (so a bare disposition on a string does not quietly become
/// a number, which is the whole point of requiring `$(( … ))`). Only the bare
/// integer literal needs saying here, because `expr_type` types it String —
/// bash does, and nothing else in the language wants `2` to be an int by
/// itself.
fn is_int_index(expr: &Expr, env: &TypeEnv) -> bool {
    (is_integer_literal(expr) && is_numeric_literal(expr)) || expr_type(expr, env) == VarType::Int
}

/// The SB4030 text for an index that is not int-typed on an indexed array.
///
/// A *variable* index gets different advice from a literal one. `conversion_hint`
/// learned this lesson already: recommending `${a[$i]}` to someone who wrote
/// `${a[$s]}` sends them from one SB4030 to the same SB4030, because the
/// spelling was never the problem — the operand's type was.
fn array_index_error(name: &str, index: &Expr, env: &TypeEnv) -> String {
    // A capture needs its own advice. The generic text below offers
    // `$(( … ))`, and `${a[$(( $(f) ))]}` is SB4035 — a capture is not an
    // arithmetic operand either. Name a route that works, an int-returning
    // function (DD#74), rather than send the author to a second error.
    if matches!(index.kind, ExprKind::CommandSubstitution(_)) {
        return format!(
            "capture as an index on indexed array '{name}' — `$( … )` yields a \
             string, and an int-indexed array is indexed by number (DD#80). \
             Declare the command as an int-returning function and call it in \
             arithmetic: `fn f -> int {{ … }}` then `${{{name}[$(( (f) ))]}}`; \
             declare `map {name}` for string keys"
        );
    }
    let Some(v) = index_var_name(index) else {
        return format!(
            "non-integer index on indexed array '{name}' — an int-indexed array is \
             indexed by number (DD#80). Use an integer literal, an `int` variable \
             (`${{{name}[$i]}}`), or `$(( … ))`; declare `map {name}` for string keys"
        );
    };
    // A variable with no recorded type is not a mistake the author can see by
    // reading the subscript, so say which fact is missing rather than only
    // which spelling is wrong.
    let why = if env.is_declared_str(v) {
        format!("`${v}` is string-typed")
    } else {
        format!("`${v}` is not int-typed")
    };
    format!(
        "non-integer index on indexed array '{name}' — {why}, and an int-indexed \
         array is indexed by number (DD#80). Convert it: \
         `${{{name}[$(( ${{{v}:?not a number}} ))]}}` (abort if it isn't an integer) \
         or `${{{name}[$(( ${{{v}:-0}} ))]}}` (0 if it isn't). Declaring `int {v}=…` \
         also works when the value is an integer by construction; declare \
         `map {name}` for string keys"
    )
}

/// The SB4030 text for a key that is not string-typed on a map, or `None` when
/// the key is acceptable.
fn map_key_error(name: &str, index: &Expr, env: &TypeEnv) -> Option<String> {
    // DD#33: a bare integer *literal* is rejected because it reads like an
    // array index, not because a map cannot hold "5" as a key.
    if is_integer_literal(index) {
        return Some(format!(
            "integer key on map '{name}' — maps use string keys. Use ${{{name}[\"key\"]}}"
        ));
    }
    if expr_type(index, env) != VarType::Int {
        return None;
    }
    // This rejects every int-typed key, not only a bare `$i`. The rule is about
    // the key's *type*, so narrowing it to variables would leave
    // `${m[$(( 1+1 ))]}` and a cell of an `int array` (DD#76) crossing in
    // silence while `${m[$i]}` was told to quote. Quoting compiles for all of
    // them, so the rule leaves no form without a remedy.
    let Some(v) = index_var_name(index) else {
        return Some(format!(
            "int-typed key on map '{name}' \u{2014} a map is keyed by string, and \
             int\u{2192}string does not happen silently (DD#105). Quote the expansion \
             to convert it: `${{{name}[\"\u{2026}\"]}}`"
        ));
    };
    // DD#105: a string context applied to an int-typed operand is a type error,
    // not a silent render. The quotes are the conversion, as they are for a
    // redirect target and for `$(strlen "$n")`.
    Some(format!(
        "int variable `${v}` as a key on map '{name}' — a map is keyed by string, and \
         int→string does not happen silently (DD#105). Write the conversion: \
         `${{{name}[\"${v}\"]}}`"
    ))
}

/// The variable a bare `$v` / `${v}` index names, if the index is one.
fn index_var_name(index: &Expr) -> Option<&str> {
    match &index.kind {
        ExprKind::SimpleExpansion(v) => Some(v.as_str()),
        ExprKind::ParameterExpansion(pe) if pe.operator.is_none() => Some(pe.name.as_str()),
        _ => None,
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
    let name = expr_to_name(arg);
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
    let name = expr_to_name(arr);
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
    let name = expr_to_name(arg);
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

/// An unsigned decimal spelling, even outside the runtime integer range.
/// Map-key disambiguation is about spelling, not whether the number fits.
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
        // structural_bodies(). Covers For, While, Function, Case (no
        // per-arm pre-work) and the wrapping StmtKinds Pipeline,
        // Group, List, Subshell, Redirected, Suppressed — so
        // wrapping a statement in grouping syntax doesn't silently
        // bypass type checking (journal 056). structural_bodies()
        // returns empty for leaf StmtKinds (Assignment,
        // Declaration, Command, Comment, ArithTest), which all
        // have their own arm above.
        _ => check_stmt_bodies(stmt, env, errors),
    }
    // A `$( … )` body is an anonymous function body (DD#86), so every
    // statement in it is type-checked like one written outside. This is
    // unconditional rather than part of the `_` arm because the arms that do
    // NOT reach `check_stmt_bodies` — `Command`, `Declaration`, `Assignment`
    // — are exactly the ones that carry a substitution. `str s=$(unset zz)`
    // is a Declaration, and `unset` on a scalar went unchecked because of it.
    //
    // `env` is shared with the body, not scoped to it. That matches how
    // function bodies are already treated here: `check_function` saves and
    // restores only `args` and the DD#100 slot kinds, never ordinary names.
    for body in stmt.cmdsub_bodies() {
        for s in body {
            check_stmt(s, env, errors);
        }
    }
}

/// The nested-body half of [`check_stmt`]. Deliberately `structural_bodies`:
/// this walk MUTATES `env`, so descending into a `$( … )` body would leak the
/// body's local names into the enclosing scope (DD#86) unless it opened a
/// child scope the way `check_function` does.
fn check_stmt_bodies(stmt: &Statement, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    for s in stmt.structural_bodies().into_iter().flatten() {
        check_stmt(s, env, errors);
    }
}

fn check_assignment(a: &Assignment, span: Span, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    record_bare_collection_value(a, env);
    // The VALUE, not just the target's index. A command walks its arguments
    // with `check_subscript_types` and an assignment walked nothing, so the
    // same `${a[foo]}` was a clean SB4030 as `echo "${a[foo]}"` and an ICE in
    // codegen as `v="${a[foo]}"`.
    check_subscript_types(&a.value, env, errors);
    // DD#75 §4: a subscript index is a numeric literal or a `$`-sigilled
    // value; a bare word is a string key (map-only). Enforce on the write
    // path as on the read path (check_subscript_key_kind), so `a[i]=…` on an
    // indexed array is SB4030 just like the read `${a[i]}`.
    if let Some(idx) = &a.index {
        check_subscript_key_kind(&a.name, idx, span, env, errors);
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
    if !env.bound.contains(&f.variable) {
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
    if !env.bound.contains(&a.name) {
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
    type_mismatch(ltype, rtype, name, span, errors);
}

fn type_mismatch(
    ltype: VarType,
    rtype: VarType,
    name: &str,
    span: Span,
    errors: &mut Vec<TypeError>,
) {
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
        if let Some(v) = &a.value {
            check_subscript_types(v, env, errors);
        }
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
        // DD#99: `readline` is the scalar half of the family (SB5057 pins it to
        // exactly one bare target), so its target type wants the opposite check.
        "readline" => check_scalar_fill_target(c, span, env, errors),
        _ => {}
    }
    if collection_fill_target(c).is_some() {
        register_collection_target(c, span, env, errors);
    }

    // DD#33: check subscript key types in command args
    check_subscript_types(&c.name, env, errors);
    check_join_operand(c, span, env, errors);
    check_keys_operand(c, span, env, errors);
    check_len_operand(c, span, env, errors);
    check_string_intrinsic_operands(c, span, env, errors);
    check_int_sink_operands(c, span, env, errors);
    for arg in &c.args {
        check_subscript_types(arg, env, errors);
    }
}

/// The same out-array role drives checking and signature inference.
fn collection_fill_target(c: &Command) -> Option<&Expr> {
    match expr_to_name(&c.name).as_str() {
        "argv" | "read" => c.args.last(),
        "match" | "matchall" if c.args.len() == 3 => c.args.last(),
        "keys" if c.args.len() == 2 => c.args.last(),
        _ => None,
    }
}

/// DD#99 `readline NAME` — the mirror of `register_collection_target`.
/// `readline` reads one line into a **scalar**; given a collection it used to
/// compile and silently do nothing, leaving the target at `len=0` and
/// discarding the line. Silent data loss, so it is rejected here.
///
/// An undeclared target stays legal — it becomes a scalar, which is the point.
fn check_scalar_fill_target(
    c: &Command,
    span: Span,
    env: &mut TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    let Some(first) = c.args.first() else {
        return;
    };
    let name = expr_to_name(first);
    if name.is_empty() {
        return;
    }
    if !env.collections.contains(&name) {
        register_readline_target(&name, span, env, errors);
        return;
    }
    let (what, fix) = if env.assoc_arrays.contains(&name) {
        (
            "a map",
            "read an array and convert it: `read a` then `map m = @{a:-EXTRA}`",
        )
    } else {
        (
            "an indexed array",
            "use `read` to split NUL-separated fields into it",
        )
    };
    errors.push(TypeError {
        message: format!(
            "cannot fill the collection `{name}` with `readline` — `readline` \
             reads one line into a scalar and `{name}` is {what}; {fix}"
        ),
        span,
    });
}

fn register_readline_target(
    name: &str,
    span: Span,
    env: &mut TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    if env.var_type(name) == VarType::Int {
        errors.push(TypeError {
            message: format!(
                "cannot fill int variable `{name}` with `readline` — this builtin \
                 produces a string; read a string and convert explicitly"
            ),
            span,
        });
    } else {
        env.declare(name, VarType::String);
    }
}

/// DD#60 `argv [N] ARR`, DD#99 `read [SRC] ARR`, DD#63 `match`/`matchall
/// S P ARR` — each fills, and auto-declares, the array named by its LAST
/// argument.
///
/// DD#86 governs the auto-declare: *"a variable should never change type"*.
/// Binding a collection onto a name already introduced as a scalar is
/// therefore an error, exactly as `int x=5; for x in a b` is. It used to be
/// silent, and the consequences were severe — the transform put a `SashTable*`
/// into the scalar's slot while the type environment still said `str`, so
/// `/bin/echo $x` handed `execve` a pointer to the table and printed a live
/// heap address; the `int` spelling reached codegen and panicked with
/// `expected string var, got int`.
fn register_collection_target(
    c: &Command,
    span: Span,
    env: &mut TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    let Some(last) = collection_fill_target(c) else {
        return;
    };
    let name = expr_to_name(last);
    if name.is_empty() || env.is_slot(&name) {
        return;
    }
    if let Some(message) = collection_fill_error(c, &name, env) {
        errors.push(TypeError { message, span });
        return;
    }
    env.declare(&name, VarType::String);
    env.collections.insert(name);
}

fn collection_fill_error(c: &Command, name: &str, env: &TypeEnv) -> Option<String> {
    if env.vars.contains_key(name) && !env.collections.contains(name) {
        return Some(format!(
            "cannot fill the collection `{}` with `{}` — `{}` is already a \
                 {} variable, and a variable does not change type (DD#86); \
                 fill a new name, or declare `array {}` instead",
            name,
            expr_to_name(&c.name),
            name,
            type_name(env.var_type(name)),
            name
        ));
    }
    // A map is in `collections` too, so the guard above waves it through. DD#99
    // settles that it must not be: a bulk fill writes the DD#33 raw-8-byte-i64
    // keys 0, 1, 2 …, and writing those into a string-keyed table yields a key
    // of eight NUL bytes — a DD#87 breach reachable from ordinary safe code.
    if env.assoc_arrays.contains(name) {
        return Some(format!(
            "cannot fill the map `{}` with `{}` — a bulk fill writes the \
                 indexed keys 0, 1, 2 …, not the string keys a map holds; \
                 DD#99: maps are built by assignment, not by `{}`. Fill an \
                 indexed array and convert it pairwise: `map {} = @{{a:-EXTRA}}`",
            name,
            expr_to_name(&c.name),
            expr_to_name(&c.name),
            name
        ));
    }
    if env.var_type(name) == VarType::Int {
        return Some(format!(
            "cannot fill int collection `{name}` with `{}` — this builtin \
                 produces string elements; fill a string array and convert explicitly",
            expr_to_name(&c.name)
        ));
    }
    None
}

/// DD#61: the attest form must match the attested entity's kind.
/// - scalar `attest v [[…]]` requires a scalar (arrays/maps need attest-all);
/// - one-binder `attest x in a [[…]]` requires an indexed array;
/// - two-binder `attest k v in m [[…]]` requires a map.
fn check_attest_form(c: &Command, span: Span, env: &mut TypeEnv, errors: &mut Vec<TypeError>) {
    let Some((form, pred)) = classify_attest(&c.args) else {
        return;
    };
    if let Some(message) = attest_form_error(&form, env) {
        errors.push(TypeError { message, span });
    }
    if let AttestPredicate::ArithConvert { target, .. } = pred {
        check_attest_convert_target(target, span, env, errors);
    }
}

fn check_attest_convert_target(
    target: &str,
    span: Span,
    env: &mut TypeEnv,
    errors: &mut Vec<TypeError>,
) {
    if env.is_collection(target) {
        errors.push(TypeError {
            message: format!(
                "attest conversion target `{target}` must be an int scalar, not a collection"
            ),
            span,
        });
        return;
    }
    if !env.bound.contains(target) {
        env.declare(target, VarType::Int);
        return;
    }
    let ltype = env.var_type(target);
    if ltype != VarType::Int {
        type_mismatch(ltype, VarType::Int, target, span, errors);
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

/// DD#61: a scalar `attest NAME` whose NAME is a collection.
///
/// DD#116: the map/array split is a *loud-vs-silent* backstop. Without it a
/// scalar attest on a map would advise `attest x in m`, the array binder form,
/// which is itself an error — so the user follows the advice and gets a second
/// diagnostic. The arm is cold because `type_check` sees few map attests, not
/// because it cannot run.
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

/// DD#30: both whole and subscripted targets must name collections.
fn check_unset_args(c: &Command, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    for target in crate::transform::unset_targets(c) {
        let name = target
            .split_once('[')
            .map_or(target.as_str(), |(base, _)| base);
        check_unset_target(name, span, env, errors);
    }
}

fn check_unset_target(name: &str, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    if env.is_collection(name) || env.is_slot(name) {
        return;
    }
    errors.push(TypeError {
        message: format!("unset on scalar '{name}' is not allowed — use {name}=\"\" to clear",),
        span,
    });
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
    enter_function_slots(&f.name, env);
    for s in &f.body {
        check_stmt(s, env, errors);
    }
    for j in 0..MAX_COLLECTION_SLOTS {
        let name = collection_slot_name(j);
        env.vars.remove(&name);
        env.bound.remove(&name);
        env.assoc_arrays.remove(&name);
        env.known_slots.remove(&name);
    }
    // SB5071 permits definitions only at top level: no enclosing slot scope.
    env.collections.remove("args");
}

/// Every checker binds the same full signature, including the element types.
fn enter_function_slots(fname: &str, env: &mut TypeEnv) {
    env.collections.insert("args".to_string());
    let slots = env
        .slot_signatures
        .get(fname)
        .map_or([None; MAX_COLLECTION_SLOTS], |s| s.slots);
    for (j, kind) in slots.into_iter().enumerate() {
        let name = collection_slot_name(j);
        env.vars.remove(&name);
        env.bound.remove(&name);
        env.assoc_arrays.remove(&name);
        env.known_slots.remove(&name);
        if let Some(kind) = kind {
            bind_function_slot(name, kind, env);
        }
    }
}

fn bind_function_slot(name: String, kind: CollectionType, env: &mut TypeEnv) {
    env.declare(&name, kind.element);
    if kind.is_map {
        env.assoc_arrays.insert(name.clone());
    }
    env.known_slots.insert(name);
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
    for body in std::iter::once(program.statements.as_slice()).chain(program.end_block.as_deref()) {
        walk_stmt_tree(body, &mut |stmt| {
            stmt_predicate_positions(stmt, env, &mut found)
        });
    }
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
        ExprKind::Literal(s) => !s.starts_with('+') && s.parse::<i64>().is_ok(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ParameterExpansion;

    #[test]
    #[should_panic(expected = "checked binding cannot change type")]
    fn bound_type_cannot_change() {
        let mut env = TypeEnv::new();
        env.declare("n", VarType::Int);
        env.declare("n", VarType::String);
    }

    #[test]
    fn unknown_collection_bindings_do_not_invent_a_slot_type() {
        let call = CollectionCall {
            caller: "uncalled".to_string(),
            callee: "forwarded".to_string(),
            args: vec![None, Some(SlotBinding::Forwarded(0))],
            span: Span::new(1, 1),
        };
        let signatures = propagate_collection_signatures(std::slice::from_ref(&call));
        let signature = &signatures["forwarded"];
        assert_eq!(signature.arity, 2);
        assert!(signature.slots.iter().all(Option::is_none));
        let mut errors = Vec::new();
        check_collection_call(&call, &signatures, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn compound_values_are_not_simple_names() {
        let value = Expr {
            kind: ExprKind::Arithmetic(Box::new(ArithExpr::Literal(1))),
            span: Span::new(1, 1),
        };
        assert_eq!(expr_to_name(&value), "");
    }

    #[test]
    fn only_literal_nodes_are_arithmetic_defaults() {
        let quoted = Expr {
            kind: ExprKind::SingleQuoted("7".to_string()),
            span: Span::new(1, 1),
        };
        assert!(!default_is_arith_literal(Some(&quoted)));
    }

    #[test]
    fn parser_shadowed_substring_operands_are_left_to_the_parser() {
        for operand in [
            None,
            Some(Box::new(Expr {
                kind: ExprKind::SimpleExpansion("offset".to_string()),
                span: Span::new(1, 1),
            })),
        ] {
            let stmt = Statement {
                kind: StmtKind::Command(Command {
                    name: Expr::literal("echo", Span::new(1, 1)),
                    args: vec![Expr::param(
                        "s",
                        Some("substring"),
                        operand,
                        Span::new(1, 1),
                    )],
                }),
                span: Span::new(1, 1),
            };
            let mut errors = Vec::new();
            check_substring_operand(&stmt, &mut errors);
            assert!(errors.is_empty(), "{errors:?}");
        }
    }

    #[test]
    fn redirect_constant_proof_requires_a_nonempty_constant_part() {
        let span = Span::new(1, 1);
        let variable = Expr {
            kind: ExprKind::SimpleExpansion("fallback".to_string()),
            span,
        };
        let quoted = Expr {
            kind: ExprKind::DoubleQuoted(vec![variable.clone(), Expr::literal("file", span)]),
            span,
        };
        let concatenation = Expr {
            kind: ExprKind::Concat(vec![quoted]),
            span,
        };
        assert!(operand_is_nonempty_const(&concatenation));
        assert!(!operand_is_nonempty_const(&variable));
    }

    #[test]
    fn internal_operator_names_are_quoted_not_invented_as_source() {
        assert_eq!(operator_noun("internal_operator"), "`internal_operator`");
    }

    #[test]
    fn equal_types_need_no_conversion_hint() {
        for ty in [VarType::String, VarType::Int] {
            assert_eq!(conversion_hint(ty, ty), "");
        }
    }

    #[test]
    fn unreconstructible_sexpr_operands_do_not_invent_a_command() {
        let mut errors = Vec::new();
        let operand = ArithExpr::BinOp {
            left: Box::new(ArithExpr::Literal(1)),
            op: IrArithOp::Add,
            right: Box::new(ArithExpr::Literal(2)),
        };
        check_sexpr_intrinsic_operands(
            "len",
            &[operand],
            Span::new(1, 1),
            &TypeEnv::new(),
            &mut errors,
        );
        assert!(errors.is_empty(), "{errors:?}");
    }

    /// DD#119's redirect-target proof has no `:~` case to explain, because an
    /// elide in a non-variadic position never survives `validate` (SB5046).
    /// Eight spellings were probed — braced, brace-less, quoted, cell-subject,
    /// `>`, `>>`, `<`, `2>`, `exec >` — and every one is rejected a pass
    /// earlier. The arm is therefore a should-never-run ICE, and only a direct
    /// call can reach it. Confirms it halts instead of returning a message a
    /// user would have to be given a way to see.
    #[test]
    fn elide_redirect_target_is_loud_ice() {
        let pe = ParameterExpansion {
            name: "p".to_string(),
            operator: Some(":~".to_string()),
            operand: None,
            index: None,
            taint_off: false,
        };
        let env = TypeEnv::new();
        let caught = std::panic::catch_unwind(|| disposition_proof(&pe, &env, false));
        assert!(
            caught.is_err(),
            "`:~` reaching DD#119's proof means SB5046 stopped rejecting the \
             elide — a compiler bug, which must be loud"
        );
    }
}
