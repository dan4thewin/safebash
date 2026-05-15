//! Arithmetic expression and command lowering.
//!
//! Handles: (( expr )), C-style for loops, arithmetic expressions.

use super::helpers::has_error_descendant;
use super::lower_body;
use super::statements::lower_statement;
use crate::ir::*;
use tree_sitter::Node;

pub(super) fn lower_arithmetic_command(node: Node, source: &[u8]) -> Option<Statement> {
    let span = Span::from_node(node);
    // DD#58: If arithmetic command contains ERROR nodes (sexpr syntax),
    // fall back to raw text extraction.
    if has_error_descendant(node) {
        let text = node.utf8_text(source).unwrap_or("(( 0 ))");
        let inner = text
            .strip_prefix("((")
            .unwrap_or(text)
            .strip_suffix("))")
            .unwrap_or(text)
            .trim()
            .to_string();
        // Treat as a `let` command with the arithmetic expression
        return Some(Statement {
            kind: StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("let".to_string()),
                    span,
                },
                args: vec![Expr {
                    kind: ExprKind::Literal(inner),
                    span,
                }],
            }),
            span,
        });
    }
    // Check for comma operator: multiple named children means comma-separated expressions
    let named_count = node.named_child_count();
    if named_count > 1 {
        // Comma operator: (( a=1, b=2, c=a+b )) → Group of individual assignments
        let mut stmts = Vec::new();
        for i in 0..named_count {
            let child = node.named_child(i).unwrap();
            // Create a synthetic single-expression arithmetic command
            // by recursively lowering each sub-expression
            if let Some(stmt) = lower_single_arith_expr(child, source, span) {
                stmts.push(stmt);
            }
        }
        return Some(Statement {
            kind: StmtKind::Group(Group { body: stmts }),
            span,
        });
    }
    let expr_node = node.named_child(0)?;
    let kind = lower_single_arith_expr_kind(expr_node, source, span)?;
    Some(Statement { kind, span })
}

/// Lower a single arithmetic sub-expression (used by both single and comma forms)
fn lower_single_arith_expr(node: Node, source: &[u8], span: Span) -> Option<Statement> {
    let kind = lower_single_arith_expr_kind(node, source, span)?;
    Some(Statement { kind, span })
}

