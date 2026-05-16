//! Expression lowering: CST expression nodes → IR Expr.
//!
//! SECURITY: Expressions are the data-carrying parts of the IR.
//! String content from source is preserved exactly (no interpretation).

use super::arithmetic::lower_arith_expr_to_exprkind;
use super::helpers::has_error_descendant;
use super::statements::lower_statement;
use crate::ir::*;
use tree_sitter::Node;

pub(super) fn lower_expr(node: Node, source: &[u8]) -> Expr {
    let span = Span::from_node(node);
    let kind = match node.kind() {
        "word" | "extglob_pattern" => {
            let text = node.utf8_text(source).unwrap_or("").trim().to_string();
            if text.starts_with("~/") {
                // Tilde expansion: ~/foo → $HOME/foo
                ExprKind::Concat(vec![
                    Expr {
                        kind: ExprKind::SimpleExpansion("HOME".to_string()),
                        span,
                    },
                    Expr {
                        kind: ExprKind::Literal(text[1..].to_string()),
                        span,
                    },
                ])
            } else if text == "~" {
                ExprKind::SimpleExpansion("HOME".to_string())
            } else if text.starts_with('@') && text.len() > 1 {
                // Sash-native: @arr → all array elements (DD#41)
                let name = text[1..].to_string();
                ExprKind::ParameterExpansion(ParameterExpansion {
                    name,
                    operator: Some("arrvals".to_string()),
                    operand: None,
                })
            } else if node.named_child_count() > 0 && {
                // Only enter expansion path if children are actual expansions
                let mut has_expansion = false;
                let mut cursor_check = node.walk();
                for child in node.named_children(&mut cursor_check) {
                    match child.kind() {
                        "simple_expansion" | "expansion" | "command_substitution" => {
                            has_expansion = true;
                            break;
                        }
                        _ => {}
                    }
                }
                has_expansion
            } {
                // Word contains expansions (e.g., ${var} in [[ =~ ]] context)
                // Build parts like lower_string
                let mut parts = Vec::new();
                let content_start = node.start_byte();
                let content_end = node.end_byte();
                let mut last_end = content_start;
                let mut cursor2 = node.walk();
                for child in node.children(&mut cursor2) {
                    if !child.is_named() {
                        continue;
                    }
                    let cs = child.start_byte();
                    if cs > last_end {
                        let gap = std::str::from_utf8(&source[last_end..cs]).unwrap_or("");
                        if !gap.is_empty() {
                            parts.push(Expr {
                                kind: ExprKind::Literal(gap.to_string()),
                                span,
                            });
                        }
                    }
                    parts.push(lower_expr(child, source));
                    last_end = child.end_byte();
                }
                if last_end < content_end {
                    let gap = std::str::from_utf8(&source[last_end..content_end]).unwrap_or("");
                    if !gap.is_empty() {
                        parts.push(Expr {
                            kind: ExprKind::Literal(gap.to_string()),
                            span,
                        });
                    }
                }
                if parts.len() == 1 {
                    parts.into_iter().next().unwrap().kind
                } else {
                    ExprKind::Concat(parts)
                }
            } else {
                ExprKind::Literal(text)
            }
        }

        "string" => lower_string(node, source),

        "raw_string" => {
            let text = node.utf8_text(source).unwrap_or("''");
            let inner = &text[1..text.len().saturating_sub(1)];
            ExprKind::SingleQuoted(inner.to_string())
        }

        "ansi_c_string" => lower_ansi_c_string(node, source),

        "string_content" => {
            let text = node.utf8_text(source).unwrap_or("").to_string();
            ExprKind::Literal(unescape_double_quoted(&text))
        }

        "simple_expansion" => {
            let var = node
                .child(1)
                .and_then(|c| c.utf8_text(source).ok())
                .unwrap_or("?")
                .to_string();
            // DD#24: $LINENO → compile-time literal line number
            if var == "LINENO" {
                let line = node.start_position().row + 1;
                return Expr {
                    kind: ExprKind::Literal(line.to_string()),
                    span: Span::from_node(node),
                };
            }
            ExprKind::SimpleExpansion(var)
        }

        "expansion" => lower_parameter_expansion(node, source),

        "command_substitution" => {
            // DD#55: $(status) → $? and $(getcpid) → $!
            // Check for single bare command that is a sash special form
            let inner_stmts: Vec<_> = node
                .children(&mut node.walk())
                .filter(|c| c.is_named())
                .collect();
            if inner_stmts.len() == 1 && inner_stmts[0].kind() == "command" {
                let cmd_node = inner_stmts[0];
                let cmd_name_node = cmd_node.child_by_field_name("name");
                let has_args = cmd_node
                    .children(&mut cmd_node.walk())
                    .filter(|c| c.is_named() && c.kind() != "command_name")
                    .count()
                    > 0;
                if !has_args {
                    if let Some(name_node) = cmd_name_node {
                        let name = name_node.utf8_text(source).unwrap_or("");
                        if name == "status" {
                            let span = Span::from_node(node);
                            return Expr {
                                kind: ExprKind::SimpleExpansion("?".to_string()),
                                span,
                            };
                        }
                        if name == "getcpid" {
                            let span = Span::from_node(node);
                            return Expr {
                                kind: ExprKind::SimpleExpansion("!".to_string()),
                                span,
                            };
                        }
                    }
                }
            }
            let mut stmts = Vec::new();
            let mut is_file_slurp = false;
            let mut slurp_path = String::new();
            for child in node.children(&mut node.walk()) {
                if child.kind() == "file_redirect" {
                    // Check for $(<file) — file slurp pattern
                    let mut has_read = false;
                    let mut path = String::new();
                    let mut rc = child.walk();
                    for rchild in child.children(&mut rc) {
                        if rchild.kind() == "<" {
                            has_read = true;
                        }
                        if rchild.kind() == "word"
                            || rchild.kind() == "string"
                            || rchild.kind() == "concatenation"
                        {
                            path = rchild.utf8_text(source).unwrap_or("").to_string();
                        }
                    }
                    if has_read && !path.is_empty() {
                        is_file_slurp = true;
                        slurp_path = path;
                    }
                } else if child.is_named() {
                    if let Some(s) = lower_statement(child, source) {
                        stmts.push(s);
                    }
                }
            }
            if is_file_slurp && stmts.is_empty() {
                // $(<file) → CommandSubstitution with a single `cat file` command
                ExprKind::CommandSubstitution(vec![Statement {
                    kind: StmtKind::Command(Command {
                        name: Expr {
                            kind: ExprKind::Literal("cat".to_string()),
                            span: Span { line: 0, col: 0 },
                        },
                        args: vec![Expr {
                            kind: ExprKind::Literal(slurp_path),
                            span: Span { line: 0, col: 0 },
                        }],
                    }),
                    span: Span { line: 0, col: 0 },
                }])
            } else {
                ExprKind::CommandSubstitution(stmts)
            }
        }

        "arithmetic_expansion" => {
            // DD#58: If arithmetic contains ERROR nodes (e.g., sexpr syntax),
            // fall back to raw text so the transform text parser can handle it.
            if has_error_descendant(node) {
                let text = node.utf8_text(source).unwrap_or("$((0))");
                let inner = text
                    .strip_prefix("$((")
                    .unwrap_or(text)
                    .strip_suffix("))")
                    .unwrap_or(text)
                    .trim()
                    .to_string();
                ExprKind::ArithmeticExpansion(inner)
            } else {
                // Walk tree-sitter's AST instead of text-parsing
                // For comma operator: multiple named children — use the last one
                let count = node.named_child_count();
                if count > 1 {
                    // Comma operator: $((a, b, c)) → evaluate all, return last
                    let last = node.named_child(count - 1).unwrap();
                    lower_arith_expr_to_exprkind(last, source)
                } else if let Some(expr_node) = node.named_child(0) {
                    lower_arith_expr_to_exprkind(expr_node, source)
                } else {
                    // Fallback: extract text
                    let text = node.utf8_text(source).unwrap_or("$((??))");
                    let inner = text
                        .strip_prefix("$((")
                        .unwrap_or(text)
                        .strip_suffix("))")
                        .unwrap_or(text)
                        .trim()
                        .to_string();
                    ExprKind::ArithmeticExpansion(inner)
                }
            } // end if has_error_descendant else
        }

        "concatenation" => {
            let mut parts = Vec::new();
            for child in node.children(&mut node.walk()) {
                if child.is_named() {
                    parts.push(lower_expr(child, source));
                }
            }
            // Sash-native: $arr[i] → Subscript (DD#41)
            // CST: concatenation(simple_expansion($arr), word([), number(i), word(]))
            // or:  concatenation(simple_expansion($arr), word([i]))
            let full_text = node.utf8_text(source).unwrap_or("");
            if parts.len() >= 2 {
                if let ExprKind::SimpleExpansion(ref name) = parts[0].kind {
                    // Check if remaining text after $name is [index]
                    let after_expansion = &full_text[name.len() + 1..]; // skip $name
                    if after_expansion.starts_with('[') && after_expansion.ends_with(']') {
                        let idx = &after_expansion[1..after_expansion.len() - 1];
                        let index_expr = if idx == "@" || idx == "*" {
                            // $arr[@] → all values
                            return Expr {
                                kind: ExprKind::ParameterExpansion(ParameterExpansion {
                                    name: name.clone(),
                                    operator: Some("arrvals".to_string()),
                                    operand: None,
                                }),
                                span,
                            };
                        } else {
                            Expr {
                                kind: ExprKind::Literal(idx.to_string()),
                                span,
                            }
                        };
                        return Expr {
                            kind: ExprKind::Subscript {
                                name: name.clone(),
                                index: Box::new(index_expr),
                            },
                            span,
                        };
                    }
                }
            }
            ExprKind::Concat(parts)
        }

        "array" => {
            let mut elems = Vec::new();
            for child in node.children(&mut node.walk()) {
                if child.is_named() {
                    elems.push(lower_expr(child, source));
                }
            }
            ExprKind::Array(elems)
        }

        "command_name" => {
            if let Some(inner) = node.child(0) {
                return lower_expr(inner, source);
            }
            ExprKind::Literal("?".to_string())
        }

        "number" => {
            let text = node.utf8_text(source).unwrap_or("0").to_string();
            ExprKind::Literal(text)
        }

        "test_command" => lower_test_command(node, source),

        "regex" => lower_regex_node(node, source, span),

        _ => {
            let text = node.utf8_text(source).unwrap_or("??").to_string();
            ExprKind::Literal(text)
        }
    };

    Expr { kind, span }
}

