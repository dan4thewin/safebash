use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB4014: tree-sitter parse error — source contains syntax that
/// tree-sitter couldn't parse. This catches malformed input before
/// lowering, where ERROR nodes would be silently skipped.
pub struct ParseError;

impl Rule for ParseError {

    fn node_kinds(&self) -> &'static [&'static str] {
        &["ERROR"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        // DD#58: Suppress parse errors inside arithmetic_expansion — sexpr
        // syntax like (add 3 4) produces ERROR nodes in tree-sitter but is
        // valid in sash's arithmetic context.
        if is_inside_arithmetic(node) {
            return vec![];
        }
        let text = node.utf8_text(source).unwrap_or("").trim();
        let snippet = if text.len() > 40 {
            format!("{}...", &text[..40])
        } else {
            text.to_string()
        };
        vec![Diagnostic::error(
            node,
            "SB4014",
            format!("parse error: `{}` — source could not be parsed", snippet),
        )]
    }
}

/// Check if a node is inside an arithmetic_expansion or arithmetic_command.
fn is_inside_arithmetic(node: Node) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "arithmetic_expansion" || parent.kind() == "arithmetic_command" {
            return true;
        }
        current = parent;
    }
    false
}
