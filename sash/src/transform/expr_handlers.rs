//! Handlers for expression evaluation in the transform.

use super::expressions::{arg_to_value, expr_to_string, expr_to_value};
use crate::ir::*;
use crate::ssa::*;

pub(super) fn handle_command_substitution(expr: &Expr, state: &VersionState) -> SsaValue {
    let Some((stmt, name_opt, c)) = single_command_subst(expr) else {
        // Multi-statement / pipeline body (`$(x=1; echo $x)`, `$(a | b)`):
        // carry the body as a `CmdSubstBody` to be lifted into a capture temp
        // by `lift_int_func_calls` (like `IntFuncCall`). Was a silent `$(...)`
        // placeholder at runtime.
        if let ExprKind::CommandSubstitution(stmts) = &expr.kind {
            let body: Vec<Statement> = stmts.to_vec();
            return SsaValue::CmdSubstBody {
                body,
                redirects: vec![],
            };
        }
        return SsaValue::Expr(expr_to_string(expr));
    };
    if let Some(name) = name_opt {
        if let Some(v) = resolve_single_cmdsubst(name, c, state) {
            return v;
        }
    }
    let argv = build_argv(c, state);
    let redirects = match &stmt.kind {
        StmtKind::Redirected(r) => r.redirects.clone(),
        _ => vec![],
    };
    // DD#55: `$(- cmd)` suppresses the captured command's exit check (no abort
    // on failure), mirroring statement-position `- cmd`. `single_command_subst`
    // peeled the `Suppressed` wrapper to reach the command, so detect it here.
    let suppressed = subst_is_suppressed(&stmt.kind);
    SsaValue::CmdSubst {
        argv,
        redirects,
        suppressed,
    }
}

/// True when a `$(…)` single-command body carries a `- ` suppression prefix
/// (`$(- cmd)`, `$(- cmd >f)`) — peels `Redirected` wrappers to find it.
fn subst_is_suppressed(kind: &StmtKind) -> bool {
    match kind {
        StmtKind::Redirected(r) => subst_is_suppressed(&r.body.kind),
        StmtKind::Suppressed(_) => true,
        _ => false,
    }
}

/// Resolve a single-command `$(name …)`: a **defined-so-far** function → capture
/// (DD#81 execution-order gate); a native intrinsic → its value; otherwise
/// record the external use (for the SB5041 collision) and return `None` so the
/// caller emits a plain external `CmdSubst`.
fn resolve_single_cmdsubst(name: String, c: &Command, state: &VersionState) -> Option<SsaValue> {
    if state.function_bodies.contains_key(name.as_str()) {
        return Some(func_call_capture(name, c, state));
    }
    if let Some(v) = try_native_intrinsic(&name, c, state) {
        return Some(v);
    }
    // Backstop (fork B): a fixed-arity value-intrinsic that didn't lower is a
    // malformed (wrong-arity) call. It must NOT fall through to a bare external
    // command — that reaches `sash_exec` and aborts at runtime ("bare command
    // passed to sash_exec"). Yield an empty capture instead; validate (SB4045)
    // reports the arity error under normal compilation, and this keeps the SSA
    // well-formed even when validation is bypassed.
    if let Some(arity) = crate::ssa::value_intrinsic_arity(&name) {
        if c.args.len() != arity {
            return Some(SsaValue::Literal(String::new()));
        }
    }
    if !state.lowering_fn_template {
        state.externally_used.borrow_mut().insert(name);
    }
    None
}

/// `$(f …)` on a known user function. Option C: an `-> int` function
/// *returns* its value, captured as an `IntFuncCall` (identical to `$((f))`,
/// lifted into a temp downstream); any other function's result is its stdout
/// (`FuncCall`).
fn func_call_capture(name: String, c: &Command, state: &VersionState) -> SsaValue {
    let args = c.args.iter().map(|a| arg_to_value(a, state)).collect();
    if state.function_int_typed.contains(name.as_str()) {
        SsaValue::IntFuncCall { name, args }
    } else {
        SsaValue::FuncCall { name, args }
    }
}

