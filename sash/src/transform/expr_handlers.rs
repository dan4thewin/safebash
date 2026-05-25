//! Extracted handlers for large ExprKind match arms in expr_to_value.

use super::expressions::{expr_to_string, expr_to_value};
use super::helpers::parse_arith_binop;
use super::resolution::try_parse_sexpr;
use crate::ir::*;
use crate::ssa::*;

/// Handle ExprKind::ArithmeticExpansion — $(( expr ))
pub(super) fn handle_arithmetic_expansion(s: &str, expr: &Expr, state: &VersionState) -> SsaValue {
    let trimmed = s.trim();
    // Empty expression: $(()) → 0
    if let Some(val) = try_parse_arith_literal(trimmed, expr, state) {
        return val;
    }
    // Unary plus: $((+x)) → strip plus
    if let Some(val) = try_parse_arith_unary(trimmed, expr, state) {
        return val;
    }
    // Arithmetic assignment: var = expr → assign var, return value
    if let Some(eq_pos) = trimmed.find('=') {
        // Make sure it's not ==, !=, <=, >=
        let before_eq = if eq_pos > 0 {
            trimmed.as_bytes()[eq_pos - 1]
        } else {
            0
        };
        let after_eq = if eq_pos + 1 < trimmed.len() {
            trimmed.as_bytes()[eq_pos + 1]
        } else {
            0
        };
        if after_eq != b'='
            && before_eq != b'!'
            && before_eq != b'<'
            && before_eq != b'>'
            && before_eq != b'='
        {
            let lhs = trimmed[..eq_pos].trim();
            let rhs = trimmed[eq_pos + 1..].trim();
            // Check for compound assignment: +=, -=, *=, /=, %=
            let (var_name, op, rhs_str) = if !lhs.is_empty() && lhs.ends_with('+') {
                (&lhs[..lhs.len() - 1], Some(ArithOp::Add), rhs)
            } else if !lhs.is_empty() && lhs.ends_with('-') {
                (&lhs[..lhs.len() - 1], Some(ArithOp::Sub), rhs)
            } else if !lhs.is_empty() && lhs.ends_with('*') {
                (&lhs[..lhs.len() - 1], Some(ArithOp::Mul), rhs)
            } else if !lhs.is_empty() && lhs.ends_with('/') {
                (&lhs[..lhs.len() - 1], Some(ArithOp::Div), rhs)
            } else if !lhs.is_empty() && lhs.ends_with('%') {
                (&lhs[..lhs.len() - 1], Some(ArithOp::Mod), rhs)
            } else {
                (lhs, None, rhs)
            };
            let var_name = var_name.trim();
            if var_name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !var_name.is_empty()
                && !rhs_str.is_empty()
            {
                let rhs_expr = Expr {
                    kind: ExprKind::ArithmeticExpansion(rhs_str.to_string()),
                    span: expr.span,
                };
                let rhs_val = expr_to_value(&rhs_expr, state);
                let result_val = if let Some(op) = op {
                    // Compound: var OP= rhs → var = var OP rhs
                    SsaValue::ArithBinOp {
                        left: Box::new(SsaValue::Var(state.lookup(var_name))),
                        op,
                        right: Box::new(rhs_val),
                    }
                } else {
                    rhs_val
                };
                // For now, we can't easily do the SSA assignment + return in an expression context.
                // But we can at least constant-fold the common case.
                if let SsaValue::Expr(ref s) = result_val {
                    if s.parse::<i64>().is_ok() {
                        // Assignment with constant value: define the var and return the value
                        // Note: we can't properly emit SSA assign here since we're in expr_to_value.
                        // Just return the value — the variable won't be updated.
                        // This handles $((x = 1)) → "1" for display purposes.
                        return result_val;
                    }
                }
                return result_val;
            }
        }
    }
    // Unary minus on variable: $((-x)) → 0 - x
    if let Some(rest) = trimmed.strip_prefix('-') {
        let inner = rest.trim();
        if inner.chars().all(|c| c.is_alphanumeric() || c == '_') && !inner.is_empty() {
            return SsaValue::ArithBinOp {
                left: Box::new(SsaValue::Expr("0".to_string())),
                op: ArithOp::Sub,
                right: Box::new(SsaValue::Var(state.lookup(inner))),
            };
        }
    }
    // Ternary: cond ? true_val : false_val
    if let Some(q_pos) = trimmed.find('?') {
        let after_q = &trimmed[q_pos + 1..];
        if let Some(c_pos) = after_q.find(':') {
            let cond_str = trimmed[..q_pos].trim();
            let true_str = after_q[..c_pos].trim();
            let false_str = after_q[c_pos + 1..].trim();
            // Recursively evaluate each part
            let cond_expr = Expr {
                kind: ExprKind::ArithmeticExpansion(cond_str.to_string()),
                span: expr.span,
            };
            let true_expr = Expr {
                kind: ExprKind::ArithmeticExpansion(true_str.to_string()),
                span: expr.span,
            };
            let false_expr = Expr {
                kind: ExprKind::ArithmeticExpansion(false_str.to_string()),
                span: expr.span,
            };
            return SsaValue::Ternary {
                cond: Box::new(expr_to_value(&cond_expr, state)),
                true_val: Box::new(expr_to_value(&true_expr, state)),
                false_val: Box::new(expr_to_value(&false_expr, state)),
            };
        }
    }
    // Parse "lhs OP rhs"
    let (lhs, op_parsed, rhs): (&str, Option<ArithOp>, &str) = parse_arith_binop(trimmed);
    if let Some(op) = op_parsed {
        return lower_arith_binop(lhs, op, rhs, s, expr, state);
    }
    // Fallback: treat as literal string
    SsaValue::Expr(format!("$(({}))", s))
}

