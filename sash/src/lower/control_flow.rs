//! Control flow lowering: if, while, for, case → IR.

use super::expressions::lower_expr;
use super::helpers::child_field_name;
use super::lower_body;
use super::statements::lower_statement;
use crate::ir::*;
use tree_sitter::Node;

pub(super) fn lower_if(node: Node, source: &[u8]) -> If {
    let mut condition = Vec::new();
    let mut then_body = Vec::new();
    let mut elif_clauses = Vec::new();
    let mut else_body = None;

    // States: gathering condition, then body, elif, else
    let mut in_then = false;

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "if" | "fi" | ";" => {}
            "then" => {
                in_then = true;
            }
            "elif_clause" => {
                elif_clauses.push(lower_elif(child, source));
            }
            "else_clause" => {
                else_body = Some(lower_body(child, source));
            }
            _ => {
                if !in_then {
                    // condition
                    if let Some(s) = lower_statement(child, source) {
                        condition.push(s);
                    }
                } else {
                    // then body
                    if let Some(s) = lower_statement(child, source) {
                        then_body.push(s);
                    }
                }
            }
        }
    }

    If {
        condition,
        then_body,
        elif_clauses,
        else_body,
    }
}

fn lower_elif(node: Node, source: &[u8]) -> ElifClause {
    let mut condition = Vec::new();
    let mut body = Vec::new();
    let mut in_then = false;

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "elif" | ";" => {}
            "then" => {
                in_then = true;
            }
            _ => {
                if !in_then {
                    if let Some(s) = lower_statement(child, source) {
                        condition.push(s);
                    }
                } else if let Some(s) = lower_statement(child, source) {
                    body.push(s);
                }
            }
        }
    }

    ElifClause { condition, body }
}

// ── While/Until lowering ──────────────────────────────────────

pub(super) fn lower_while(node: Node, source: &[u8]) -> While {
    let mut condition = Vec::new();
    let mut body = Vec::new();
    let mut until = false;

    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "while" => {
                until = false;
            }
            "until" => {
                until = true;
            }
            "do_group" => {
                body = lower_body(child, source);
            }
            ";" => {}
            _ => {
                // condition nodes (before do_group)
                if let Some(s) = lower_statement(child, source) {
                    condition.push(s);
                }
            }
        }
    }

    While {
        condition,
        body,
        until,
    }
}

// ── For lowering ──────────────────────────────────────────────

pub(super) fn lower_for(node: Node, source: &[u8]) -> For {
    let variable = node
        .child_by_field_name("variable")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("?")
        .to_string();

    // Collect the word list (field "value" children)
    let mut words = Vec::new();
    let mut body = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "do_group" => {
                body = lower_body(child, source);
            }
            "for" | "in" | ";" | "variable_name" => {}
            _ => {
                if let Some(field) = child_field_name(child, node) {
                    if field == "value" {
                        words.push(lower_expr(child, source));
                    }
                }
            }
        }
    }

    For {
        variable,
        words,
        body,
    }
}

// ── Case lowering ─────────────────────────────────────────────

pub(super) fn lower_case(node: Node, source: &[u8]) -> Case {
    let value = node
        .child_by_field_name("value")
        .map(|n| lower_expr(n, source))
        .unwrap_or_else(|| Expr {
            kind: ExprKind::Literal("?".to_string()),
            span: Span::from_node(node),
        });

    let mut items = Vec::new();
    for child in node.children(&mut node.walk()) {
        if child.kind() == "case_item" {
            items.push(lower_case_item(child, source));
        }
    }

    Case { value, items }
}

pub(super) fn lower_case_item(node: Node, source: &[u8]) -> CaseItem {
    let mut patterns = Vec::new();
    let mut body = Vec::new();
    let mut fallthrough = false;
    let mut resume = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            ")" | "|" => {}
            ";;" => {
                fallthrough = false;
                resume = false;
            }
            ";&" => {
                fallthrough = true;
                resume = false;
            }
            ";;&" => {
                fallthrough = false;
                resume = true;
            }
            _ => {
                if let Some(field) = child_field_name(child, node) {
                    match field {
                        "value" => {
                            patterns.push(lower_expr(child, source));
                        }
                        "termination" => {
                            let text = child.utf8_text(source).unwrap_or(";;");
                            fallthrough = text == ";&";
                            resume = text == ";;&";
                        }
                        _ => {
                            if let Some(s) = lower_statement(child, source) {
                                body.push(s);
                            }
                        }
                    }
                } else if let Some(s) = lower_statement(child, source) {
                    body.push(s);
                }
            }
        }
    }

    CaseItem {
        patterns,
        body,
        fallthrough,
        resume,
    }
}