/// Extract `$(<single-command>)` shape: returns the original outer
/// `Statement` (needed to recover any redirects), the head name if
/// it's a bare literal, and the inner `Command`. Returns `None` for
/// anything that isn't a single-command substitution after stripping
/// `Comment` filler and `Redirected` / `Suppressed` wrappers.
fn single_command_subst(expr: &Expr) -> Option<(&Statement, Option<String>, &Command)> {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return None;
    };
    let real_stmts: Vec<&Statement> = stmts.iter().collect();
    if real_stmts.len() != 1 {
        return None;
    }
    let stmt = real_stmts[0];
    let inner = unwrap_subst_wrappers(&stmt.kind);
    let StmtKind::Command(c) = inner else {
        return None;
    };
    let name = match &c.name.kind {
        ExprKind::Literal(n) => Some(n.clone()),
        _ => None,
    };
    Some((stmt, name, c))
}

/// Peel any number of `Redirected` / `Suppressed` wrappers off a
/// `StmtKind`, returning a reference to the innermost kind. Used by
/// `single_command_subst` to look through `$(- foo)` etc.
fn unwrap_subst_wrappers(mut s: &StmtKind) -> &StmtKind {
    loop {
        match s {
            StmtKind::Redirected(r) => s = &r.body.kind,
            StmtKind::Suppressed(inner) => s = &inner.kind,
            other => return other,
        }
    }
}

/// Native-builtin shortcut for `$(name args…)` — the DD#41 collection
/// intrinsics, the DD#65 string sugar (delegated to
/// `try_string_intrinsic`), and echo (DD#14). Returns `None` for any other
/// builtin (or unknown name), letting the caller fall through to the regular
/// `CmdSubst` argv path.
fn try_native_intrinsic(name: &str, c: &Command, state: &VersionState) -> Option<SsaValue> {
    match BuiltinCmd::from_name(name) {
        BuiltinCmd::Len => try_len_intrinsic(c, state),
        BuiltinCmd::Strlen => try_strlen_intrinsic(c, state),
        // `keys` is not a scalar value intrinsic: a value-context `$(keys m)` is
        // rejected SB5025 in validate, and `for … in $(keys m)` uses the splat
        // path — so it falls through here (was a dead `try_keys_intrinsic`).
        BuiltinCmd::Join => Some(join_intrinsic(c, state)),
        BuiltinCmd::Echo => Some(echo_concat(c, state)),
        other => try_string_intrinsic(other, c, state),
    }
}

/// DD#65 string sugar: the intrinsics that lower to a `RegexReplace`,
/// `RegexMatch` or `CaseConvert` value rather than touching a collection.
/// Split from `try_native_intrinsic` because the two families share nothing
/// but the dispatch.
fn try_string_intrinsic(cmd: BuiltinCmd, c: &Command, state: &VersionState) -> Option<SsaValue> {
    match cmd {
        BuiltinCmd::Sub => regex_replace_intrinsic(c, state, false),
        BuiltinCmd::Gsub => regex_replace_intrinsic(c, state, true),
        BuiltinCmd::Match => regex_match_intrinsic(c, state),
        BuiltinCmd::Toupper => case_convert_intrinsic(c, state, true),
        BuiltinCmd::Tolower => case_convert_intrinsic(c, state, false),
        BuiltinCmd::Basename => path_sugar_intrinsic(c, state, ".*/"),
        BuiltinCmd::Dirname => path_sugar_intrinsic(c, state, "/[^/]*$"),
        _ => None,
    }
}

/// DD#65: `$(basename p)` / `$(dirname p)` are **sugar** for a fixed-pattern
/// `sub` — `basename` strips everything up to the last `/` (`.*/` → ``),
/// `dirname` strips the trailing `/component` (`/[^/]*$` → ``). Reuses the
/// `RegexReplace` path (no fork, no taint beyond the operand). `None` on
/// wrong arity (falls through to the generic cmdsub path).
fn path_sugar_intrinsic(c: &Command, state: &VersionState, pattern: &str) -> Option<SsaValue> {
    if c.args.len() != 1 {
        return None;
    }
    Some(SsaValue::RegexReplace {
        subject: Box::new(expr_to_value(&c.args[0], state)),
        pattern: Box::new(SsaValue::Literal(pattern.to_string())),
        replacement: Box::new(SsaValue::Literal(String::new())),
        global: false,
    })
}

