use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5001: bash declaration keywords rejected in sash-native.
///
/// `export`, `declare`, `typeset`, `readonly` are bash syntax.
/// Sash uses: int, const, array, map, local.
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

        match keyword {
            "export" => {
                return vec![Diagnostic::error(
                    node,
                    "SB5001",
                    "'export' is not allowed — use FOO=bar before a command instead".to_string(),
                )];
            }
            "declare" | "typeset" => {
                return vec![Diagnostic::error(
                    node,
                    "SB5001",
                    format!("'{}' is bash syntax — use sash declarations: int, const, array, map", keyword),
                )];
            }
            "readonly" => {
                return vec![Diagnostic::error(
                    node,
                    "SB5001",
                    "'readonly' is bash syntax — use 'const' in sash".to_string(),
                )];
            }
            _ => {}
        }

        // DD#35: reject invalid declaration stacking (const array, const map)
        if keyword == "const" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let text = child.utf8_text(source).unwrap_or("");
                if text == "array" || text == "map" {
                    return vec![Diagnostic::error(
                        node,
                        "SB5001",
                        format!("'const {}' is not allowed \u{2014} collections are mutable", text),
                    )];
                }
            }
        }

        vec![]
    }
}
