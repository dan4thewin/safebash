//! Handlers for expression evaluation in the transform.

use super::expressions::{expr_to_string, expr_to_value};
use crate::ir::*;
use crate::ssa::*;

pub(super) fn handle_command_substitution(expr: &Expr, state: &VersionState) -> SsaValue {
    let Some((stmt, name_opt, c)) = single_command_subst(expr) else {
        return SsaValue::Expr(expr_to_string(expr));
    };
    if let Some(name) = name_opt {
        if state.known_functions.contains(name.as_str()) {
            return SsaValue::FuncCall(name);
        }
        if let Some(v) = try_native_intrinsic(&name, c, state) {
            return v;
        }
    }
    let argv = build_argv(c, state);
    let redirects = match &stmt.kind {
        StmtKind::Redirected(r) => r.redirects.clone(),
        _ => vec![],
    };
    SsaValue::CmdSubst { argv, redirects }
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
    let real_stmts: Vec<&Statement> = stmts
        .iter()
        .filter(|s| !matches!(s.kind, StmtKind::Comment(_)))
        .collect();
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

/// Native-builtin shortcut for `$(name args…)` — len / keys / join
/// (DD#41) and echo (DD#14). Returns `None` for any other builtin
/// (or unknown name), letting the caller fall through to the regular
/// `CmdSubst` argv path.
fn try_native_intrinsic(name: &str, c: &Command, state: &VersionState) -> Option<SsaValue> {
    match BuiltinCmd::from_name(name) {
        BuiltinCmd::Len => try_len_intrinsic(c, state),
        BuiltinCmd::Keys => try_keys_intrinsic(c, state),
        BuiltinCmd::Join => Some(join_intrinsic(c, state)),
        BuiltinCmd::Echo => Some(echo_concat(c, state)),
        _ => None,
    }
}

/// Build the runtime argv: head followed by every argument lowered
/// to an `SsaValue` (preserves the `expr_to_value` shape that
/// `CmdSubst` expects).
fn build_argv(c: &Command, state: &VersionState) -> Vec<SsaValue> {
    let mut argv = Vec::with_capacity(c.args.len() + 1);
    argv.push(expr_to_value(&c.name, state));
    for arg in &c.args {
        argv.push(expr_to_value(arg, state));
    }
    argv
}

fn try_len_intrinsic(c: &Command, state: &VersionState) -> Option<SsaValue> {
    let arg = c.args.first()?;
    let arg_str = expr_to_string(arg);
    if let Some(var_name) = arg_str.strip_prefix('$') {
        // len $var → string length
        let v = state.lookup(var_name);
        Some(SsaValue::ParamExpansion {
            var: v,
            operator: "strlen".to_string(),
            operand: String::new(),
        })
    } else {
        // len arr → array length
        let v = state.lookup(&arg_str);
        Some(SsaValue::ParamExpansion {
            var: v,
            operator: "arrlen".to_string(),
            operand: String::new(),
        })
    }
}

fn try_keys_intrinsic(c: &Command, state: &VersionState) -> Option<SsaValue> {
    let arg = c.args.first()?;
    let arg_str = expr_to_string(arg);
    let v = state.lookup(&arg_str);
    Some(SsaValue::ParamExpansion {
        var: v,
        operator: "arrkeys".to_string(),
        operand: String::new(),
    })
}

fn join_intrinsic(c: &Command, state: &VersionState) -> SsaValue {
    // join @arr → space-separated
    // join sep @arr → sep-separated
    let (sep, arr_expr) = if c.args.len() >= 2 {
        // Strip quotes from separator
        let sep_raw = match &c.args[0].kind {
            ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => s.clone(),
            ExprKind::DoubleQuoted(parts) if parts.len() == 1 => match &parts[0].kind {
                ExprKind::Literal(s) => s.clone(),
                _ => expr_to_string(&c.args[0]),
            },
            _ => expr_to_string(&c.args[0]),
        };
        (sep_raw, &c.args[1])
    } else if c.args.len() == 1 {
        (" ".to_string(), &c.args[0])
    } else {
        panic!("ICE: join requires at least one argument");
    };
    // The array arg should be @arr → ParameterExpansion(arrvals)
    let arr_name = match &arr_expr.kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            pe.name.clone()
        }
        _ => expr_to_string(arr_expr),
    };
    let v = state.lookup(&arr_name);
    SsaValue::ParamExpansion {
        var: v,
        operator: "arrjoin".to_string(),
        operand: sep,
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
    match arith {
        ArithExpr::Literal(n) => SsaValue::Expr(n.to_string()),
        ArithExpr::Var(name) => var_to_ssa(name, state),
        ArithExpr::DollarVar(name) => SsaValue::Var(state.lookup(name)),
        ArithExpr::BinOp { left, op, right } => binop_to_ssa(left, *op, right, expr, state),
        ArithExpr::UnaryOp { op, operand } => unaryop_to_ssa(*op, operand, expr, state),
        ArithExpr::PreIncDec { var, inc } | ArithExpr::PostIncDec { var, inc } => {
            incdec_to_ssa(var, *inc, state)
        }
        ArithExpr::Assign { value, .. } => arith_to_ssa(value, expr, state),
        ArithExpr::CompoundAssign { var, op, value } => {
            compound_assign_to_ssa(var, *op, value, expr, state)
        }
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
    }
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
    Some(SsaValue::Expr(fold_arith_op(lv, rv, op).to_string()))
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

fn incdec_to_ssa(var: &str, inc: bool, state: &VersionState) -> SsaValue {
    let op = if inc { ArithOp::Add } else { ArithOp::Sub };
    SsaValue::ArithBinOp {
        left: Box::new(SsaValue::Var(state.lookup(var))),
        op,
        right: Box::new(SsaValue::Expr("1".to_string())),
    }
}

fn compound_assign_to_ssa(
    var: &str,
    op: IrArithOp,
    value: &ArithExpr,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    let rhs = arith_to_ssa(value, expr, state);
    SsaValue::ArithBinOp {
        left: Box::new(SsaValue::Var(state.lookup(var))),
        op: ir_op_to_ssa(op),
        right: Box::new(rhs),
    }
}

fn comma_to_ssa(exprs: &[ArithExpr], expr: &Expr, state: &VersionState) -> SsaValue {
    match exprs.last() {
        Some(last) => arith_to_ssa(last, expr, state),
        None => SsaValue::Expr("0".to_string()),
    }
}

fn funccall_to_ssa(name: &str, args: &[ArithExpr], expr: &Expr, state: &VersionState) -> SsaValue {
    let ssa_args: Vec<SsaValue> = args.iter().map(|a| arith_to_ssa(a, expr, state)).collect();
    SsaValue::IntFuncCall {
        name: name.to_string(),
        args: ssa_args,
    }
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

fn fold_arith_op(l: i64, r: i64, op: ArithOp) -> i64 {
    match op {
        ArithOp::Add => l.wrapping_add(r),
        ArithOp::Sub => l.wrapping_sub(r),
        ArithOp::Mul => l.wrapping_mul(r),
        ArithOp::Div => {
            if r != 0 {
                l / r
            } else {
                0
            }
        }
        ArithOp::Mod => {
            if r != 0 {
                l % r
            } else {
                0
            }
        }
        ArithOp::Eq => (l == r) as i64,
        ArithOp::Ne => (l != r) as i64,
        ArithOp::Lt => (l < r) as i64,
        ArithOp::Le => (l <= r) as i64,
        ArithOp::Gt => (l > r) as i64,
        ArithOp::Ge => (l >= r) as i64,
        ArithOp::And => ((l != 0) && (r != 0)) as i64,
        ArithOp::Or => ((l != 0) || (r != 0)) as i64,
        ArithOp::Not => (l == 0) as i64,
        ArithOp::Shl => l << r,
        ArithOp::Shr => l >> r,
        ArithOp::BitAnd => l & r,
        ArithOp::BitOr => l | r,
        ArithOp::BitXor => l ^ r,
        ArithOp::Pow => l.pow(r as u32),
    }
}
