use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB5006: Only * glob is allowed. ? and [...] glob patterns are rejected.
pub struct GlobRestricted;

impl Rule for GlobRestricted {
    fn name(&self) -> &'static str { "glob-restricted" }

    fn node_kinds(&self) -> &'static [&'static str] {
        &["word", "concatenation"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let text = node.utf8_text(source).unwrap_or("");

        // Skip words inside [[ ]] (regex context) or string literals
        if has_ancestor(node, "test_command") || has_ancestor(node, "string") {
            return vec![];
        }

        // Skip quoted words (the word itself won't contain glob chars literally
        // if properly quoted, but tree-sitter separates them)
        if text.starts_with('"') || text.starts_with('\'') {
            return vec![];
        }

        // Check for ** globstar (but not single *)
        if text.contains("**") && !text.starts_with("${") {
            return vec![Diagnostic::error(
                node,
                "SB5006",
                format!("** globstar is not supported in sash — only * is allowed: {}", text),
            )];
        }

        // Check for ? glob (but not in ${var?...} or ${var:?...})
        if text.contains('?') && !text.starts_with("${") {
            return vec![Diagnostic::error(
                node,
                "SB5006",
                format!("? glob is not supported in sash — only * is allowed: {}", text),
            )];
        }

        // Check for [...] character class glob (but not array subscript ${arr[n]})
        if has_bracket_glob(text) && !text.starts_with("${") {
            return vec![Diagnostic::error(
                node,
                "SB5006",
                format!("[...] glob is not supported in sash — only * is allowed: {}", text),
            )];
        }

        vec![]
    }
}

fn has_ancestor(node: Node, kind: &str) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == kind { return true; }
        current = n.parent();
    }
    false
}

/// Check if text contains a bracket glob pattern [...]
/// Must distinguish from array subscripts and arithmetic
fn has_bracket_glob(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            // Look for matching ]
            if let Some(j) = bytes[i+1..].iter().position(|&b| b == b']') {
                let inner = &text[i+1..i+1+j];
                // Skip empty [] or [0], [1] etc (array subscripts)
                if !inner.is_empty() && !inner.chars().all(|c| c.is_ascii_digit()) {
                    return true;
                }
                i = i + 1 + j + 1;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    false
}