/// DD#65: `$(toupper s)` / `$(tolower s)` → `SsaValue::CaseConvert`. The
/// single operand is an ordinary lowered value (quoting/interpolation are
/// normal argument evaluation). `None` (falls through to the generic cmdsub
/// path) if the arity is wrong.
fn case_convert_intrinsic(c: &Command, state: &VersionState, upper: bool) -> Option<SsaValue> {
    if c.args.len() != 1 {
        return None;
    }
    Some(SsaValue::CaseConvert {
        subject: Box::new(expr_to_value(&c.args[0], state)),
        upper,
    })
}

/// DD#63: `$(sub S P R)` / `$(gsub S P R)` → `SsaValue::RegexReplace`.
/// The three operands are ordinary lowered values, so quoting and
/// interpolation are just normal argument evaluation. Returns `None`
/// (falls through to the generic cmdsub path) if the arity is wrong.
fn regex_replace_intrinsic(c: &Command, state: &VersionState, global: bool) -> Option<SsaValue> {
    if c.args.len() != 3 {
        return None;
    }
    Some(SsaValue::RegexReplace {
        subject: Box::new(expr_to_value(&c.args[0], state)),
        pattern: Box::new(expr_to_value(&c.args[1], state)),
        replacement: Box::new(expr_to_value(&c.args[2], state)),
        global,
    })
}

/// DD#63: `$(match S P)` → `SsaValue::RegexMatch` (whole-match capture).
/// The two operands are ordinary lowered values. Returns `None` (falls
/// through to the generic cmdsub path) if the arity is wrong.
fn regex_match_intrinsic(c: &Command, state: &VersionState) -> Option<SsaValue> {
    if c.args.len() != 2 {
        return None;
    }
    Some(SsaValue::RegexMatch {
        subject: Box::new(expr_to_value(&c.args[0], state)),
        pattern: Box::new(expr_to_value(&c.args[1], state)),
    })
}

/// Build the runtime argv: head followed by every argument lowered
/// to an `SsaValue` (preserves the `expr_to_value` shape that
/// `CmdSubst` expects).
fn build_argv(c: &Command, state: &VersionState) -> Vec<SsaValue> {
    let mut argv = Vec::with_capacity(c.args.len() + 1);
    // DD#23: resolve the command head to its absolute path, like direct
    // commands and pipeline stages. Otherwise a bare external name (cat,
    // printf, …) reaches sash_exec unresolved ("bare command passed to
    // sash_exec"). echo/true/false/: stay bare — the cmdsub codegen
    // inlines them.
    let head = match &c.name.kind {
        ExprKind::Literal(name) => {
            SsaValue::Literal(super::resolution::resolve_pipeline_command(name, state))
        }
        _ => expr_to_value(&c.name, state),
    };
    argv.push(head);
    for arg in &c.args {
        argv.push(arg_to_value(arg, state));
    }
    argv
}

/// `$(strlen X)` — byte length of an ordinary value operand. Amends DD#75 §2
/// (`len` dispatched on the operand's type, so *bytes* and *elements* shared
/// one spelling) and DD#102 (the operand is a value, not a name).
///
/// The operand is lowered by `expr_to_value` exactly as `toupper`'s is, so
/// quoting and interpolation are normal argument evaluation: `strlen s` is
/// the length of the string `"s"`, `strlen $s` the length of `s`'s value, and
/// `strlen ${a[0]}` the length of that cell. `None` on wrong arity, which
/// falls through to the generic cmdsub path.
fn try_strlen_intrinsic(c: &Command, state: &VersionState) -> Option<SsaValue> {
    if c.args.len() != 1 {
        return None;
    }
    Some(SsaValue::StrLen {
        subject: Box::new(expr_to_value(&c.args[0], state)),
    })
}

