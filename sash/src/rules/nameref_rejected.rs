use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5003: declare -n / local -n / typeset -n (nameref) is not allowed.
pub struct NamerefRejected;

impl Rule for NamerefRejected {

    fn node_kinds(&self) -> &'static [&'static str] {
        &["declaration_command"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "word" {
                let text = child.utf8_text(source).unwrap_or("");
                // Check for -n flag (standalone or combined like -rn, -nr)
                if text.starts_with('-') && text.contains('n') {
                    return vec![Diagnostic::error(
                        node,
                        "SB5003",
                        "nameref (declare -n / local -n) is not allowed in the safe subset".to_string(),
                    )];
                }
            }
        }
        vec![]
    }
}
