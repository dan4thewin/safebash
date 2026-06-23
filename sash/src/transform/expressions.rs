//! Expression → SSA value conversion.
//!
//! SECURITY: expr_to_value converts IR expressions to SsaValue.
//! This is where taint originates — $(getenv), $(argv), command
//! substitution all produce tainted SsaValues.

use crate::ir::*;
use crate::ssa::*;

pub(super) fn expr_to_value(expr: &Expr, state: &VersionState) -> SsaValue {
    match &expr.kind {
        ExprKind::Literal(s) => SsaValue::Literal(s.clone()),
        ExprKind::SimpleExpansion(var) => simple_expansion_to_value(var, state),
        ExprKind::SingleQuoted(s) => SsaValue::Literal(s.clone()),
        ExprKind::Arithmetic(a) => {
            super::expr_handlers::handle_structured_arithmetic(a, expr, state)
        }
        ExprKind::ParameterExpansion(pe) => parameter_expansion_to_value(pe, state),
        ExprKind::DoubleQuoted(parts) => dq_to_value(parts, state),
        ExprKind::AssocArray(pairs) => assoc_array_to_value(pairs, state),
        ExprKind::Subscript { name, index } => subscript_to_value(name, index, state),
        ExprKind::Concat(parts) => concat_to_value(parts, state),
        ExprKind::Array(elems) => lower_array_literal(elems, state),
        _ => super::expr_handlers::handle_command_substitution(expr, state),
    }
}

/// Lower a command-*argument* expression to an `SsaValue`, recognising
/// the `@a` / `"${a[@]}"` array splat (a `ParameterExpansion` whose
/// operator is `arrvals`) as [`SsaValue::ArraySplat`] so it expands to
/// one verbatim argv entry per element at codegen time (DD#20/DD#22).
/// Outside command-arg position `@a` keeps its joined-string meaning via
/// [`expr_to_value`].
pub(super) fn arg_to_value(expr: &Expr, state: &VersionState) -> SsaValue {
    match command_arg_splat(&expr.kind) {
        Some(array) => SsaValue::ArraySplat { array },
        None => expr_to_value(expr, state),
    }
}

/// Recognise `@a` and the equivalent single-part `"${a[@]}"` as an array
/// splat, returning the array name.
fn command_arg_splat(kind: &ExprKind) -> Option<String> {
    match kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            Some(pe.name.clone())
        }
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => command_arg_splat(&parts[0].kind),
        _ => None,
    }
}

/// DD#62: pre-lower a `[[ ]]` `TestExpr` into an `SsaTest` whose value
/// operands are `SsaValue` (so codegen uses the ordinary value path,
/// with `assoc_arrays`-correct subscript key-typing). `=~`/`=glob` right
/// operands stay as a raw pattern string.
pub(super) fn test_to_ssa(test: &TestExpr, state: &VersionState) -> SsaTest {
    match test {
        TestExpr::Unary { op, operand } => SsaTest::Unary {
            op: op.clone(),
            operand: expr_to_value(operand, state),
        },
        TestExpr::Binary { left, op, right } => {
            let rhs = if op == "=~" || op == "=glob" {
                SsaTestRhs::Pattern(expr_to_string(right))
            } else {
                SsaTestRhs::Value(expr_to_value(right, state))
            };
            SsaTest::Binary {
                left: expr_to_value(left, state),
                op: op.clone(),
                right: rhs,
            }
        }
        TestExpr::Not(inner) => SsaTest::Not(Box::new(test_to_ssa(inner, state))),
        TestExpr::And(a, b) => SsaTest::And(
            Box::new(test_to_ssa(a, state)),
            Box::new(test_to_ssa(b, state)),
        ),
        TestExpr::Or(a, b) => SsaTest::Or(
            Box::new(test_to_ssa(a, state)),
            Box::new(test_to_ssa(b, state)),
        ),
    }
}
fn simple_expansion_to_value(var: &str, state: &VersionState) -> SsaValue {
    if var == "EPOCHSECONDS" {
        return SsaValue::CmdSubst {
            argv: vec![SsaValue::Literal("epochseconds".to_string())],
            redirects: vec![],
        };
    }
    SsaValue::Var(state.lookup(var))
}

fn parameter_expansion_to_value(pe: &ParameterExpansion, state: &VersionState) -> SsaValue {
    let var = state.lookup(&pe.name);
    let Some(op) = pe.operator.as_ref() else {
        return SsaValue::Var(var);
    };
    let operand = pe
        .operand
        .as_ref()
        .map(|e| expr_to_string(e))
        .unwrap_or_default();
    SsaValue::ParamExpansion {
        var,
        operator: op.clone(),
        operand,
    }
}