fn try_len_intrinsic(c: &Command, state: &VersionState) -> Option<SsaValue> {
    // DD#75 §2: dispatch on the operand's TYPE, not the `$` sigil. A
    // collection (array/map) → element/entry count; anything else (a string
    // scalar) → byte length. This retires the old `len a` vs `len $a`
    // syntactic split that made `len <string>` read a non-table (garbage).
    let arg = c.args.first()?;
    let name = match &arg.kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            pe.name.clone()
        }
        _ => {
            let s = expr_to_string(arg);
            s.strip_prefix('$').unwrap_or(&s).to_string()
        }
    };
    // DD#75 §5: a user-written `args` names the implicit per-function array.
    let name = super::expressions::resolve_args_name(&name, state);
    let v = state.lookup(&name);
    let operator = if state.collections.contains(&name) {
        "arrlen"
    } else {
        "strlen"
    };
    Some(SsaValue::ParamExpansion {
        var: v,
        operator: operator.to_string(),
        operand: String::new(),
    })
}

fn join_intrinsic(c: &Command, state: &VersionState) -> SsaValue {
    // DD#75: `join arr` → space-separated; `join arr sep` → sep-separated.
    // Array first, separator second (least-surprise, collection-first —
    // consistent with `len`/`keys`).
    let (arr_expr, sep) = if c.args.len() >= 2 {
        (&c.args[0], strip_word_quotes(&c.args[1]))
    } else if c.args.len() == 1 {
        (&c.args[0], " ".to_string())
    } else {
        // INVARIANT: validate::check_join_arity (SB5040) rejects a 0-arg (and
        // >2-arg) `$(join …)` before transform, so this is unreachable.
        unreachable!("join arity guaranteed by validate::check_join_arity");
    };
    // The array arg should be @arr → ParameterExpansion(arrvals)
    let arr_name = match &arr_expr.kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            pe.name.clone()
        }
        _ => expr_to_string(arr_expr),
    };
    let arr_name = super::expressions::resolve_args_name(&arr_name, state);
    let v = state.lookup(&arr_name);
    SsaValue::ParamExpansion {
        var: v,
        operator: "arrjoin".to_string(),
        operand: sep,
    }
}

/// Strip a single layer of quotes from a word, yielding its literal VALUE
/// (not its source form): a bare/single/double-quoted literal yields its
/// text, an empty `''`/`""` yields the empty string; anything richer falls
/// back to `expr_to_string`. Used for `read`/`echo` separator/flag words so
/// `read -d ''` sees an empty value (→ NUL), not the quote characters.
pub(super) fn strip_word_quotes(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => s.clone(),
        ExprKind::DoubleQuoted(parts) => match parts.as_slice() {
            [] => String::new(),
            [p] => match &p.kind {
                ExprKind::Literal(s) => s.clone(),
                _ => expr_to_string(expr),
            },
            _ => expr_to_string(expr),
        },
        _ => expr_to_string(expr),
    }
}

fn echo_concat(c: &Command, state: &VersionState) -> SsaValue {
    if c.args.is_empty() {
        return SsaValue::Literal(String::new());
    }
    let mut parts: Vec<SsaValue> = Vec::new();
    for (i, arg) in c.args.iter().enumerate() {
        if i > 0 {
            parts.push(SsaValue::Literal(" ".to_string()));
        }
        parts.push(expr_to_value(arg, state));
    }
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        SsaValue::Concat(parts)
    }
}
pub(super) fn handle_structured_arithmetic(
    arith: &ArithExpr,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    arith_to_ssa(arith, expr, state)
}