/// Try to parse arithmetic as a literal (number, hex, octal, base, variable lookup, function call).
fn try_parse_arith_literal(trimmed: &str, expr: &Expr, state: &VersionState) -> Option<SsaValue> {
    if trimmed.is_empty() {
        return Some(SsaValue::Expr("0".to_string()));
    }
    // DD#58: look up arithmetic placeholder from preprocessor
    if trimmed.starts_with("__ARITH_") && trimmed.ends_with("__") {
        if let Some(original) = crate::lookup_arith(trimmed) {
            let e = Expr {
                kind: ExprKind::ArithmeticExpansion(original),
                span: expr.span,
            };
            return Some(expr_to_value(&e, state));
        }
    }
    // Base literal: N#digits → evaluate at compile time
    if let Some(hash_pos) = trimmed.find('#') {
        let base_str = &trimmed[..hash_pos];
        let digits = &trimmed[hash_pos + 1..];
        if let Ok(base) = base_str.parse::<u32>() {
            if (2..=64).contains(&base) && !digits.is_empty() {
                let val = digits.chars().fold(0i64, |acc, c| {
                    let d = match c {
                        '0'..='9' => c as i64 - '0' as i64,
                        'a'..='z' => c as i64 - 'a' as i64 + 10,
                        'A'..='Z' => c as i64 - 'A' as i64 + 36,
                        '@' => 62,
                        '_' => 63,
                        _ => 0,
                    };
                    acc * base as i64 + d
                });
                return Some(SsaValue::Expr(val.to_string()));
            }
        }
    }
    // Hex literal: 0x... or 0X...
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        if let Ok(v) = i64::from_str_radix(&trimmed[2..], 16) {
            return Some(SsaValue::Expr(v.to_string()));
        }
    }
    // Octal literal: 0... (but not just "0")
    if trimmed.starts_with('0')
        && trimmed.len() > 1
        && trimmed[1..].chars().all(|c| ('0'..='7').contains(&c))
    {
        if let Ok(v) = i64::from_str_radix(&trimmed[1..], 8) {
            return Some(SsaValue::Expr(v.to_string()));
        }
    }
    // Bare number: $((7)) or $((-7)) or $((+1)) → literal
    if let Ok(v) = trimmed.parse::<i64>() {
        return Some(SsaValue::Expr(v.to_string()));
    }
    // $-prefixed variable: $(($x)) or $(($1)) → variable lookup
    if trimmed.starts_with('$') {
        let varname = trimmed.trim_start_matches('$');
        if varname.chars().all(|c| c.is_alphanumeric() || c == '_') && !varname.is_empty() {
            return Some(SsaValue::ArithBinOp {
                left: Box::new(SsaValue::Var(state.lookup(varname))),
                op: ArithOp::Add,
                right: Box::new(SsaValue::Expr("0".to_string())),
            });
        }
    }
    // DD#58: Bare function name in arithmetic → int function call
    if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !trimmed.is_empty()
        && state.function_int_typed.contains(trimmed)
    {
        return Some(SsaValue::IntFuncCall {
            name: trimmed.to_string(),
            args: vec![],
        });
    }
    // DD#58: Sexpr call (name args...) in arithmetic context
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = trimmed[1..trimmed.len() - 1].trim();
        if let Some(call) = try_parse_sexpr(inner, expr.span, state) {
            return Some(call);
        }
    }
    // Bare variable: $((y)) → force arithmetic interpretation (empty/non-numeric → 0)
    if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_') && !trimmed.is_empty() {
        return Some(SsaValue::ArithBinOp {
            left: Box::new(SsaValue::Var(state.lookup(trimmed))),
            op: ArithOp::Add,
            right: Box::new(SsaValue::Expr("0".to_string())),
        });
    }
    None
}