fn lower_single_arith_expr_kind(expr_node: Node, source: &[u8], span: Span) -> Option<StmtKind> {
    let kind = match expr_node.kind() {
        "postfix_expression" => {
            let var_node = expr_node.named_child(0)?;
            let var_name = var_node.utf8_text(source).ok()?.to_string();
            let op_text = expr_node.utf8_text(source).ok()?;
            let arith_op = if op_text.ends_with("++") { "+" } else { "-" };
            StmtKind::Assignment(Assignment {
                index: None,
                op: AssignOp::Set,
                name: var_name.clone(),
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(format!("{} {} 1", var_name, arith_op)),
                    span,
                },
            })
        }
        "binary_expression" => {
            let left_node = expr_node.child_by_field_name("left")?;
            let right_node = expr_node.child_by_field_name("right")?;
            let left_text = arith_node_to_string(left_node, source);
            let right_text = arith_node_to_string(right_node, source);
            // Find the operator — it's the unnamed child between left and right
            let mut operator = "=";
            for i in 0..expr_node.child_count() {
                let child = expr_node.child(i).unwrap();
                if !child.is_named() {
                    let op = child.utf8_text(source).unwrap_or("");
                    if !op.trim().is_empty() {
                        operator = match op.trim() {
                            "+=" => "+=",
                            "-=" => "-=",
                            "*=" => "*=",
                            "/=" => "/=",
                            "%=" => "%=",
                            "=" => "=",
                            ">" | "<" | ">=" | "<=" | "==" | "!=" => {
                                // Emit as raw ArithmeticExpansion text — the transform
                                // text parser handles comparisons with DD#58 function
                                // call awareness.
                                let full_text =
                                    expr_node.utf8_text(source).unwrap_or("0").to_string();
                                return Some(StmtKind::Command(Command {
                                    name: Expr {
                                        kind: ExprKind::Literal("let".to_string()),
                                        span,
                                    },
                                    args: vec![Expr {
                                        kind: ExprKind::Literal(full_text),
                                        span,
                                    }],
                                }));
                            }
                            other => other,
                        };
                        break;
                    }
                }
            }
            // It's an assignment: x = rhs, x += rhs, etc.
            let arith_expr = match operator {
                "+=" => format!("{} + {}", left_text, right_text),
                "-=" => format!("{} - {}", left_text, right_text),
                "*=" => format!("{} * {}", left_text, right_text),
                "/=" => format!("{} / {}", left_text, right_text),
                "%=" => format!("{} % {}", left_text, right_text),
                "=" => right_text.clone(),
                _ => format!("{} {} {}", left_text, operator, right_text),
            };
            StmtKind::Assignment(Assignment {
                index: None,
                op: AssignOp::Set,
                name: left_text,
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(arith_expr),
                    span,
                },
            })
        }
        "unary_expression" => {
            let var_node = expr_node.named_child(0)?;
            let var_name = var_node.utf8_text(source).ok()?.to_string();
            let op_text = expr_node.utf8_text(source).ok()?;
            if op_text.starts_with("++") || op_text.starts_with("--") {
                // ++x or --x
                let arith_op = if op_text.starts_with("++") { "+" } else { "-" };
                StmtKind::Assignment(Assignment {
                    index: None,
                    op: AssignOp::Set,
                    name: var_name.clone(),
                    value: Expr {
                        kind: ExprKind::ArithmeticExpansion(format!("{} {} 1", var_name, arith_op)),
                        span,
                    },
                })
            } else if op_text.starts_with('-')
                || op_text.starts_with('+')
                || op_text.starts_with('!')
                || op_text.starts_with('~')
            {
                // Unary -, +, !, ~ used as condition: (( -1 )), (( !x ))
                let text = op_text.trim().to_string();
                let test_var = ExprKind::ArithmeticExpansion(text);
                StmtKind::Command(Command {
                    name: Expr {
                        kind: ExprKind::Literal("[[".to_string()),
                        span,
                    },
                    args: vec![Expr {
                        kind: ExprKind::Test(TestExpr::Binary {
                            left: Box::new(Expr {
                                kind: test_var,
                                span,
                            }),
                            op: "-ne".to_string(),
                            right: Box::new(Expr {
                                kind: ExprKind::Literal("0".to_string()),
                                span,
                            }),
                        }),
                        span,
                    }],
                })
            } else {
                let arith_op = if op_text.starts_with("++") { "+" } else { "-" };
                StmtKind::Assignment(Assignment {
                    index: None,
                    op: AssignOp::Set,
                    name: var_name.clone(),
                    value: Expr {
                        kind: ExprKind::ArithmeticExpansion(format!("{} {} 1", var_name, arith_op)),
                        span,
                    },
                })
            }
        }
        "parenthesized_expression" => {
            // (( (expr) )) — recurse into the inner expression
            if let Some(inner) = expr_node.named_child(0) {
                return lower_single_arith_expr_kind(inner, source, span);
            }
            return None;
        }
        _ => {
            // Bare expression — could be a number (used as condition) or variable
            let text = expr_node.utf8_text(source).ok()?.trim().to_string();
            // Desugar (( expr )) to [[ expr -ne 0 ]] for use as a test
            let test_var = if text.chars().all(|c| c.is_ascii_digit())
                || (text.starts_with('-') && text[1..].chars().all(|c| c.is_ascii_digit()))
            {
                // Literal number: (( 0 )) → false, (( 5 )) → true, (( -1 )) → true
                ExprKind::Literal(text)
            } else {
                // Variable: (( x )) → [[ $x -ne 0 ]]
                ExprKind::SimpleExpansion(text)
            };
            StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("[[".to_string()),
                    span,
                },
                args: vec![Expr {
                    kind: ExprKind::Test(TestExpr::Binary {
                        left: Box::new(Expr {
                            kind: test_var,
                            span,
                        }),
                        op: "-ne".to_string(),
                        right: Box::new(Expr {
                            kind: ExprKind::Literal("0".to_string()),
                            span,
                        }),
                    }),
                    span,
                }],
            })
        }
    };
    Some(kind)
}