#[allow(clippy::only_used_in_recursion)]
fn arith_to_ssa(arith: &ArithExpr, expr: &Expr, state: &VersionState) -> SsaValue {
    if let Some(v) = arith_leaf_to_ssa(arith, state) {
        return v;
    }
    match arith {
        ArithExpr::BinOp { left, op, right } => binop_to_ssa(left, *op, right, expr, state),
        ArithExpr::UnaryOp { op, operand } => unaryop_to_ssa(*op, operand, expr, state),
        // A mutation (`x = …`, `x += …`, `x++`/`x--`, `a[i] = …`) never reaches
        // the value path: in a `$(( … ))` value it is rejected by validate
        // (SB4020, `check_arith_value_mutation`); in a `(( … ))` statement or
        // condition it is either a top-level statement-mutation
        // (`lower_arith_mutation_structured`) or hoisted into a preceding store
        // by `lower_arith_test_stmt`, which leaves a mutation-free residual
        // here. (Bug #3 fork A — this closed the half-wired value path that
        // used to silently drop the store.)
        ArithExpr::PreIncDec { .. }
        | ArithExpr::PostIncDec { .. }
        | ArithExpr::Assign { .. }
        | ArithExpr::CompoundAssign { .. }
        | ArithExpr::SubscriptAssign { .. } => unreachable!(
            "arith mutation in a value context is rejected by validate (SB4020) \
             or hoisted before reaching the value path"
        ),
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => SsaValue::Ternary {
            cond: Box::new(arith_to_ssa(cond, expr, state)),
            true_val: Box::new(arith_to_ssa(true_val, expr, state)),
            false_val: Box::new(arith_to_ssa(false_val, expr, state)),
        },
        ArithExpr::Comma(exprs) => comma_to_ssa(exprs, expr, state),
        ArithExpr::FuncCall { name, args } => funccall_to_ssa(name, args, expr, state),
        ArithExpr::Paren(inner) => arith_to_ssa(inner, expr, state),
        // arith-subscript model B (Stage 2): a bare int-array cell read. The
        // index is a full arith expr → lowered to an int; the read uses it as
        // the integer key (lower_subscript_raw's is_integer_key path calls
        // lower_value_as_int on the index).
        ArithExpr::Subscript { name, index } => subscript_read_to_ssa(name, index, expr, state),
        // Value leaves are handled by `arith_leaf_to_ssa` above.
        _ => unreachable!("arith value leaf not handled by arith_leaf_to_ssa"),
    }
}

/// arith-subscript model B: lower a bare `a[i]` cell read to `SsaValue::
/// Subscript`. The index is the arith-int value; `is_integer_key`/`_value`
/// follow the array's declaration (a map is rejected earlier by type_check).
fn subscript_read_to_ssa(
    name: &str,
    index: &ArithExpr,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    // Resolve the implicit positional array `args` → the per-inline `__argsN`
    // slot, exactly as the word cell-read path does (`subscript_to_value`).
    // Without this, `$(( args[i] ))` reached codegen with the unresolved name
    // and tripped the defensive `find_array_slot` ICE
    // (arith-args-subscript-codegen); the SB5031 false-positive had masked it.
    let resolved = super::expressions::resolve_args_name(name, state);
    SsaValue::Subscript {
        index: Box::new(arith_to_ssa(index, expr, state)),
        is_integer_key: !state.assoc_arrays.contains(&resolved),
        is_integer_value: state.type_map.get(&resolved).copied() == Some(crate::ir::VarType::Int),
        array: resolved,
    }
}

/// The value-producing leaves of an `ArithExpr`: a literal, a bare/`$`-var, or a
/// `${…}` normal-expansion import (`${a[i]}`/`${m[k]}` → the cell via the shared
/// `expr_to_value`). Returns `None` for the operator/mutation/control forms,
/// which the caller's match handles.
fn arith_leaf_to_ssa(arith: &ArithExpr, state: &VersionState) -> Option<SsaValue> {
    match arith {
        ArithExpr::Literal(n) => Some(SsaValue::Expr(n.to_string())),
        // DD#75 §5: `$1`/`$#` in arithmetic mean the same `args` reads as
        // outside `$(( ))` — route positionals through the shared helper first.
        ArithExpr::Var(name) => Some(
            super::expressions::positional_to_value(name, state)
                .unwrap_or_else(|| var_to_ssa(name, state)),
        ),
        ArithExpr::DollarVar(name) => Some(
            super::expressions::positional_to_value(name, state)
                .unwrap_or_else(|| SsaValue::Var(state.lookup(name))),
        ),
        ArithExpr::Expansion(inner) => {
            Some(canonicalize_arith_default(expr_to_value(inner, state)))
        }
        _ => None,
    }
}

