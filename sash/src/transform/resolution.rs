//! Command resolution and sexpr parsing.
//!
//! TRUST: resolve_pipeline_command uses compile-time PATH resolution (DD#23).
//! No runtime PATH lookup — the resolved absolute path is embedded in the binary.

use super::expressions::expr_to_value;
use crate::ir::*;
use crate::ssa::*;

pub(super) fn resolve_pipeline_command(name: &str) -> String {
    if name.contains('/') {
        return name.to_string();
    }
    // Inline builtins handled directly in codegen — no resolution needed
    if matches!(name, "echo" | "true" | "false" | ":") {
        return name.to_string();
    }
    // DD#23: compile-time PATH resolution
    resolve_command_path(name).unwrap_or_else(|| name.to_string())
}

/// DD#23: Search PATH for a command, return absolute path.
/// Respects --sysroot for cross-compilation.
pub(super) fn resolve_command_path(name: &str) -> Option<String> {
    let sysroot = crate::get_sysroot();
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        let target_path = format!("{}/{}", dir, name);
        let exists = if let Some(ref sr) = sysroot {
            crate::sysroot_file_exists(sr, &target_path)
        } else {
            std::path::Path::new(&target_path).is_file()
        };
        if exists {
            return Some(target_path);
        }
    }
    None
}

pub(super) fn try_parse_sexpr(inner: &str, span: Span, state: &VersionState) -> Option<SsaValue> {
    let inner = inner.trim();
    if inner.is_empty() {
        return None;
    }

    // Extract the function name (first whitespace-delimited token)
    let name_end = inner
        .find(|c: char| c.is_whitespace())
        .unwrap_or(inner.len());
    let name = &inner[..name_end];

    // Must be a known -> int function
    if !state.function_int_typed.contains(name) {
        return None;
    }

    // Parse arguments (space-separated, respecting nested parens)
    let args_str = inner[name_end..].trim();
    let args = if args_str.is_empty() {
        vec![]
    } else {
        parse_sexpr_args(args_str, span, state)
    };

    Some(SsaValue::IntFuncCall {
        name: name.to_string(),
        args,
    })
}

/// Parse space-separated arguments for a sexpr call, respecting nested parens.
pub(super) fn parse_sexpr_args(s: &str, span: Span, state: &VersionState) -> Vec<SsaValue> {
    let mut args = Vec::new();
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        // Skip whitespace
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        if bytes[i] == b'(' {
            // Find matching close paren
            let start = i;
            let mut depth = 0;
            while i < bytes.len() {
                if bytes[i] == b'(' {
                    depth += 1;
                }
                if bytes[i] == b')' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            let token = s[start..i].trim();
            // Try as nested sexpr
            let inner = &token[1..token.len() - 1];
            if let Some(call) = try_parse_sexpr(inner, span, state) {
                args.push(call);
            } else {
                // Arithmetic grouping: evaluate as arithmetic expression
                let e = Expr {
                    kind: ExprKind::ArithmeticExpansion(inner.to_string()),
                    span,
                };
                args.push(expr_to_value(&e, state));
            }
        } else {
            // Read until whitespace or paren
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'(' {
                i += 1;
            }
            let token = &s[start..i];
            // Parse as arithmetic expression (number or variable)
            let e = Expr {
                kind: ExprKind::ArithmeticExpansion(token.to_string()),
                span,
            };
            args.push(expr_to_value(&e, state));
        }
    }
    args
}

fn parse_arith_binop(s: &str) -> (&str, Option<ArithOp>, &str) {
    let bytes = s.as_bytes();
    let start = if bytes.first() == Some(&b'-') { 1 } else { 0 };
    // Check two-char operators first: ==, !=, <=, >=, ||, &&
    // Skip operators inside parentheses
    let mut depth = 0i32;
    for i in start..bytes.len().saturating_sub(1) {
        if bytes[i] == b'(' {
            depth += 1;
            continue;
        }
        if bytes[i] == b')' {
            depth -= 1;
            continue;
        }
        if depth > 0 {
            continue;
        }
        let two = &s[i..i + 2];
        if let Some(op) = ArithOp::from_str(two) {
            let lhs = s[..i].trim();
            let rhs = s[i + 2..].trim();
            if !lhs.is_empty() && !rhs.is_empty() {
                return (lhs, Some(op), rhs);
            }
        }
    }
    // Single-char operators: +, -, *, /, %, <, >
    depth = 0;
    for (i, &b) in bytes[start..].iter().enumerate() {
        let pos = i + start;
        if b == b'(' {
            depth += 1;
            continue;
        }
        if b == b')' {
            depth -= 1;
            continue;
        }
        if depth > 0 {
            continue;
        }
        let op_str = &s[pos..pos + 1];
        if let Some(op) = ArithOp::from_str(op_str) {
            let lhs = s[..pos].trim();
            let rhs = s[pos + 1..].trim();
            if !lhs.is_empty() && !rhs.is_empty() {
                return (lhs, Some(op), rhs);
            }
        }
    }
    // Check if the whole thing is parenthesized: (expr)
    let trimmed = s.trim();
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        // Strip outer parens and recurse
        let inner = &trimmed[1..trimmed.len() - 1];
        return parse_arith_binop(inner);
    }
    (s, None, "")
}
