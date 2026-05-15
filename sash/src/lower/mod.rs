//! CST → IR lowering.
//!
//! SECURITY: This module converts tree-sitter CST into safebash IR.
//! Every CST node type must be explicitly handled — unknown node types
//! are ignored (producing no IR), which is safe because the validator
//! has already rejected anything not in the safe subset.
//!
//! A reviewer should verify:
//! 1. No IR node is synthesized from thin air (every IR construct traces to CST)
//! 2. String content from the source is faithfully preserved (no injection)
//! 3. Spans are propagated (enables accurate error reporting downstream)

mod arithmetic;
mod control_flow;
mod expressions;
mod functions;
mod helpers;
mod statements;

use crate::ir::*;
use tree_sitter::Node;

/// Lower an entire program (root node) into IR.
pub fn lower_program(root: Node, source: &[u8]) -> Program {
    let stmts = lower_body(root, source);
    Program { statements: stmts }
}

/// Lower a body (sequence of statements) from a CST node.
pub fn lower_body(node: Node, source: &[u8]) -> Vec<Statement> {
    let mut stmts = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "comment" || child.kind() == "\n" {
            continue;
        }
        if let Some(stmt) = statements::lower_statement(child, source) {
            stmts.push(stmt);
        }
    }
    stmts
}