pub(super) fn lower_c_style_for(node: Node, source: &[u8]) -> Statement {
    let span = Span::from_node(node);

    // Initializer: variable_assignment(s) or expression(s)
    let mut init_stmts = Vec::new();
    if let Some(init) = node.child_by_field_name("initializer") {
        if init.kind() == "variable_assignment" {
            if let Some(s) = lower_statement(init, source) {
                init_stmts.push(s);
            }
        } else {
            // Expression like i=0 inside (( )) context — parse as arithmetic
            let text = init.utf8_text(source).unwrap_or("").to_string();
            if let Some(eq) = text.find('=') {
                let name = text[..eq].trim().to_string();
                let val = text[eq + 1..].trim().to_string();
                init_stmts.push(Statement {
                    kind: StmtKind::Assignment(Assignment {
                        index: None,
                        op: AssignOp::Set,
                        name,
                        value: Expr {
                            kind: ExprKind::Literal(val),
                            span,
                        },
                    }),
                    span,
                });
            }
        }
    }

    // Condition: binary_expression → desugar to [[ test ]]
    let condition = if let Some(cond) = node.child_by_field_name("condition") {
        let _cond_text = cond.utf8_text(source).unwrap_or("true");
        // Parse "i<5" style expressions
        let cond_stmt = if cond.kind() == "binary_expression" {
            let left = cond
                .child_by_field_name("left")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("0")
                .to_string();
            let right = cond
                .child_by_field_name("right")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("0")
                .to_string();
            // Find operator
            let mut op = "<";
            for i in 0..cond.child_count() {
                let child = cond.child(i).unwrap();
                if !child.is_named() {
                    let t = child.utf8_text(source).unwrap_or("");
                    if matches!(t, "<" | ">" | "<=" | ">=" | "==" | "!=") {
                        op = match t {
                            "<" => "-lt",
                            ">" => "-gt",
                            "<=" => "-le",
                            ">=" => "-ge",
                            "==" => "-eq",
                            "!=" => "-ne",
                            _ => "-lt",
                        };
                        break;
                    }
                }
            }
            Statement {
                kind: StmtKind::Command(Command {
                    name: Expr {
                        kind: ExprKind::Literal("[[".to_string()),
                        span,
                    },
                    args: vec![Expr {
                        kind: ExprKind::Test(TestExpr::Binary {
                            left: Box::new(Expr {
                                kind: ExprKind::SimpleExpansion(left),
                                span,
                            }),
                            op: op.to_string(),
                            right: Box::new(Expr {
                                kind: ExprKind::Literal(right),
                                span,
                            }),
                        }),
                        span,
                    }],
                }),
                span,
            }
        } else {
            // Fallback: treat as literal test
            Statement {
                kind: StmtKind::Command(Command {
                    name: Expr {
                        kind: ExprKind::Literal("true".to_string()),
                        span,
                    },
                    args: vec![],
                }),
                span,
            }
        };
        vec![cond_stmt]
    } else {
        vec![Statement {
            kind: StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("true".to_string()),
                    span,
                },
                args: vec![],
            }),
            span,
        }]
    };

    // Update: postfix_expression, binary_expression, etc.
    let mut update_stmts = Vec::new();
    if let Some(upd) = node.child_by_field_name("update") {
        // Create a fake arithmetic_command-like node and reuse the same logic
        let upd_text = upd.utf8_text(source).unwrap_or("").to_string();
        if upd.kind() == "postfix_expression" {
            let var_node = upd.named_child(0);
            let var_name = var_node
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("x")
                .to_string();
            let arith_op = if upd_text.ends_with("++") { "+" } else { "-" };
            update_stmts.push(Statement {
                kind: StmtKind::Assignment(Assignment {
                    index: None,
                    op: AssignOp::Set,
                    name: var_name.clone(),
                    value: Expr {
                        kind: ExprKind::ArithmeticExpansion(format!("{} {} 1", var_name, arith_op)),
                        span,
                    },
                }),
                span,
            });
        } else if upd.kind() == "binary_expression" {
            let left = upd
                .child_by_field_name("left")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("x")
                .to_string();
            let right = upd
                .child_by_field_name("right")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("1")
                .to_string();
            let mut operator = "+=";
            for i in 0..upd.child_count() {
                let child = upd.child(i).unwrap();
                if !child.is_named() {
                    let t = child.utf8_text(source).unwrap_or("").trim();
                    if !t.is_empty() {
                        operator = match t {
                            "+=" => "+=",
                            "-=" => "-=",
                            _ => t,
                        };
                        break;
                    }
                }
            }
            let arith = match operator {
                "+=" => format!("{} + {}", left, right),
                "-=" => format!("{} - {}", left, right),
                _ => format!("{} {} {}", left, operator, right),
            };
            update_stmts.push(Statement {
                kind: StmtKind::Assignment(Assignment {
                    index: None,
                    op: AssignOp::Set,
                    name: left,
                    value: Expr {
                        kind: ExprKind::ArithmeticExpansion(arith),
                        span,
                    },
                }),
                span,
            });
        }
    }

    // Body
    let body_node = node.child_by_field_name("body");
    let mut body = body_node.map(|n| lower_body(n, source)).unwrap_or_default();
    body.extend(update_stmts);

    // Desugar: init; while cond; do body; update; done
    // Return as a Group containing init stmts + while
    let while_stmt = Statement {
        kind: StmtKind::While(While {
            condition,
            body,
            until: false,
        }),
        span,
    };

    let mut group_body = init_stmts;
    group_body.push(while_stmt);
    Statement {
        kind: StmtKind::Group(Group { body: group_body }),
        span,
    }
}

