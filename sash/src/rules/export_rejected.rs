use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5001: export is not allowed in the safe subset.
///
/// The safe subset has no `export` keyword. Environment variables are
/// passed to commands inline: `FOO=bar cmd`. `declare -x` is also
/// rejected (it's a synonym for export).
///
/// See DD#45 amendment.
pub struct ExportRejected;

impl Rule for ExportRejected {

    fn node_kinds(&self) -> &'static [&'static str] {
        &["declaration_command"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let Some(first) = node.child(0) else {
            return vec![];
        };
        let keyword = first.utf8_text(source).unwrap_or("");

        if keyword == "export" {
            return vec![Diagnostic::error(
                node,
                "SB5001",
                "'export' is not allowed — use FOO=bar before a command instead".to_string(),
            )];
        }

        // Check for `declare -x` or combined flags containing x
        if matches!(keyword, "declare" | "typeset") {
            for i in 1..node.child_count() {
                let child = node.child(i).unwrap();
                if child.kind() == "word" {
                    let text = child.utf8_text(source).unwrap_or("");
                    if text.starts_with('-') && text.contains('x') {
                        return vec![Diagnostic::error(
                            node,
                            "SB5001",
                            "'declare -x' is not allowed — use FOO=bar before a command instead".to_string(),
                        )];
                    }
                }
            }
        }

        vec![]
    }
}
