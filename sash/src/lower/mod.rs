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
    Program {
        statements: stmts,
        end_block: None,
    }
}

/// Lower a body (sequence of statements) from a CST node.
pub fn lower_body(node: Node, source: &[u8]) -> Vec<Statement> {
    let mut stmts = Vec::new();
    let children: Vec<Node> = node.children(&mut node.walk()).collect();
    let mut i = 0;
    while i < children.len() {
        let child = children[i];
        if child.kind() == "comment" || child.kind() == "\n" {
            i += 1;
            continue;
        }
        // Check if next sibling is "&" (background operator)
        let is_background = i + 1 < children.len() && children[i + 1].kind() == "&";
        if let Some(s) = statements::lower_statement(child, source) {
            if is_background {
                // Wrap in a List with Background op
                let bg = Statement {
                    kind: StmtKind::List(List {
                        op: ListOp::Background,
                        left: Box::new(s),
                        right: Box::new(Statement {
                            kind: StmtKind::Command(Command {
                                name: Expr {
                                    kind: ExprKind::Literal("true".to_string()),
                                    span: Span::from_node(child),
                                },
                                args: vec![],
                            }),
                            span: Span::from_node(child),
                        }),
                    }),
                    span: Span::from_node(child),
                };
                stmts.push(bg);
                i += 2; // skip the "&"
            } else {
                stmts.push(s);
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    stmts
}
