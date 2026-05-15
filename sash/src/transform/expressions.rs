//! Expression → SSA value conversion.
//!
//! SECURITY: expr_to_value converts IR expressions to SsaValue.
//! This is where taint originates — $(getenv), $(argv), command
//! substitution all produce tainted SsaValues.

use super::helpers::parse_arith_binop;
use super::resolution::try_parse_sexpr;
use crate::ir::*;
use crate::ssa::*;

pub(super) fn expr_to_value(expr: &Expr, state: &VersionState) -> SsaValue {
    match &expr.kind {
        ExprKind::Literal(s) => SsaValue::Literal(s.clone()),
        ExprKind::SimpleExpansion(var) => {
            // Coerce magic variables to builtins (DD#40)
            if var == "EPOCHSECONDS" {
                return SsaValue::CmdSubst {
                    argv: vec![SsaValue::Literal("epochseconds".to_string())],
                    redirects: vec![],
                };
            }
            SsaValue::Var(state.lookup(var))
        }
        ExprKind::SingleQuoted(s) => SsaValue::Literal(s.clone()),
        ExprKind::ArithmeticExpansion(s) => {
            super::expr_handlers::handle_arithmetic_expansion(s, expr, state)
        }
        ExprKind::ParameterExpansion(pe) => {
            let var = state.lookup(&pe.name);
            if let Some(ref op) = pe.operator {
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
            } else {
                SsaValue::Var(var)
            }
        }
        ExprKind::DoubleQuoted(parts) => {
            // Unwrap single-element DoubleQuoted to its inner value.
            if parts.len() == 1 {
                return expr_to_value(&parts[0], state);
            }
            // Multi-part: check if all literal
            let mut all_literal = true;
            let mut lit_concat = String::new();
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
                SsaValue::Literal(lit_concat)
            } else {
                // Build a Concat of resolved parts
                let mut concat_parts = Vec::new();
                for part in parts {
                    concat_parts.push(expr_to_value(part, state));
                }
                SsaValue::Concat(concat_parts)
            }
        }
        ExprKind::AssocArray(pairs) => {
            let vals: Vec<(String, SsaValue)> = pairs
                .iter()
                .map(|(k, v)| (k.clone(), expr_to_value(v, state)))
                .collect();
            SsaValue::AssocArray(vals)
        }
        ExprKind::Subscript { name, index } => {
            let is_integer = !state.assoc_arrays.contains(name);
            SsaValue::Subscript {
                array: name.clone(),
                index: Box::new(expr_to_value(index, state)),
                is_integer_key: is_integer,
            }
        }
        ExprKind::Concat(parts) => {
            let vals: Vec<SsaValue> = parts.iter().map(|p| expr_to_value(p, state)).collect();
            if vals.len() == 1 {
                return vals.into_iter().next().unwrap();
            }
            SsaValue::Concat(vals)
        }
        ExprKind::Array(elems) => {
            let vals: Vec<SsaValue> = elems.iter().map(|e| expr_to_value(e, state)).collect();
            // Check for regex filter pattern: (* =~ 'regex')
            // vals would be ["*", "=~", "pattern"]
            if vals.len() == 3 {
                if let (SsaValue::Literal(star), SsaValue::Literal(op), SsaValue::Literal(pat)) =
                    (&vals[0], &vals[1], &vals[2])
                {
                    if star == "*" && op == "=~" {
                        return SsaValue::ParamExpansion {
                            var: VarRef::new(".", 0),
                            operator: "glob_regex".to_string(),
                            operand: pat.clone(),
                        };
                    }
                }
            }
            // Check if any element is a glob pattern (contains *)
            let has_glob = vals.iter().any(|v| {
                if let SsaValue::Literal(s) = v {
                    s.contains('*')
                } else {
                    false
                }
            });
            if has_glob && vals.len() == 1 {
                // Single glob pattern: arr=(*.sh) → sash_glob(".", "*.sh")
                SsaValue::ParamExpansion {
                    var: VarRef::new(".", 0),
                    operator: "glob".to_string(),
                    operand: if let SsaValue::Literal(s) = &vals[0] {
                        s.clone()
                    } else {
                        String::new()
                    },
                }
            } else {
                SsaValue::IndexedArray(vals)
            }
        }
        _ => super::expr_handlers::handle_command_substitution(expr, state),
    }
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
        ExprKind::ArithmeticExpansion(e) => format!("$(({}))", e),
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
