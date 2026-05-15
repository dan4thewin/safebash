use crate::diagnostic::Diagnostic;
use crate::rule::Rule;
use tree_sitter::Node;

/// SB5004: process substitution <(...) / >(...) is not allowed.
pub struct ProcessSubstitution;

impl Rule for ProcessSubstitution {
    fn node_kinds(&self) -> &'static [&'static str] {
        &["process_substitution"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let text = node.utf8_text(source).unwrap_or("?");
        vec![Diagnostic::error(
            node,
            "SB5004",
            format!(
                "process substitution {} is not allowed in the safe subset",
                text
            ),
        )]
    }
}