fn lower_regex_node(node: Node, source: &[u8], span: Span) -> ExprKind {
    // Regex nodes in [[ =~ ]] — tree-sitter doesn't expand ${var} inside regex.
    // Manually parse variable references.
    let text = node.utf8_text(source).unwrap_or("").to_string();
    if text.contains("${")
        || (text.contains('$') && {
            // Only treat $ as expansion if followed by alpha/_ or {
            let b = text.as_bytes();
            b.iter().enumerate().any(|(i, &c)| {
                c == b'$'
                    && i + 1 < b.len()
                    && (b[i + 1] == b'{' || b[i + 1].is_ascii_alphabetic() || b[i + 1] == b'_')
            })
        })
    {
        // Parse variable expansions manually
        let mut parts = Vec::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        let mut literal_start = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' {
                // Capture preceding literal
                if i > literal_start {
                    parts.push(Expr {
                        kind: ExprKind::Literal(text[literal_start..i].to_string()),
                        span: Span::from_node(node),
                    });
                }
                if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                    // ${var} or ${var[...]}
                    if let Some(close) = text[i..].find('}') {
                        let var_name = &text[i + 2..i + close];
                        parts.push(Expr {
                            kind: ExprKind::SimpleExpansion(var_name.to_string()),
                            span: Span::from_node(node),
                        });
                        i += close + 1;
                        literal_start = i;
                        continue;
                    }
                } else if i + 1 < bytes.len()
                    && (bytes[i + 1].is_ascii_alphanumeric() || bytes[i + 1] == b'_')
                {
                    // $var
                    let start = i + 1;
                    let mut end = start;
                    while end < bytes.len()
                        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                    {
                        end += 1;
                    }
                    parts.push(Expr {
                        kind: ExprKind::SimpleExpansion(text[start..end].to_string()),
                        span: Span::from_node(node),
                    });
                    i = end;
                    literal_start = i;
                    continue;
                }
            }
            i += 1;
        }
        if literal_start < bytes.len() {
            parts.push(Expr {
                kind: ExprKind::Literal(text[literal_start..].to_string()),
                span: Span::from_node(node),
            });
        }
        if parts.len() == 1 {
            parts.into_iter().next().unwrap().kind
        } else {
            ExprKind::Concat(parts)
        }
    } else {
        ExprKind::Literal(text)
    }
}