fn assoc_array_to_value(pairs: &[(String, Expr)], state: &VersionState) -> SsaValue {
    let vals: Vec<(String, SsaValue)> = pairs
        .iter()
        .map(|(k, v)| (k.clone(), expr_to_value(v, state)))
        .collect();
    SsaValue::AssocArray(vals)
}

pub(super) fn subscript_to_value(name: &str, index: &Expr, state: &VersionState) -> SsaValue {
    let is_integer = !state.assoc_arrays.contains(name);
    SsaValue::Subscript {
        array: name.to_string(),
        index: Box::new(expr_to_value(index, state)),
        is_integer_key: is_integer,
    }
}

fn concat_to_value(parts: &[Expr], state: &VersionState) -> SsaValue {
    let mut vals: Vec<SsaValue> = parts.iter().map(|p| expr_to_value(p, state)).collect();
    if vals.len() == 1 {
        return vals.pop().unwrap();
    }
    SsaValue::Concat(vals)
}

/// `"…"` double-quoted: single-part is the part itself; all-literal parts
/// flatten to a single literal; otherwise build a `Concat`.
fn dq_to_value(parts: &[Expr], state: &VersionState) -> SsaValue {
    if parts.len() == 1 {
        return expr_to_value(&parts[0], state);
    }
    let mut lit_concat = String::new();
    let mut all_literal = true;
    for part in parts {
        match &part.kind {
            ExprKind::Literal(lit) => lit_concat.push_str(lit),
            _ => {
                all_literal = false;
                break;
            }
        }
    }
    if all_literal {
        return SsaValue::Literal(lit_concat);
    }
    let concat_parts: Vec<SsaValue> = parts.iter().map(|p| expr_to_value(p, state)).collect();
    SsaValue::Concat(concat_parts)
}

/// `arr=(…)` array literal: detect two sash-native shorthand patterns
/// (regex filter and single-glob) before falling back to IndexedArray.
fn lower_array_literal(elems: &[Expr], state: &VersionState) -> SsaValue {
    let vals: Vec<SsaValue> = elems.iter().map(|e| expr_to_value(e, state)).collect();
    if let Some(v) = try_array_regex_filter(&vals) {
        return v;
    }
    if let Some(v) = try_array_single_glob(&vals) {
        return v;
    }
    SsaValue::IndexedArray(vals)
}

/// arr=(* =~ 'pat') → sash_glob(".") | grep -E 'pat' shorthand.
fn try_array_regex_filter(vals: &[SsaValue]) -> Option<SsaValue> {
    if vals.len() != 3 {
        return None;
    }
    let (SsaValue::Literal(star), SsaValue::Literal(op), SsaValue::Literal(pat)) =
        (&vals[0], &vals[1], &vals[2])
    else {
        return None;
    };
    if star != "*" || op != "=~" {
        return None;
    }
    Some(SsaValue::ParamExpansion {
        var: VarRef::new(".", 0),
        operator: "glob_regex".to_string(),
        operand: pat.clone(),
    })
}

/// arr=(*.sh) → sash_glob(".", "*.sh") shorthand.
fn try_array_single_glob(vals: &[SsaValue]) -> Option<SsaValue> {
    if vals.len() != 1 {
        return None;
    }
    let SsaValue::Literal(s) = &vals[0] else {
        return None;
    };
    if !s.contains('*') {
        return None;
    }
    Some(SsaValue::ParamExpansion {
        var: VarRef::new(".", 0),
        operator: "glob".to_string(),
        operand: s.clone(),
    })
}

/// Convert an expression to a simple string representation.
pub(super) fn expr_to_string(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => s.clone(),
        ExprKind::SimpleExpansion(var) => format!("${}", var),
        ExprKind::SingleQuoted(s) => format!("'{}'", s),
        ExprKind::DoubleQuoted(parts) => {
            let mut s = String::from("\"");
            for p in parts {
                s.push_str(&expr_to_string(p));
            }
            s.push('"');
            s
        }
        ExprKind::ParameterExpansion(pe) => {
            let mut s = format!("${{{}", pe.name);
            if let Some(op) = &pe.operator {
                s.push_str(op);
            }
            s.push('}');
            s
        }
        ExprKind::CommandSubstitution(_) => "$(...)".to_string(),
        ExprKind::Arithmetic(_) => "$(( ... ))".to_string(),
        ExprKind::Concat(parts) => parts.iter().map(expr_to_string).collect(),
        ExprKind::Array(elems) => {
            let inner: Vec<String> = elems.iter().map(expr_to_string).collect();
            format!("({})", inner.join(" "))
        }
        ExprKind::AssocArray(pairs) => {
            let inner: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("[{}]={}", k, expr_to_string(v)))
                .collect();
            format!("({})", inner.join(" "))
        }
        ExprKind::Subscript { name, index } => {
            format!("${{{}[{}]}}", name, expr_to_string(index))
        }
        ExprKind::Test(_) => "[[ ... ]]".to_string(),
    }
}
