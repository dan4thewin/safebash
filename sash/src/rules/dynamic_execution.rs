use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB4001: eval is not allowed.
/// SB4002: source/. compiled to safe kvp loader (marker).
/// SB4004: variable-as-command deferred to SSA.
pub struct DynamicExecution;

impl Rule for DynamicExecution {

    fn node_kinds(&self) -> &'static [&'static str] {
        &["command"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let Some(name_node) = node.child_by_field_name("name") else {
            return vec![];
        };
        let Some(first_child) = name_node.child(0) else {
            return vec![];
        };

        if first_child.kind() != "word" {
            return vec![Diagnostic::deferred(
                name_node,
                "SB4004",
                format!(
                    "variable as command name: cannot verify command is a literal word (deferred to SSA), found {}",
                    first_child.kind()
                ),
            )];
        }

        let word = first_child.utf8_text(source).unwrap_or("");
        match word {
            "eval" => vec![Diagnostic::error(
                node,
                "SB4001",
                "eval is not allowed in the safe subset".to_string(),
            )],
            "source" | "." => vec![Diagnostic::error(
                node,
                "SB4002",
                format!("'source'/'.' is not allowed — use `sash-expand file | mapfile -m var` instead"),
            )],
            _ => vec![],
        }
    }
}
