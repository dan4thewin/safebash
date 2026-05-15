use crate::diagnostic::Diagnostic;
use crate::rule::Rule;
use tree_sitter::Node;

/// SB5005: extglob patterns @(), ?(), +(), *(), !() are not allowed.
pub struct ExtglobRejected;

impl Rule for ExtglobRejected {
    fn node_kinds(&self) -> &'static [&'static str] {
        &["extglob_pattern"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let text = node.utf8_text(source).unwrap_or("?");
        // Bare * in case patterns also parses as extglob_pattern — allow it
        if text == "*" {
            return vec![];
        }
        vec![Diagnostic::error(
            node,
            "SB5005",
            format!(
                "extglob pattern {} is not supported in sash — use regex operators instead",
                text
            ),
        )]
    }
}
