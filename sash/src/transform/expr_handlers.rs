//! Handlers for expression evaluation in the transform.

use super::expressions::{expr_to_string, expr_to_value};
use crate::ir::*;
use crate::ssa::*;

pub(super) fn handle_command_substitution(expr: &Expr, state: &VersionState) -> SsaValue {
    // Handle CommandSubstitution → FuncCall if it's a known function,
    // or CmdSubst for runtime capture
    if let ExprKind::CommandSubstitution(stmts) = &expr.kind {
        // Filter out comments — they're not real statements
        let real_stmts: Vec<&Statement> = stmts
            .iter()
            .filter(|s| !matches!(s.kind, StmtKind::Comment(_)))
            .collect();
        if real_stmts.len() == 1 {
            // Unwrap Redirected or Suppressed to get the inner Command
            let (inner, _is_suppressed) = {
                let mut s = &real_stmts[0].kind;
                let mut suppressed = false;
                loop {
                    match s {
                        StmtKind::Redirected(r) => {
                            s = &r.body.kind;
                        }
                        StmtKind::Suppressed(inner) => {
                            s = &inner.kind;
                            suppressed = true;
                        }
                        other => break (other, suppressed),
                    }
                }
            };
            if let StmtKind::Command(c) = inner {
                if let ExprKind::Literal(name) = &c.name.kind {
                    if state.known_functions.contains(name.as_str()) {
                        return SsaValue::FuncCall(name.clone());
                    }
                    // Sash-native builtins: len, keys, join (DD#41) and echo (DD#14)
                    match BuiltinCmd::from_name(name) {
                        BuiltinCmd::Len => {
                            if let Some(v) = try_len_intrinsic(c, state) {
                                return v;
                            }
                        }
                        BuiltinCmd::Keys => {
                            if let Some(v) = try_keys_intrinsic(c, state) {
                                return v;
                            }
                        }
                        BuiltinCmd::Join => {
                            return join_intrinsic(c, state);
                        }
                        BuiltinCmd::Echo => {
                            return echo_concat(c, state);
                        }
                        _ => {}
                    }
                }

                // Not an elidable function — emit CmdSubst with argv
                let mut argv = Vec::new();
                argv.push(expr_to_value(&c.name, state));
                for arg in &c.args {
                    argv.push(expr_to_value(arg, state));
                }
                // Capture redirects from Redirected wrapper
                let redirects = match &real_stmts[0].kind {
                    StmtKind::Redirected(r) => r.redirects.clone(),
                    _ => vec![],
                };
                return SsaValue::CmdSubst { argv, redirects };
            }
        }
    }
    SsaValue::Expr(expr_to_string(expr))
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
        ArithExpr::Var(name) => {
            // Check if it's a known int-typed function (DD#58)
            if state.function_int_typed.contains(name.as_str()) {
                return SsaValue::IntFuncCall {
                    name: name.clone(),
                    args: vec![],
                };
            }
            // Variable in arithmetic context — direct reference
            SsaValue::Var(state.lookup(name))
        }
        ArithExpr::DollarVar(name) => SsaValue::Var(state.lookup(name)),
        ArithExpr::BinOp { left, op, right } => {
            let l = arith_to_ssa(left, expr, state);
            let r = arith_to_ssa(right, expr, state);
            let ssa_op = ir_op_to_ssa(*op);
            // Constant folding
            if let (SsaValue::Expr(ref ls), SsaValue::Expr(ref rs)) = (&l, &r) {
                if let (Ok(lv), Ok(rv)) = (ls.parse::<i64>(), rs.parse::<i64>()) {
                    let result = fold_arith_op(lv, rv, ssa_op);
                    return SsaValue::Expr(result.to_string());
                }
            }
            SsaValue::ArithBinOp {
                left: Box::new(l),
                op: ssa_op,
                right: Box::new(r),
            }
        }
        ArithExpr::UnaryOp { op, operand } => {
            let inner = arith_to_ssa(operand, expr, state);
            match op {
                IrUnaryArithOp::Neg => {
                    if let SsaValue::Expr(ref s) = inner {
                        if let Ok(v) = s.parse::<i64>() {
                            return SsaValue::Expr((-v).to_string());
                        }
                    }
                    SsaValue::ArithBinOp {
                        left: Box::new(SsaValue::Expr("0".to_string())),
                        op: ArithOp::Sub,
                        right: Box::new(inner),
                    }
                }
                IrUnaryArithOp::Pos => inner,
                IrUnaryArithOp::BitNot => {
                    if let SsaValue::Expr(ref s) = inner {
                        if let Ok(v) = s.parse::<i64>() {
                            return SsaValue::Expr((!v).to_string());
                        }
                    }
                    SsaValue::ArithBinOp {
                        left: Box::new(inner),
                        op: ArithOp::BitXor,
                        right: Box::new(SsaValue::Expr((-1i64).to_string())),
                    }
                }
                IrUnaryArithOp::Not => {
                    if let SsaValue::Expr(ref s) = inner {
                        if let Ok(v) = s.parse::<i64>() {
                            return SsaValue::Expr(if v == 0 { "1" } else { "0" }.to_string());
                        }
                    }
                    // Runtime: compare == 0
                    SsaValue::ArithBinOp {
                        left: Box::new(inner),
                        op: ArithOp::Eq,
                        right: Box::new(SsaValue::Expr("0".to_string())),
                    }
                }
            }
        }
        ArithExpr::PreIncDec { var, inc } | ArithExpr::PostIncDec { var, inc } => {
            let op = if *inc { ArithOp::Add } else { ArithOp::Sub };
            SsaValue::ArithBinOp {
                left: Box::new(SsaValue::Var(state.lookup(var))),
                op,
                right: Box::new(SsaValue::Expr("1".to_string())),
            }
        }
        ArithExpr::Assign { var: _, value } => {
            // In expression context, assignment returns the value
            arith_to_ssa(value, expr, state)
        }
        ArithExpr::CompoundAssign { var, op, value } => {
            let rhs = arith_to_ssa(value, expr, state);
            SsaValue::ArithBinOp {
                left: Box::new(SsaValue::Var(state.lookup(var))),
                op: ir_op_to_ssa(*op),
                right: Box::new(rhs),
            }
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
        ArithExpr::Comma(exprs) => {
            // In expression context, comma returns the last value
            if let Some(last) = exprs.last() {
                arith_to_ssa(last, expr, state)
            } else {
                SsaValue::Expr("0".to_string())
            }
        }
        ArithExpr::FuncCall { name, args } => {
            let ssa_args: Vec<SsaValue> =
                args.iter().map(|a| arith_to_ssa(a, expr, state)).collect();
            SsaValue::IntFuncCall {
                name: name.clone(),
                args: ssa_args,
            }
        }
        ArithExpr::Paren(inner) => arith_to_ssa(inner, expr, state),
    }
}

/// Convert IR arithmetic op to SSA arithmetic op.
// cyclo-allow: 1:1 IR → SSA op enum
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

// cyclo-allow: small ArithOp dispatcher, 1-3 lines per arm
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
