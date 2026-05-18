//! Function and redirected statement lowering.

use super::expressions::lower_expr;
use super::helpers::child_field_name;
use super::lower_body;
use super::statements::lower_statement;
use crate::ir::*;
use tree_sitter::Node;

pub(super) fn lower_function(node: Node, source: &[u8]) -> Function {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("?")
        .to_string();

    let return_type = if node.child_by_field_name("type").is_some() {
        VarType::Int
    } else {
        VarType::String
    };

    let body = node
        .child_by_field_name("body")
        .map(|b| lower_body(b, source))
        .unwrap_or_default();

    Function {
        name,
        return_type,
        body,
        is_local: false,
    }
}

// ── Redirected statement lowering ─────────────────────────────

pub(super) fn lower_redirected(node: Node, source: &[u8]) -> Redirected {
    let mut body_stmt = None;
    let mut redirects = Vec::new();

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "file_redirect" | "heredoc_redirect" | "herestring_redirect" => {
                let redir = lower_redirect(child, source);
                // Check if this is &> (tree-sitter gives &> as the operator)
                let mut is_ampersand_redirect = false;
                for gc in child.children(&mut child.walk()) {
                    if gc.kind() == "&>" {
                        is_ampersand_redirect = true;
                        break;
                    }
                }
                if is_ampersand_redirect {
                    // &>file desugars to >file 2>&1
                    let span = redir.target.span;
                    redirects.push(Redirect {
                        fd: Some(1),
                        op: RedirectOp::Write,
                        target: redir.target,
                    });
                    redirects.push(Redirect {
                        fd: Some(2),
                        op: RedirectOp::DupOutput,
                        target: Expr {
                            kind: ExprKind::Literal("1".to_string()),
                            span,
                        },
                    });
                } else {
                    redirects.push(redir);
                }
            }
            _ => {
                if body_stmt.is_none() {
                    if let Some(s) = lower_statement(child, source) {
                        body_stmt = Some(s);
                    }
                }
            }
        }
    }

    let body = body_stmt.unwrap_or_else(|| Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("?".to_string()),
                span: Span::from_node(node),
            },
            args: vec![],
        }),
        span: Span::from_node(node),
    });

    Redirected {
        body: Box::new(body),
        redirects,
    }
}

pub(super) fn lower_redirect(node: Node, source: &[u8]) -> Redirect {
    let mut fd = None;
    let mut op = RedirectOp::Write;
    let mut strip_tabs = false;
    let mut target = Expr {
        kind: ExprKind::Literal(String::new()),
        span: Span::from_node(node),
    };

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "file_descriptor" => {
                if let Ok(text) = child.utf8_text(source) {
                    fd = text.parse::<u32>().ok();
                }
            }
            ">" => op = RedirectOp::Write,
            ">>" => op = RedirectOp::Append,
            "<" => op = RedirectOp::Read,
            ">&" => op = RedirectOp::DupOutput,
            "<&" => op = RedirectOp::DupInput,
            "<>" => op = RedirectOp::ReadWrite,
            "<<" | "<<-" => {
                op = RedirectOp::HereDoc;
                if child.utf8_text(source).unwrap_or("") == "<<-" {
                    strip_tabs = true;
                }
            }
            "<<<" => op = RedirectOp::HereString,
            "heredoc_body" => {
                // Heredoc body — walk children for variable expansion
                // If it has named children (simple_expansion, etc.), treat like double-quoted
                if child.named_child_count() > 0 {
                    // Build parts like lower_string does
                    let mut parts = Vec::new();
                    let content_start = child.start_byte();
                    let content_end = child.end_byte();
                    let mut last_end = content_start;
                    for gc in child.children(&mut child.walk()) {
                        if !gc.is_named() {
                            continue;
                        }
                        let gc_start = gc.start_byte();
                        if gc_start > last_end {
                            let gap =
                                std::str::from_utf8(&source[last_end..gc_start]).unwrap_or("");
                            if !gap.is_empty() {
                                parts.push(Expr {
                                    kind: ExprKind::Literal(gap.to_string()),
                                    span: Span::from_node(gc),
                                });
                            }
                        }
                        parts.push(lower_expr(gc, source));
                        last_end = gc.end_byte();
                    }
                    if last_end < content_end {
                        let gap = std::str::from_utf8(&source[last_end..content_end]).unwrap_or("");
                        if !gap.is_empty() {
                            parts.push(Expr {
                                kind: ExprKind::Literal(gap.to_string()),
                                span: Span::from_node(child),
                            });
                        }
                    }
                    target = Expr {
                        kind: if parts.len() == 1 {
                            parts.into_iter().next().unwrap().kind
                        } else {
                            ExprKind::DoubleQuoted(parts)
                        },
                        span: Span::from_node(child),
                    };
                } else {
                    // No expansions — literal text
                    let mut body_text = child.utf8_text(source).unwrap_or("").to_string();
                    if strip_tabs {
                        body_text = body_text
                            .lines()
                            .map(|l| l.trim_start_matches('\t'))
                            .collect::<Vec<_>>()
                            .join("\n");
                        if child.utf8_text(source).unwrap_or("").ends_with('\n') {
                            body_text.push('\n');
                        }
                    }
                    target = Expr {
                        kind: ExprKind::Literal(body_text),
                        span: Span::from_node(child),
                    };
                }
            }
            "string" | "raw_string" | "word" if op == RedirectOp::HereString => {
                // Here-string: <<< "text" — bash adds trailing newline
                let mut text = match child.kind() {
                    "string" | "raw_string" => {
                        // Strip quotes
                        let raw = child.utf8_text(source).unwrap_or("");
                        if (raw.starts_with('"') && raw.ends_with('"'))
                            || (raw.starts_with('\'') && raw.ends_with('\''))
                        {
                            raw[1..raw.len() - 1].to_string()
                        } else {
                            raw.to_string()
                        }
                    }
                    _ => child.utf8_text(source).unwrap_or("").to_string(),
                };
                text.push('\n'); // bash adds trailing newline to here-strings
                target = Expr {
                    kind: ExprKind::Literal(text),
                    span: Span::from_node(child),
                };
            }
            _ => {
                if let Some(field) = child_field_name(child, node) {
                    if field == "destination" {
                        target = lower_expr(child, source);
                    }
                }
            }
        }
    }

    Redirect { fd, op, target }
}
