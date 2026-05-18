//! Helper utilities shared across lowering functions.

use crate::ir::*;
use tree_sitter::Node;

pub(super) fn child_field_name(child: Node, parent: Node) -> Option<&'static str> {
    let mut cursor = parent.walk();
    cursor.goto_first_child();
    loop {
        if cursor.node() == child {
            return cursor.field_name();
        }
        if !cursor.goto_next_sibling() {
            return None;
        }
    }
}

/// Check if a node has any ERROR descendant (used for DD#58 sexpr fallback).
pub(super) fn has_error_descendant(node: Node) -> bool {
    let mut cursor = node.walk();
    fn walk(node: Node, cursor: &mut tree_sitter::TreeCursor) -> bool {
        if node.kind() == "ERROR" {
            return true;
        }
        if cursor.goto_first_child() {
            loop {
                if walk(cursor.node(), cursor) {
                    return true;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
        false
    }
    walk(node, &mut cursor)
}

/// Get the text of a named field, or "?" if missing.
pub(super) fn field_text(node: Node, field: &str, source: &[u8]) -> String {
    node.child_by_field_name(field)
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("?")
        .to_string()
}

/// Get a named field as a lowered Expr, or a placeholder "?" literal.
pub(super) fn field_expr(node: Node, field: &str, source: &[u8]) -> Expr {
    node.child_by_field_name(field)
        .map(|n| super::expressions::lower_expr(n, source))
        .unwrap_or_else(|| Expr {
            kind: ExprKind::Literal("?".to_string()),
            span: Span::from_node(node),
        })
}

/// Get the first named child after the operator field.
pub(super) fn operand_after_operator<'a>(node: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    let op_end = node
        .child_by_field_name("operator")
        .map(|n| n.end_byte())
        .unwrap_or(0);
    node.children(&mut node.walk())
        .filter(|c| c.is_named() && c.kind() != "test_operator")
        .find(|c| c.start_byte() > op_end)
}

/// Placeholder Expr for missing nodes.
pub(super) fn placeholder_expr(node: Node) -> Expr {
    Expr {
        kind: ExprKind::Literal("?".to_string()),
        span: Span::from_node(node),
    }
}