fn lower_ansi_c_string(node: Node, source: &[u8]) -> ExprKind {
    // $'...' — compile-time desugaring of escape sequences
    let text = node.utf8_text(source).unwrap_or("$''");
    let inner = &text[2..text.len().saturating_sub(1)]; // strip $' and '
    let mut result = String::new();
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'n' => {
                    result.push('\n');
                    i += 2;
                }
                b't' => {
                    result.push('\t');
                    i += 2;
                }
                b'r' => {
                    result.push('\r');
                    i += 2;
                }
                b'a' => {
                    result.push('\x07');
                    i += 2;
                }
                b'b' => {
                    result.push('\x08');
                    i += 2;
                }
                b'e' | b'E' => {
                    result.push('\x1b');
                    i += 2;
                }
                b'f' => {
                    result.push('\x0c');
                    i += 2;
                }
                b'v' => {
                    result.push('\x0b');
                    i += 2;
                }
                b'\\' => {
                    result.push('\\');
                    i += 2;
                }
                b'\'' => {
                    result.push('\'');
                    i += 2;
                }
                b'"' => {
                    result.push('"');
                    i += 2;
                }
                b'0' => {
                    // Octal: \0NNN
                    let mut val = 0u8;
                    let mut j = i + 2;
                    let end = (j + 3).min(bytes.len());
                    while j < end && bytes[j] >= b'0' && bytes[j] <= b'7' {
                        val = val * 8 + (bytes[j] - b'0');
                        j += 1;
                    }
                    result.push(val as char);
                    i = j;
                }
                b'x' => {
                    // Hex: \xNN
                    let mut val = 0u8;
                    let mut j = i + 2;
                    let end = (j + 2).min(bytes.len());
                    while j < end {
                        let d = match bytes[j] {
                            b'0'..=b'9' => bytes[j] - b'0',
                            b'a'..=b'f' => bytes[j] - b'a' + 10,
                            b'A'..=b'F' => bytes[j] - b'A' + 10,
                            _ => break,
                        };
                        val = val * 16 + d;
                        j += 1;
                    }
                    result.push(val as char);
                    i = j;
                }
                _ => {
                    result.push('\\');
                    result.push(bytes[i + 1] as char);
                    i += 2;
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    ExprKind::Literal(result)
}

/// Lower a [[ ]] or [ ] test_command to a Test expression.
pub(super) fn lower_test_command(node: Node, source: &[u8]) -> ExprKind {
    // Find the expression child (unary_expression, binary_expression, etc.)
    for child in node.children(&mut node.walk()) {
        if child.is_named() {
            match child.kind() {
                "unary_expression"
                | "binary_expression"
                | "negation_expression"
                | "parenthesized_expression" => {
                    return ExprKind::Test(lower_test_expr(child, source));
                }
                // Bare word/string in [[ ]] → implicit -n (non-empty test)
                "word" | "string" | "raw_string" | "string_content" | "simple_expansion"
                | "expansion" | "concatenation" => {
                    let expr = lower_expr(child, source);
                    return ExprKind::Test(TestExpr::Unary {
                        op: "-n".to_string(),
                        operand: Box::new(expr),
                    });
                }
                _ => {}
            }
        }
    }
    // Fallback: just capture the text
    let text = node.utf8_text(source).unwrap_or("??").to_string();
    ExprKind::Literal(text)
}

/// Lower a test expression node recursively.
fn lower_test_expr(node: Node, source: &[u8]) -> TestExpr {
    match node.kind() {
        "unary_expression" => {
            let op = node
                .child_by_field_name("operator")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("?")
                .to_string();
            if op == "!" {
                // [[ ! expr ]] — negation: operand is itself a test expression
                // The "!" is the operator (unnamed or field), inner expr is first named child after it
                let inner = node
                    .children(&mut node.walk())
                    .filter(|c| c.is_named() && c.kind() != "test_operator")
                    .find(|c| {
                        c.start_byte()
                            > node
                                .child_by_field_name("operator")
                                .map(|n| n.end_byte())
                                .unwrap_or(0)
                    })
                    .map(|c| lower_test_expr(c, source))
                    .unwrap_or_else(|| TestExpr::Unary {
                        op: "?".to_string(),
                        operand: Box::new(Expr {
                            kind: ExprKind::Literal("?".to_string()),
                            span: Span::from_node(node),
                        }),
                    });
                TestExpr::Not(Box::new(inner))
            } else {
                // -z, -n, -f, etc. — standard unary test
                let operand = node
                    .children(&mut node.walk())
                    .filter(|c| c.is_named())
                    .nth(1)
                    .map(|c| lower_expr(c, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    });
                TestExpr::Unary {
                    op,
                    operand: Box::new(operand),
                }
            }
        }
        "binary_expression" => {
            // Check if left side is a negation: [[ ! expr == expr ]] means !(expr == expr)
            let left_node = node.child_by_field_name("left");
            let is_negated_binary = left_node
                .map(|n| {
                    n.kind() == "unary_expression"
                        && n.child_by_field_name("operator")
                            .and_then(|o| o.utf8_text(source).ok())
                            .map(|o| o == "!")
                            .unwrap_or(false)
                })
                .unwrap_or(false);

            if is_negated_binary {
                // Extract the real left operand from inside the unary_expression
                let left_unary = left_node.unwrap();
                let real_left = left_unary
                    .children(&mut left_unary.walk())
                    .filter(|c| c.is_named() && c.kind() != "test_operator")
                    .find(|c| {
                        c.start_byte()
                            > left_unary
                                .child_by_field_name("operator")
                                .map(|n| n.end_byte())
                                .unwrap_or(0)
                    })
                    .map(|c| lower_expr(c, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(left_unary),
                    });
                let op = node
                    .child_by_field_name("operator")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("?")
                    .to_string();
                let right = node
                    .child_by_field_name("right")
                    .map(|n| lower_expr(n, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    });
                let inner = TestExpr::Binary {
                    left: Box::new(real_left),
                    op,
                    right: Box::new(right),
                };
                TestExpr::Not(Box::new(inner))
            } else {
                let left = left_node
                    .map(|n| lower_expr(n, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    });
                let op = node
                    .child_by_field_name("operator")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("?")
                    .to_string();
                let right = node
                    .child_by_field_name("right")
                    .map(|n| lower_expr(n, source))
                    .unwrap_or_else(|| Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    });
                // Handle && and || as And/Or, not binary operators
                if op == "&&" {
                    let left_te = lower_test_expr(left_node.unwrap(), source);
                    let right_te = node
                        .child_by_field_name("right")
                        .map(|n| lower_test_expr(n, source))
                        .unwrap_or_else(|| TestExpr::Unary {
                            op: "?".to_string(),
                            operand: Box::new(Expr {
                                kind: ExprKind::Literal("?".to_string()),
                                span: Span::from_node(node),
                            }),
                        });
                    TestExpr::And(Box::new(left_te), Box::new(right_te))
                } else if op == "||" {
                    let left_te = lower_test_expr(left_node.unwrap(), source);
                    let right_te = node
                        .child_by_field_name("right")
                        .map(|n| lower_test_expr(n, source))
                        .unwrap_or_else(|| TestExpr::Unary {
                            op: "?".to_string(),
                            operand: Box::new(Expr {
                                kind: ExprKind::Literal("?".to_string()),
                                span: Span::from_node(node),
                            }),
                        });
                    TestExpr::Or(Box::new(left_te), Box::new(right_te))
                } else {
                    TestExpr::Binary {
                        left: Box::new(left),
                        op,
                        right: Box::new(right),
                    }
                }
            }
        }
        "negation_expression" => {
            // [[ ! expr ]] → Not(inner)
            let inner = node
                .children(&mut node.walk())
                .find(|c| c.is_named() && c.kind() != "!")
                .map(|c| lower_test_expr(c, source))
                .unwrap_or_else(|| TestExpr::Unary {
                    op: "?".to_string(),
                    operand: Box::new(Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    }),
                });
            TestExpr::Not(Box::new(inner))
        }
        "parenthesized_expression" => {
            let inner = node
                .children(&mut node.walk())
                .find(|c| c.is_named())
                .map(|c| lower_test_expr(c, source))
                .unwrap_or_else(|| TestExpr::Unary {
                    op: "?".to_string(),
                    operand: Box::new(Expr {
                        kind: ExprKind::Literal("?".to_string()),
                        span: Span::from_node(node),
                    }),
                });
            TestExpr::Paren(Box::new(inner))
        }
        _ => {
            // Fallback — wrap as unary with the full text
            let text = node.utf8_text(source).unwrap_or("?").to_string();
            TestExpr::Unary {
                op: "?".to_string(),
                operand: Box::new(Expr {
                    kind: ExprKind::Literal(text),
                    span: Span::from_node(node),
                }),
            }
        }
    }
}

