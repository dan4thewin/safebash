//! Helper utilities shared across lowering functions.

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