/// DD#72: in arith context, a `${s:-LITERAL}` default is used as an int operand.
/// The default is lowered by a decimal conversion at runtime, so rewrite a
/// non-decimal DD#72 literal default (`0xff`, `0o17`, `0b101`) to its decimal
/// form here — the type-check D4 pass accepts exactly these literals. Leaves a
/// non-literal default untouched (a plain decimal is already correct; a
/// non-literal was rejected by D4). Arith-scoped, so a *string*-context
/// `${s:-0xff}` keeps its literal "0xff" (that path never calls this).
fn canonicalize_arith_default(v: SsaValue) -> SsaValue {
    let SsaValue::DefaultValue { source, default } = v else {
        return v;
    };
    let default = match *default {
        SsaValue::Literal(ref s) => match crate::parse::parse_dd72_literal(s) {
            Some(n) => Box::new(SsaValue::Literal(n.to_string())),
            None => default,
        },
        _ => default,
    };
    SsaValue::DefaultValue { source, default }
}

/// In arithmetic context, a bare name is either a known int-typed
/// function (DD#58 nullary call) or a normal variable reference.
fn var_to_ssa(name: &str, state: &VersionState) -> SsaValue {
    if state.function_int_typed.contains(name) {
        return SsaValue::IntFuncCall {
            name: name.to_string(),
            args: vec![],
        };
    }
    SsaValue::Var(state.lookup(name))
}

fn binop_to_ssa(
    left: &ArithExpr,
    op: IrArithOp,
    right: &ArithExpr,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    let l = arith_to_ssa(left, expr, state);
    let r = arith_to_ssa(right, expr, state);
    let ssa_op = ir_op_to_ssa(op);
    if let Some(folded) = try_fold_binop(&l, &r, ssa_op) {
        return folded;
    }
    SsaValue::ArithBinOp {
        left: Box::new(l),
        op: ssa_op,
        right: Box::new(r),
    }
}

/// Constant-fold `int op int` when both sides are `Expr` literals.
fn try_fold_binop(l: &SsaValue, r: &SsaValue, op: ArithOp) -> Option<SsaValue> {
    let (SsaValue::Expr(ls), SsaValue::Expr(rs)) = (l, r) else {
        return None;
    };
    let lv = ls.parse::<i64>().ok()?;
    let rv = rs.parse::<i64>().ok()?;
    Some(SsaValue::Expr(fold_arith_op(lv, rv, op)?.to_string()))
}

fn unaryop_to_ssa(
    op: IrUnaryArithOp,
    operand: &ArithExpr,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    let inner = arith_to_ssa(operand, expr, state);
    if let Some(folded) = try_fold_unop(&inner, op) {
        return folded;
    }
    match op {
        // `-x` → `0 - x`
        IrUnaryArithOp::Neg => SsaValue::ArithBinOp {
            left: Box::new(SsaValue::Expr("0".to_string())),
            op: ArithOp::Sub,
            right: Box::new(inner),
        },
        IrUnaryArithOp::Pos => inner,
        // `~x` → `x ^ -1`
        IrUnaryArithOp::BitNot => SsaValue::ArithBinOp {
            left: Box::new(inner),
            op: ArithOp::BitXor,
            right: Box::new(SsaValue::Expr((-1i64).to_string())),
        },
        // `!x` → `x == 0`
        IrUnaryArithOp::Not => SsaValue::ArithBinOp {
            left: Box::new(inner),
            op: ArithOp::Eq,
            right: Box::new(SsaValue::Expr("0".to_string())),
        },
    }
}

/// Constant-fold a unary arith op when its operand is an `Expr` literal.
fn try_fold_unop(inner: &SsaValue, op: IrUnaryArithOp) -> Option<SsaValue> {
    let SsaValue::Expr(s) = inner else {
        return None;
    };
    let v = s.parse::<i64>().ok()?;
    let folded = match op {
        IrUnaryArithOp::Neg => (-v).to_string(),
        IrUnaryArithOp::Pos => v.to_string(),
        IrUnaryArithOp::BitNot => (!v).to_string(),
        IrUnaryArithOp::Not => (if v == 0 { 1 } else { 0 }).to_string(),
    };
    Some(SsaValue::Expr(folded))
}

fn comma_to_ssa(exprs: &[ArithExpr], expr: &Expr, state: &VersionState) -> SsaValue {
    match exprs.last() {
        Some(last) => arith_to_ssa(last, expr, state),
        None => SsaValue::Expr("0".to_string()),
    }
}