/// Walk tree-sitter arithmetic AST nodes to produce an ExprKind.
/// Handles parenthesized_expression, binary_expression, unary_expression,
/// ternary_expression, number, variable_name, etc.
pub(super) fn lower_arith_expr_to_exprkind(node: Node, source: &[u8]) -> ExprKind {
    match node.kind() {
        "parenthesized_expression" => {
            // ( expr ) → recurse into inner
            if let Some(inner) = node.named_child(0) {
                lower_arith_expr_to_exprkind(inner, source)
            } else {
                ExprKind::ArithmeticExpansion("0".to_string())
            }
        }
        "binary_expression" => {
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            if let (Some(l), Some(r)) = (left, right) {
                // Find operator — unnamed child between left and right
                let mut op_str = "+";
                for i in 0..node.child_count() {
                    let child = node.child(i).unwrap();
                    if !child.is_named() {
                        let text = child.utf8_text(source).unwrap_or("").trim();
                        if !text.is_empty() && text != "(" && text != ")" {
                            op_str = child.utf8_text(source).unwrap_or("+").trim();
                            break;
                        }
                    }
                }
                let l_expr = arith_node_to_string(l, source);
                let r_expr = arith_node_to_string(r, source);
                ExprKind::ArithmeticExpansion(format!("{} {} {}", l_expr, op_str, r_expr))
            } else {
                ExprKind::ArithmeticExpansion(node.utf8_text(source).unwrap_or("0").to_string())
            }
        }
        "unary_expression" => {
            let text = node.utf8_text(source).unwrap_or("0").trim().to_string();
            ExprKind::ArithmeticExpansion(text)
        }
        "ternary_expression" => {
            let text = node.utf8_text(source).unwrap_or("0").trim().to_string();
            ExprKind::ArithmeticExpansion(text)
        }
        "number" | "variable_name" | "word" => {
            let text = node.utf8_text(source).unwrap_or("0").trim().to_string();
            let text = text.trim_start_matches('$').to_string();
            ExprKind::ArithmeticExpansion(text)
        }
        "simple_expansion" => {
            // Keep $ prefix so transform knows this is a variable, not a literal
            let text = node.utf8_text(source).unwrap_or("$0").trim().to_string();
            ExprKind::ArithmeticExpansion(text)
        }
        _ => {
            let text = node.utf8_text(source).unwrap_or("0").trim().to_string();
            ExprKind::ArithmeticExpansion(text)
        }
    }
}

/// Recursively convert an arithmetic AST node to a flat string representation
/// that the existing text-based transform pipeline can handle.
/// Parenthesized expressions are recursively expanded so the text parser
/// never sees raw parens.
fn arith_node_to_string(node: Node, source: &[u8]) -> String {
    match node.kind() {
        "parenthesized_expression" => {
            if let Some(inner) = node.named_child(0) {
                // Recurse — the parenthesized sub-expression becomes its own
                // ArithmeticExpansion string, which the transform will parse
                // as a nested expression
                format!("({})", arith_node_to_string(inner, source))
            } else {
                "0".to_string()
            }
        }
        "binary_expression" => {
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            if let (Some(l), Some(r)) = (left, right) {
                let mut op_str = "+";
                for i in 0..node.child_count() {
                    let child = node.child(i).unwrap();
                    if !child.is_named() {
                        let text = child.utf8_text(source).unwrap_or("").trim();
                        if !text.is_empty() && text != "(" && text != ")" {
                            op_str = child.utf8_text(source).unwrap_or("+").trim();
                            break;
                        }
                    }
                }
                format!(
                    "{} {} {}",
                    arith_node_to_string(l, source),
                    op_str,
                    arith_node_to_string(r, source)
                )
            } else {
                node.utf8_text(source).unwrap_or("0").to_string()
            }
        }
        "unary_expression" => {
            // e.g. -x, !x
            node.utf8_text(source).unwrap_or("0").trim().to_string()
        }
        "number" => node.utf8_text(source).unwrap_or("0").to_string(),
        "variable_name" | "word" => node
            .utf8_text(source)
            .unwrap_or("0")
            .trim_start_matches('$')
            .to_string(),
        "simple_expansion" => {
            // $var → keep $ so transform distinguishes from numeric literals
            // e.g. $1 stays "$1" (variable), bare 1 is literal number
            let text = node.utf8_text(source).unwrap_or("$0");
            if text.starts_with('$') {
                text.to_string()
            } else {
                format!("${}", text)
            }
        }
        _ => node.utf8_text(source).unwrap_or("0").trim().to_string(),
    }
}