/// Try to parse arithmetic as a unary operator (+, ~, !).
fn try_parse_arith_unary(trimmed: &str, expr: &Expr, state: &VersionState) -> Option<SsaValue> {
    if let Some(rest) = trimmed.strip_prefix('+') {
        let inner = rest.trim();
        if !inner.is_empty() {
            let e = Expr {
                kind: ExprKind::ArithmeticExpansion(inner.to_string()),
                span: expr.span,
            };
            return Some(expr_to_value(&e, state));
        }
    }
    // Unary bitwise NOT: $((~expr)) → compute inline
    if let Some(rest) = trimmed.strip_prefix('~') {
        let inner = rest.trim();
        if !inner.is_empty() {
            let e = Expr {
                kind: ExprKind::ArithmeticExpansion(inner.to_string()),
                span: expr.span,
            };
            let inner_val = expr_to_value(&e, state);
            // Constant fold if possible
            if let SsaValue::Expr(ref s) = inner_val {
                if let Ok(v) = s.parse::<i64>() {
                    return Some(SsaValue::Expr((!v).to_string()));
                }
            }
            // Runtime: XOR with -1
            return Some(SsaValue::ArithBinOp {
                left: Box::new(inner_val),
                op: ArithOp::BitXor,
                right: Box::new(SsaValue::Expr((-1i64).to_string())),
            });
        }
    }
    // Unary logical NOT: $((! expr)) → compute inline
    if trimmed.starts_with('!') && !trimmed.starts_with("!=") {
        let inner = trimmed[1..].trim();
        if !inner.is_empty() {
            let e = Expr {
                kind: ExprKind::ArithmeticExpansion(inner.to_string()),
                span: expr.span,
            };
            let inner_val = expr_to_value(&e, state);
            if let SsaValue::Expr(ref s) = inner_val {
                if let Ok(v) = s.parse::<i64>() {
                    return Some(SsaValue::Expr(if v == 0 { "1" } else { "0" }.to_string()));
                }
            }
            // Runtime: compare with 0
            return Some(SsaValue::ArithBinOp {
                left: Box::new(inner_val),
                op: ArithOp::Eq,
                right: Box::new(SsaValue::Expr("0".to_string())),
            });
        }
    }
    None
}