fn funccall_to_ssa(name: &str, args: &[ArithExpr], expr: &Expr, state: &VersionState) -> SsaValue {
    if let Some(v) = try_arith_int_intrinsic(name, args, state) {
        return v;
    }
    if args.is_empty() {
        if let Some(v) = resolve_nullary_sexpr(name, state) {
            return v;
        }
    }
    let ssa_args: Vec<SsaValue> = args.iter().map(|a| arith_to_ssa(a, expr, state)).collect();
    SsaValue::IntFuncCall {
        name: name.to_string(),
        args: ssa_args,
    }
}

/// DD#58/#75: an int-returning intrinsic with args (e.g. `len`) called as an
/// arith sexpr `(len a)` must EVALUATE the intrinsic — not flow through the
/// user-int-func path (`inline_int_func`), which finds no user body and returns
/// 0. Reconstruct a `Command` from the sexpr args and reuse the normal builder.
/// Only int-returning intrinsics are intercepted; a string intrinsic stays an
/// IntFuncCall (handled elsewhere). `None` → not this case.
fn try_arith_int_intrinsic(
    name: &str,
    args: &[ArithExpr],
    state: &VersionState,
) -> Option<SsaValue> {
    if !is_arith_int_intrinsic(name) {
        return None;
    }
    let cmd = reconstruct_intrinsic_command(name, args)?;
    try_native_intrinsic(name, &cmd, state)
}

/// Resolve a nullary sexpr atom `(name)`. Returns `None` for a known user
/// `-> int` function (the caller inlines it via `IntFuncCall`).
///
/// DD#58/Option C: a NULLARY int intrinsic — `(getpid)`, `(argc)`, … — must
/// EVALUATE (it was silently 0 via the empty-int-func path); an int producer
/// works in both `$(f)` capture and the `$(( (f) ))` sexpr, and intrinsics
/// must not differ from `len`/user int fns. `getpid`/`status` reuse the
/// `$$`/`$?` specials (native + c lower them directly); the rest reuse the
/// word-form `CmdSubst` node (codegen computes the i64: native direct via
/// `int_intrinsic_i64`, c via its atoi fallback). `getcpid` (`$!`) is excluded
/// — filed. A string intrinsic here is category B (rejected in type_check);
/// see bug-arith-sexpr-intrinsics-zero.
fn resolve_nullary_sexpr(name: &str, state: &VersionState) -> Option<SsaValue> {
    match name {
        "getpid" => Some(SsaValue::Var(state.lookup("$"))),
        "status" => Some(SsaValue::Var(state.lookup("?"))),
        "getppid" | "argc" | "random" | "signum" | "epochseconds" => Some(SsaValue::CmdSubst {
            argv: vec![SsaValue::Literal(name.to_string())],
            redirects: vec![],
            suppressed: false,
        }),
        // `(x)` on a plain variable is arithmetic GROUPING, not a call: the
        // arith parser emits `FuncCall` for every `(name)` and defers the
        // decision here (DD#58). A nullary name that is neither an intrinsic
        // nor a known function is a variable reference — return its value.
        // (Was silently 0 via the empty-int-func path: `$(( (x) ))` → 0.)
        _ if !state.known_functions.contains(name) => Some(SsaValue::Var(state.lookup(name))),
        // A known user `-> int` function → None: the caller emits IntFuncCall.
        _ => None,
    }
}

/// Int-returning intrinsics that evaluate as an arith sexpr operand. `len`
/// (element/entry count) and `strlen` (byte length) are the ones in
/// `try_native_intrinsic`; the string intrinsics (join/keys/toupper/…) are
/// not int and are excluded.
fn is_arith_int_intrinsic(name: &str) -> bool {
    name == "len" || name == "strlen"
}

