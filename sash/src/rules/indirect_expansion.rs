use crate::diagnostic::Diagnostic;
use crate::rule::Rule;
use tree_sitter::Node;

/// SB4003: ${!var} indirect expansion is not allowed.
pub struct IndirectExpansion;

impl Rule for IndirectExpansion {
    fn node_kinds(&self) -> &'static [&'static str] {
        &["expansion"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "!" {
                let text = node.utf8_text(source).unwrap_or("?");
                return vec![Diagnostic::error(
                    node,
                    "SB4003",
                    format!(
                        "indirect expansion {} is not allowed in the safe subset",
                        text
                    ),
                )];
            }
        }
        vec![]
    }
}