/// Process bash double-quote escape sequences: \" → ", \\ → \, \$ → $, \` → `, \newline → ""
fn unescape_double_quoted(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'"' | b'\\' | b'$' | b'`' => {
                    result.push(bytes[i + 1] as char);
                    i += 2;
                }
                b'\n' => {
                    // line continuation — skip both
                    i += 2;
                }
                _ => {
                    // Not a recognized escape — keep the backslash
                    result.push('\\');
                    i += 1;
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

pub(super) fn lower_string(node: Node, source: &[u8]) -> ExprKind {
    let mut parts = Vec::new();
    let mut last_end;

    // First pass: find the content range (inside the quotes)
    let mut content_start = node.start_byte();
    let mut content_end = node.end_byte();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() && child.kind() == "\"" {
            if child.start_byte() == node.start_byte() {
                content_start = child.end_byte();
            } else {
                // Closing quote: use end_byte - 1 to skip only the actual " char.
                // tree-sitter may include preceding whitespace in the quote token.
                content_end = child.end_byte() - 1;
            }
        }
    }
    last_end = content_start;

    // Second pass: collect named children and capture gaps as literals
    let mut cursor2 = node.walk();
    for child in node.children(&mut cursor2) {
        if !child.is_named() {
            continue;
        }

        // For simple_expansion inside strings, tree-sitter may include
        // leading whitespace in the node's range.  Detect and emit prefix.
        let child_start = child.start_byte();
        if child.kind() == "simple_expansion" || child.kind() == "expansion" {
            if let Some(first) = child.child(0) {
                let token_text = first.utf8_text(source).unwrap_or("");
                let trimmed = token_text.trim_start();
                let prefix_len = token_text.len() - trimmed.len();
                if prefix_len > 0 {
                    let prefix = &token_text[..prefix_len];
                    parts.push(Expr {
                        kind: ExprKind::Literal(prefix.to_string()),
                        span: Span { line: 0, col: 0 },
                    });
                }
            }
        }

        // Capture any gap between previous part and this child
        if child_start > last_end {
            let gap = std::str::from_utf8(&source[last_end..child_start]).unwrap_or("");
            if !gap.is_empty() {
                parts.push(Expr {
                    kind: ExprKind::Literal(unescape_double_quoted(gap)),
                    span: Span { line: 0, col: 0 },
                });
            }
        }
        parts.push(lower_expr(child, source));
        last_end = child.end_byte();
    }

    // Capture trailing text after last named child
    if last_end < content_end {
        let gap = std::str::from_utf8(&source[last_end..content_end]).unwrap_or("");
        if !gap.is_empty() {
            parts.push(Expr {
                kind: ExprKind::Literal(unescape_double_quoted(gap)),
                span: Span { line: 0, col: 0 },
            });
        }
    }

    if parts.is_empty() {
        ExprKind::DoubleQuoted(vec![])
    } else {
        // Sash-native: $arr[i] inside strings (DD#41)
        // CST gives: SimpleExpansion("arr") + Literal("[0] rest")
        // Rewrite to: Subscript{arr, 0} + Literal(" rest")
        let mut rewritten = Vec::new();
        let mut i = 0;
        while i < parts.len() {
            if let ExprKind::SimpleExpansion(ref name) = parts[i].kind {
                if i + 1 < parts.len() {
                    if let ExprKind::Literal(ref lit) = parts[i + 1].kind {
                        if lit.starts_with('[') {
                            if let Some(close) = lit.find(']') {
                                let idx = &lit[1..close];
                                let rest = &lit[close + 1..];
                                if idx == "@" || idx == "*" {
                                    rewritten.push(Expr {
                                        kind: ExprKind::ParameterExpansion(ParameterExpansion {
                                            name: name.clone(),
                                            operator: Some("arrvals".to_string()),
                                            operand: None,
                                        }),
                                        span: parts[i].span,
                                    });
                                } else {
                                    rewritten.push(Expr {
                                        kind: ExprKind::Subscript {
                                            name: name.clone(),
                                            index: Box::new(Expr {
                                                kind: ExprKind::Literal(idx.to_string()),
                                                span: parts[i].span,
                                            }),
                                        },
                                        span: parts[i].span,
                                    });
                                }
                                if !rest.is_empty() {
                                    rewritten.push(Expr {
                                        kind: ExprKind::Literal(rest.to_string()),
                                        span: parts[i + 1].span,
                                    });
                                }
                                i += 2;
                                continue;
                            }
                        }
                    }
                }
            }
            rewritten.push(parts[i].clone());
            i += 1;
        }
        ExprKind::DoubleQuoted(rewritten)
    }
}

/// Convert a bash glob pattern to a Plan 9 regex pattern.
/// Safe subset limits globs to * only; ? is also converted.
/// Character classes (e.g. [abc]) are rejected — caller should error.
/// Regex metacharacters in the pattern are escaped.
fn glob_to_regex(glob: &str) -> String {
    let mut re = String::with_capacity(glob.len() * 2);
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            '\\' => {
                // Escaped char — pass through literally (escaped in regex too)
                if let Some(next) = chars.next() {
                    re.push('\\');
                    re.push(next);
                }
            }
            // Escape regex metacharacters
            '.' | '+' | '(' | ')' | '|' | '^' | '$' => {
                re.push('\\');
                re.push(c);
            }
            _ => re.push(c),
        }
    }
    re
}

