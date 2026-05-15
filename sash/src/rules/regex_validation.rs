use crate::diagnostic::Diagnostic;
use crate::rule::Rule;
use tree_sitter::Node;

/// SB5023: Validate literal regex patterns at compile time.
/// Catches invalid patterns (bad syntax, unsupported PCRE features)
/// before the binary is produced.
pub struct RegexValidation;

impl Rule for RegexValidation {
    fn node_kinds(&self) -> &'static [&'static str] {
        &["binary_expression"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        // Only care about =~ inside [[ ]]
        if !has_ancestor(node, "test_command") {
            return vec![];
        }

        // Check for =~ operator
        let op = node
            .child_by_field_name("operator")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if op != "=~" {
            return vec![];
        }

        // Get the right-hand side (pattern)
        let rhs = match node.child_by_field_name("right") {
            Some(n) => n,
            None => return vec![],
        };

        // Only validate literal patterns (no variable expansions)
        let pat_text = rhs.utf8_text(source).unwrap_or("");
        if pat_text.contains('$') {
            return vec![]; // dynamic pattern — can't validate at compile time
        }

        // Validate via regcomp9
        match crate::regex_validate::validate_regex(pat_text) {
            Ok(()) => vec![],
            Err(msg) => vec![Diagnostic::error(
                rhs,
                "SB5023",
                format!("invalid regex pattern: {}", msg),
            )],
        }
    }
}

fn has_ancestor(node: Node, kind: &str) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == kind {
            return true;
        }
        current = n.parent();
    }
    false
}