/// Rebuild a `Command` for an int intrinsic from its arith sexpr args, so the
/// normal intrinsic builder can consume it. Only simple name/number operands
/// are supported (`len a`, `len $a`); a complex operand returns `None` (the
/// caller falls back to the IntFuncCall path).
///
/// `Var` and `DollarVar` rebuild to *different* expressions. Both spellings
/// name the same variable to `len`, so collapsing them to one `Literal` was
/// lossless while every int intrinsic resolved a name — but DD#102 gave
/// `strlen` a value operand, where `s` is a one-character string and `$s` is
/// the variable's contents. Flattening made `$(( (strlen $s) ))` measure the
/// name.
fn reconstruct_intrinsic_command(name: &str, args: &[ArithExpr]) -> Option<Command> {
    let mut cmd_args = Vec::with_capacity(args.len());
    for a in args {
        let kind = match a {
            ArithExpr::Var(n) => ExprKind::Literal(n.clone()),
            ArithExpr::DollarVar(n) => ExprKind::SimpleExpansion(n.clone()),
            ArithExpr::Literal(n) => ExprKind::Literal(n.to_string()),
            _ => return None,
        };
        cmd_args.push(Expr {
            kind,
            span: Span::new(0, 0),
        });
    }
    Some(Command {
        name: Expr {
            kind: ExprKind::Literal(name.to_string()),
            span: Span::new(0, 0),
        },
        args: cmd_args,
    })
}

/// Convert IR arithmetic op to SSA arithmetic op.
fn ir_op_to_ssa(op: IrArithOp) -> ArithOp {
    match op {
        IrArithOp::Add => ArithOp::Add,
        IrArithOp::Sub => ArithOp::Sub,
        IrArithOp::Mul => ArithOp::Mul,
        IrArithOp::Div => ArithOp::Div,
        IrArithOp::Mod => ArithOp::Mod,
        IrArithOp::Pow => ArithOp::Pow,
        IrArithOp::Eq => ArithOp::Eq,
        IrArithOp::Ne => ArithOp::Ne,
        IrArithOp::Lt => ArithOp::Lt,
        IrArithOp::Le => ArithOp::Le,
        IrArithOp::Gt => ArithOp::Gt,
        IrArithOp::Ge => ArithOp::Ge,
        IrArithOp::And => ArithOp::And,
        IrArithOp::Or => ArithOp::Or,
        IrArithOp::Shl => ArithOp::Shl,
        IrArithOp::Shr => ArithOp::Shr,
        IrArithOp::BitAnd => ArithOp::BitAnd,
        IrArithOp::BitOr => ArithOp::BitOr,
        IrArithOp::BitXor => ArithOp::BitXor,
    }
}

/// Constant-fold `int op int`. Returns `None` when the result is not a
/// safe compile-time constant — a +,-,* overflow, or an out-of-range
/// shift/pow — so the caller falls back to a runtime `ArithBinOp`, where
/// the DD#25 overflow trap (for +,-,*) applies. This keeps a
/// compile-time-known overflow from silently wrapping
/// (bug-runtime-arith-overflow-wraps) without introducing a new
/// compile-time error.
fn fold_arith_op(l: i64, r: i64, op: ArithOp) -> Option<i64> {
    match op {
        ArithOp::Add => l.checked_add(r),
        ArithOp::Sub => l.checked_sub(r),
        ArithOp::Mul => l.checked_mul(r),
        ArithOp::Div => {
            if r == 0 {
                Some(0)
            } else {
                l.checked_div(r)
            }
        }
        ArithOp::Mod => {
            if r == 0 {
                Some(0)
            } else {
                l.checked_rem(r)
            }
        }
        ArithOp::Eq => Some((l == r) as i64),
        ArithOp::Ne => Some((l != r) as i64),
        ArithOp::Lt => Some((l < r) as i64),
        ArithOp::Le => Some((l <= r) as i64),
        ArithOp::Gt => Some((l > r) as i64),
        ArithOp::Ge => Some((l >= r) as i64),
        ArithOp::And => Some(((l != 0) && (r != 0)) as i64),
        ArithOp::Or => Some(((l != 0) || (r != 0)) as i64),
        ArithOp::Shl => (0..64).contains(&r).then(|| l << r),
        ArithOp::Shr => (0..64).contains(&r).then(|| l >> r),
        ArithOp::BitAnd => Some(l & r),
        ArithOp::BitOr => Some(l | r),
        ArithOp::BitXor => Some(l ^ r),
        ArithOp::Pow => u32::try_from(r).ok().and_then(|e| l.checked_pow(e)),
    }
}
