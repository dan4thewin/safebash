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
fn check_sexpr_string_producers(stmt: &Statement, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    walk_all_exprs(stmt, &mut |e| {
        if let ExprKind::Arithmetic(a) = &e.kind {
            check_arith_sexpr(a, e.span, env, errors);
        }
    });
    if let StmtKind::ArithTest(a) = &stmt.kind {
        check_arith_sexpr(a, stmt.span, env, errors);
    }
    for body in stmt.child_bodies() {
        for s in body {
            check_sexpr_string_producers(s, env, errors);
        }
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
                "string key on indexed array '{}' — use integer indices. Declare as `map` for string keys",
                name,
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
fn join_operand_name(expr: &Expr) -> String {
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
        ExprKind::Literal(s) => !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit()),
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
    let cmd_name = expr_to_name(&c.name);
    if cmd_name == "unset" {
        check_unset_args(c, span, env, errors);
    }
    if cmd_name == "exit" {
        check_exit_arg(c, span, env, errors);
    }
    if cmd_name == "bless" {
        check_bless_form(c, span, env, errors);
    }
    if cmd_name == "argv" {
        // DD#60: `argv [N] a` fills (and auto-declares) the trailing array
        // as an indexed collection — register it so a later bless / len /
        // iteration sees an array, not a scalar.
        if let Some(arr) = c.args.last() {
            env.collections.insert(expr_to_name(arr));
        }
    }

    // DD#33: check subscript key types in command args
    check_join_operand(c, span, env, errors);
    check_keys_operand(c, span, env, errors);
    for arg in &c.args {
        check_subscript_types(arg, env, errors);
    }
}

/// DD#61: the bless form must match the blessed entity's kind.
/// - scalar `bless v [[…]]` requires a scalar (arrays/maps need bless-all);
/// - one-binder `bless x in a [[…]]` requires an indexed array;
/// - two-binder `bless k v in m [[…]]` requires a map.
fn check_bless_form(c: &Command, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let Some((form, _)) = classify_bless(&c.args) else {
        return;
    };
    if let Some(message) = bless_form_error(&form, env) {
        errors.push(TypeError { message, span });
    }
}

/// Returns the form/kind-mismatch error message for a bless, or `None`
/// when the form matches the entity's declared kind.
fn bless_form_error(form: &BlessForm, env: &TypeEnv) -> Option<String> {
    match form {
        BlessForm::Scalar { var } => bless_scalar_form_error(var, env),
        BlessForm::Indexed { binder, array } => bless_indexed_form_error(binder, array, env),
        BlessForm::Map { key, val, map } => bless_map_form_error(key, val, map, env),
    }
}

fn bless_scalar_form_error(var: &str, env: &TypeEnv) -> Option<String> {
    if !env.collections.contains(var) {
        return None;
    }
    let (kind, binders) = if env.assoc_arrays.contains(var) {
        ("map", "k v")
    } else {
        ("array", "x")
    };
    Some(format!(
        "scalar bless on {kind} '{var}' — use bless-all: `bless {binders} in {var} [[ … ]]`"
    ))
}

fn bless_indexed_form_error(binder: &str, array: &str, env: &TypeEnv) -> Option<String> {
    if !env.collections.contains(array) {
        Some(format!(
            "`bless {binder} in {array}` requires an array or map — '{array}' is a scalar; \
             use scalar bless `bless {array} [[ … ]]`"
        ))
    } else if env.assoc_arrays.contains(array) {
        Some(format!(
            "'{array}' is a map — bless-all needs two binders: `bless {binder} v in {array} [[ … ]]`"
        ))
    } else {
        None
    }
}

fn bless_map_form_error(key: &str, val: &str, map: &str, env: &TypeEnv) -> Option<String> {
    if env.assoc_arrays.contains(map) {
        return None;
    }
    let what = if env.collections.contains(map) {
        "an indexed array — use one binder"
    } else {
        "a scalar — use scalar bless"
    };
    Some(format!(
        "`bless {key} {val} in {map}` requires a map — '{map}' is {what}: \
         `bless {key} in {map} [[ … ]]`"
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

/// `exit $intvar` is rejected because exit codes are bounded 0..255
/// and an int value may exceed that range. Suggest a clamping form.
fn check_exit_arg(c: &Command, span: Span, env: &TypeEnv, errors: &mut Vec<TypeError>) {
    let Some(arg) = c.args.first() else {
        return;
    };
    if expr_type(arg, env) != VarType::Int {
        return;
    }
    let ExprKind::SimpleExpansion(var) = &arg.kind else {
        return;
    };
    errors.push(TypeError {
        message: format!(
            "exit with int variable `${}` — value may exceed 0-255. \
             Use `exit $(({}<256?{}:1))`",
            var, var, var,
        ),
        span,
    });
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