/// Handle the catch-all arm — primarily CommandSubstitution → FuncCall/CmdSubst.
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
                    // Sash-native builtins: len, keys (DD#41)
                    if name == "len" {
                        if let Some(arg) = c.args.first() {
                            let arg_str = expr_to_string(arg);
                            if let Some(var_name) = arg_str.strip_prefix('$') {
                                // len $var → string length
                                let v = state.lookup(var_name);
                                return SsaValue::ParamExpansion {
                                    var: v,
                                    operator: "strlen".to_string(),
                                    operand: String::new(),
                                };
                            } else {
                                // len arr → array length
                                let v = state.lookup(&arg_str);
                                return SsaValue::ParamExpansion {
                                    var: v,
                                    operator: "arrlen".to_string(),
                                    operand: String::new(),
                                };
                            }
                        }
                    }
                    if name == "keys" {
                        if let Some(arg) = c.args.first() {
                            let arg_str = expr_to_string(arg);
                            let v = state.lookup(&arg_str);
                            return SsaValue::ParamExpansion {
                                var: v,
                                operator: "arrkeys".to_string(),
                                operand: String::new(),
                            };
                        }
                    }
                    if name == "join" {
                        // join @arr → space-separated
                        // join sep @arr → sep-separated
                        let (sep, arr_expr) = if c.args.len() >= 2 {
                            // Strip quotes from separator
                            let sep_raw = match &c.args[0].kind {
                                ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => s.clone(),
                                ExprKind::DoubleQuoted(parts) if parts.len() == 1 => {
                                    match &parts[0].kind {
                                        ExprKind::Literal(s) => s.clone(),
                                        _ => expr_to_string(&c.args[0]),
                                    }
                                }
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
                            ExprKind::ParameterExpansion(pe)
                                if pe.operator.as_deref() == Some("arrvals") =>
                            {
                                pe.name.clone()
                            }
                            _ => expr_to_string(arr_expr),
                        };
                        let v = state.lookup(&arr_name);
                        return SsaValue::ParamExpansion {
                            var: v,
                            operator: "arrjoin".to_string(),
                            operand: sep,
                        };
                    }
                }
                // DD#14: $(echo args) — build inline as concat
                if let ExprKind::Literal(name) = &c.name.kind {
                    if name == "echo" {
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
                        return if parts.len() == 1 {
                            parts.remove(0)
                        } else {
                            SsaValue::Concat(parts)
                        };
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

/// Lower a binary arithmetic expression given parsed components.
fn lower_arith_binop(
    lhs: &str,
    op: ArithOp,
    rhs: &str,
    s: &str,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    let lhs_was_var = lhs.starts_with('$');
    let rhs_was_var = rhs.starts_with('$');
    let lhs = lhs.trim_start_matches('$');
    let rhs = rhs.trim_start_matches('$');

    // Helper: convert an arithmetic sub-expression string to SsaValue
    let to_val = |s: &str| -> SsaValue {
        let e = Expr {
            kind: ExprKind::ArithmeticExpansion(s.to_string()),
            span: expr.span,
        };
        expr_to_value(&e, state)
    };

    let l_const = if lhs_was_var {
        Err(std::num::IntErrorKind::InvalidDigit)
    } else {
        lhs.parse::<i64>().map_err(|e| *e.kind())
    };
    let r_const = if rhs_was_var {
        Err(std::num::IntErrorKind::InvalidDigit)
    } else {
        rhs.parse::<i64>().map_err(|e| *e.kind())
    };

    // Check if lhs/rhs are complex (contain operators/parens) — if so, recurse
    let l_is_simple = lhs.chars().all(|c| c.is_alphanumeric() || c == '_');
    let r_is_simple = rhs.chars().all(|c| c.is_alphanumeric() || c == '_');

    match (l_const, r_const) {
        (Ok(l), Ok(r)) => lower_arith_both_literal(l, r, op, lhs, rhs, state),
        (Err(_), Ok(operand)) => {
            if l_is_simple {
                // DD#58: check if lhs is a known int function
                if state.function_int_typed.contains(lhs) {
                    return SsaValue::ArithBinOp {
                        left: Box::new(SsaValue::IntFuncCall {
                            name: lhs.to_string(),
                            args: vec![],
                        }),
                        op,
                        right: Box::new(SsaValue::Expr(operand.to_string())),
                    };
                }
                return SsaValue::ArithExpr {
                    var: state.lookup(lhs),
                    op,
                    operand,
                };
            } else {
                return SsaValue::ArithBinOp {
                    left: Box::new(to_val(lhs)),
                    op,
                    right: Box::new(SsaValue::Expr(operand.to_string())),
                };
            }
        }
        (Ok(l_val), Err(_)) => {
            if r_is_simple {
                match op {
                    ArithOp::Add
                    | ArithOp::Mul
                    | ArithOp::Eq
                    | ArithOp::Ne
                    | ArithOp::And
                    | ArithOp::Or => {
                        return SsaValue::ArithExpr {
                            var: state.lookup(rhs),
                            op,
                            operand: l_val,
                        };
                    }
                    _ => {
                        return SsaValue::ArithBinOp {
                            left: Box::new(SsaValue::Expr(l_val.to_string())),
                            op,
                            right: Box::new(SsaValue::Var(state.lookup(rhs))),
                        };
                    }
                }
            } else {
                return SsaValue::ArithBinOp {
                    left: Box::new(SsaValue::Expr(l_val.to_string())),
                    op,
                    right: Box::new(to_val(rhs)),
                };
            }
        }
        (Err(_), Err(_)) => {
            let left = if l_is_simple {
                SsaValue::Var(state.lookup(lhs))
            } else {
                to_val(lhs)
            };
            let right = if r_is_simple {
                SsaValue::Var(state.lookup(rhs))
            } else {
                to_val(rhs)
            };
            return SsaValue::ArithBinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
    }
}

/// Both operands are numeric literals — try constant folding or emit binop.
fn lower_arith_both_literal(
    l: i64,
    r: i64,
    op: ArithOp,
    lhs: &str,
    rhs: &str,
    state: &VersionState,
) -> SsaValue {
    // Constant fold
    let result = match op {
        ArithOp::Add => l + r,
        ArithOp::Sub => l - r,
        ArithOp::Mul => l * r,
        ArithOp::Div => {
            if r != 0 {
                l / r
            } else {
                return SsaValue::ArithBinOp {
                    left: Box::new(SsaValue::Expr(l.to_string())),
                    op: ArithOp::Div,
                    right: Box::new(SsaValue::Expr(r.to_string())),
                };
            }
        }
        ArithOp::Mod => {
            if r != 0 {
                l % r
            } else {
                return SsaValue::ArithBinOp {
                    left: Box::new(SsaValue::Expr(l.to_string())),
                    op: ArithOp::Mod,
                    right: Box::new(SsaValue::Expr(r.to_string())),
                };
            }
        }
        ArithOp::Eq => {
            if l == r {
                1
            } else {
                0
            }
        }
        ArithOp::Ne => {
            if l != r {
                1
            } else {
                0
            }
        }
        ArithOp::Lt => {
            if l < r {
                1
            } else {
                0
            }
        }
        ArithOp::Le => {
            if l <= r {
                1
            } else {
                0
            }
        }
        ArithOp::Gt => {
            if l > r {
                1
            } else {
                0
            }
        }
        ArithOp::Ge => {
            if l >= r {
                1
            } else {
                0
            }
        }
        ArithOp::And => {
            if l != 0 && r != 0 {
                1
            } else {
                0
            }
        }
        ArithOp::Or => {
            if l != 0 || r != 0 {
                1
            } else {
                0
            }
        }
        ArithOp::Not => 0, // shouldn't appear as binary
        ArithOp::Shl => l << r,
        ArithOp::Shr => l >> r,
        ArithOp::BitAnd => l & r,
        ArithOp::BitOr => l | r,
        ArithOp::BitXor => l ^ r,
        ArithOp::Pow => l.pow(r as u32),
    };
    return SsaValue::Expr(result.to_string());
}

/// Handle structured ArithExpr from the Pratt parser.
/// Converts ir::ArithExpr → SsaValue.
pub(super) fn handle_structured_arithmetic(arith: &ArithExpr, expr: &Expr, state: &VersionState) -> SsaValue {
    arith_to_ssa(arith, expr, state)
}

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
        ArithExpr::DollarVar(name) => {
            SsaValue::Var(state.lookup(name))
        }
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
        ArithExpr::Assign { var, value } => {
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
        ArithExpr::Ternary { cond, true_val, false_val } => {
            SsaValue::Ternary {
                cond: Box::new(arith_to_ssa(cond, expr, state)),
                true_val: Box::new(arith_to_ssa(true_val, expr, state)),
                false_val: Box::new(arith_to_ssa(false_val, expr, state)),
            }
        }
        ArithExpr::Comma(exprs) => {
            // In expression context, comma returns the last value
            if let Some(last) = exprs.last() {
                arith_to_ssa(last, expr, state)
            } else {
                SsaValue::Expr("0".to_string())
            }
        }
        ArithExpr::FuncCall { name, args } => {
            let ssa_args: Vec<SsaValue> = args.iter().map(|a| arith_to_ssa(a, expr, state)).collect();
            SsaValue::IntFuncCall {
                name: name.clone(),
                args: ssa_args,
            }
        }
        ArithExpr::Paren(inner) => arith_to_ssa(inner, expr, state),
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
        ArithOp::Div => if r != 0 { l / r } else { 0 },
        ArithOp::Mod => if r != 0 { l % r } else { 0 },
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