fn expr_to_literal_string(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(s) => s.clone(),
        ExprKind::SingleQuoted(s) => s.clone(),
        _ => {
            // For non-literal expressions, return the debug representation
            // This is a fallback — most operands in ${var/pat/rep} are literals
            format!("{:?}", expr.kind)
        }
    }
}

fn lower_parameter_expansion(node: Node, source: &[u8]) -> ExprKind {
    let mut name = String::new();
    let mut operator = None;
    let mut operand = None;
    let mut is_subscript = false;
    let mut subscript_index: Option<Expr> = None;

    // Pre-scan for ${var:s/regex/replacement/flags} (DD#39 sed expression)
    // The external scanner consumes :s/ as a hidden token, so we detect it
    // by checking source bytes between variable_name and first regex child.
    {
        let mut var_end = 0usize;
        let mut first_regex_start = 0usize;
        let mut regex_texts = Vec::new();
        let mut flags_text = String::new();
        let mut found_var = false;
        for child in node.children(&mut node.walk()) {
            match child.kind() {
                "variable_name" if !found_var => {
                    name = child.utf8_text(source).unwrap_or("").to_string();
                    var_end = child.end_byte();
                    found_var = true;
                }
                "regex" if found_var => {
                    if first_regex_start == 0 {
                        first_regex_start = child.start_byte();
                    }
                    regex_texts.push(child.utf8_text(source).unwrap_or("").to_string());
                }
                "word" if found_var && !regex_texts.is_empty() => {
                    // flags node after the last /
                    flags_text = child.utf8_text(source).unwrap_or("").to_string();
                }
                _ => {}
            }
        }
        if found_var && first_regex_start > var_end {
            let gap = std::str::from_utf8(&source[var_end..first_regex_start]).unwrap_or("");
            if gap.starts_with(":s/") {
                // It's a sed expression: ${var:s/pattern/replacement/flags}
                let pattern = regex_texts.first().cloned().unwrap_or_default();
                let replacement = regex_texts.get(1).cloned().unwrap_or_default();
                let is_global = flags_text.contains('g');
                // Build operand as pattern\0replacement (null-separated, like ${var/pat/rep})
                let combined = format!("{}\0{}", pattern, replacement);
                let op = if is_global { "//" } else { "/" };
                return ExprKind::ParameterExpansion(ParameterExpansion {
                    name,
                    operator: Some(op.to_string()),
                    operand: Some(Box::new(Expr {
                        kind: ExprKind::Literal(combined),
                        span: Span::from_node(node),
                    })),
                });
            }
        }
        // Reset name for normal path
        name = String::new();
    }

    // Check for substring pattern: ${var:offset:length} or ${var:offset}
    // tree-sitter gives: variable_name, ":", number, ":", number
    let mut colon_count = 0;
    let mut substring_parts: Vec<String> = Vec::new();
    let mut has_named_op = false;

    // Pre-scan for substring pattern
    let mut negate_next = false;
    for child in node.children(&mut node.walk()) {
        match child.kind() {
            ":" if !name.is_empty() && !has_named_op => {
                colon_count += 1;
            }
            "-" if colon_count > 0 && !has_named_op => {
                negate_next = true;
            }
            "number" if colon_count > 0 && !has_named_op => {
                let mut val = child.utf8_text(source).unwrap_or("0").to_string();
                if negate_next {
                    val = format!("-{}", val);
                    negate_next = false;
                }
                substring_parts.push(val);
            }
            "parenthesized_expression" if colon_count > 0 && !has_named_op => {
                // ${var:(-5)} — negative offset in parens
                let inner = child.utf8_text(source).unwrap_or("0");
                // Strip parens, extract number
                let inner = inner.trim_start_matches('(').trim_end_matches(')').trim();
                substring_parts.push(inner.to_string());
            }
            _ if child.kind().starts_with(':') && child.kind().len() > 1 => {
                has_named_op = true; // :- :+ := :? — not substring
            }
            "variable_name" => {
                name = child.utf8_text(source).unwrap_or("").to_string();
            }
            _ => {}
        }
    }

    if colon_count >= 1 && !substring_parts.is_empty() && !has_named_op {
        // ${var:offset} or ${var:offset:length}
        let offset = substring_parts[0].clone();
        let length = if substring_parts.len() > 1 {
            substring_parts[1].clone()
        } else {
            String::new()
        };
        let operand_str = if length.is_empty() {
            offset
        } else {
            format!("{}:{}", offset, length)
        };
        return ExprKind::ParameterExpansion(ParameterExpansion {
            name,
            operator: Some("substring".to_string()),
            operand: Some(Box::new(Expr {
                kind: ExprKind::Literal(operand_str),
                span: Span::from_node(node),
            })),
        });
    }

    // Reset for normal parsing
    let mut name = String::new();

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "variable_name" => {
                name = child.utf8_text(source).unwrap_or("").to_string();
            }
            "#" | "##" if name.is_empty() => {
                // ${#var} — string length (# before variable_name)
                operator = Some("strlen".to_string());
            }
            "!" if name.is_empty() => {
                // ${!arr[@]} — indirect expansion / all keys
                operator = Some("!".to_string());
            }
            _ if operator.is_none()
                && (child.kind().starts_with(':')
                    || child.kind() == "%"
                    || child.kind() == "%%"
                    || child.kind() == "#"
                    || child.kind() == "##"
                    || child.kind() == "/"
                    || child.kind() == "//"
                    || child.kind() == "/#"
                    || child.kind() == "/%"
                    || child.kind() == "^"
                    || child.kind() == "^^"
                    || child.kind() == ","
                    || child.kind() == ",,") =>
            {
                operator = Some(child.kind().to_string());
            }
            "subscript" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    name = name_node.utf8_text(source).unwrap_or("").to_string();
                }
                if let Some(idx) = child.child_by_field_name("index") {
                    let idx_expr = lower_expr(idx, source);
                    is_subscript = true;
                    subscript_index = Some(idx_expr);
                } else {
                    // No index field → ${arr[@]} or ${#arr[@]}
                    is_subscript = true;
                }
            }
            "word"
            | "string"
            | "raw_string"
            | "regex"
            | "simple_expansion"
            | "expansion"
            | "command_substitution"
            | "concatenation" => {
                if operator.is_some() {
                    let op_str = operator.as_deref().unwrap_or("");
                    if (op_str == "/" || op_str == "//" || op_str == "/#" || op_str == "/%")
                        && operand.is_none()
                        && child.kind() == "regex"
                    {
                        // First part after /: this is the pattern — convert glob→regex now
                        let pat_glob = expr_to_literal_string(&lower_expr(child, source));
                        let mut pat_regex = glob_to_regex(&pat_glob);
                        if op_str == "/#" {
                            pat_regex = format!("^{}", pat_regex);
                        }
                        if op_str == "/%" {
                            pat_regex = format!("{}$", pat_regex);
                        }
                        operand = Some(Box::new(Expr {
                            kind: ExprKind::Literal(pat_regex),
                            span: Span::from_node(child),
                        }));
                    } else if (op_str == "/" || op_str == "//" || op_str == "/#" || op_str == "/%")
                        && operand.is_some()
                    {
                        // Second part after /: this is the replacement
                        // Pattern already converted to regex above
                        let pat_expr = operand.take().unwrap();
                        let pat_regex = expr_to_literal_string(&pat_expr);
                        let rep_text = expr_to_literal_string(&lower_expr(child, source));
                        let combined = format!("{}\0{}", pat_regex, rep_text);
                        operand = Some(Box::new(Expr {
                            kind: ExprKind::Literal(combined),
                            span: Span::from_node(child),
                        }));
                    } else {
                        // Accumulate operand — multiple children after operator
                        if let Some(prev) = operand.take() {
                            let prev_str = expr_to_literal_string(&prev);
                            let cur_str = expr_to_literal_string(&lower_expr(child, source));
                            operand = Some(Box::new(Expr {
                                kind: ExprKind::Literal(format!("{}{}", prev_str, cur_str)),
                                span: Span::from_node(child),
                            }));
                        } else {
                            operand = Some(Box::new(lower_expr(child, source)));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if is_subscript {
        if let Some(ref op) = operator {
            if op == "strlen" {
                // ${#arr[@]} or ${#arr[*]} — array length
                // ${#arr[N]} — string length of element N
                let is_all = subscript_index.as_ref().is_some_and(
                    |idx| matches!(&idx.kind, ExprKind::Literal(s) if s == "@" || s == "*"),
                );
                if is_all || subscript_index.is_none() {
                    return ExprKind::ParameterExpansion(ParameterExpansion {
                        name,
                        operator: Some("arrlen".to_string()),
                        operand: None,
                    });
                } else {
                    // ${#arr[N]} — get element N, then strlen
                    return ExprKind::ParameterExpansion(ParameterExpansion {
                        name,
                        operator: Some("strlen_elem".to_string()),
                        operand: subscript_index.map(Box::new),
                    });
                }
            }
        }
        if let Some(idx) = subscript_index {
            // ${arr[@]} or ${arr[*]} — all elements
            if matches!(&idx.kind, ExprKind::Literal(s) if s == "@" || s == "*") {
                if operator.as_deref() == Some("!") {
                    return ExprKind::ParameterExpansion(ParameterExpansion {
                        name,
                        operator: Some("arrkeys".to_string()),
                        operand: None,
                    });
                }
                // DD#33: per-element operations
                if let Some(ref op) = operator {
                    if matches!(
                        op.as_str(),
                        "/" | "//" | "#" | "##" | "%" | "%%" | "^" | "^^" | "," | ",,"
                    ) {
                        // Encode as arrvals_OP:operand for codegen to iterate
                        return ExprKind::ParameterExpansion(ParameterExpansion {
                            name,
                            operator: Some(format!("arrvals_{}", op)),
                            operand,
                        });
                    }
                }
                return ExprKind::ParameterExpansion(ParameterExpansion {
                    name,
                    operator: Some("arrvals".to_string()),
                    operand: None,
                });
            }
            return ExprKind::Subscript {
                name,
                index: Box::new(idx),
            };
        }
        // ${arr[@]} or ${!arr[@]} — all elements/keys
        // For now, return as a special parameter expansion
        if operator.as_deref() == Some("!") {
            return ExprKind::ParameterExpansion(ParameterExpansion {
                name,
                operator: Some("arrkeys".to_string()),
                operand: None,
            });
        }
        return ExprKind::ParameterExpansion(ParameterExpansion {
            name,
            operator: Some("arrvals".to_string()),
            operand: None,
        });
    }

    ExprKind::ParameterExpansion(ParameterExpansion {
        name,
        operator,
        operand,
    })
}
