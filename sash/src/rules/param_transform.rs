use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB4015: Reject ${var@X} parameter transformations.
/// The safe subset uses ^^/^/,,/, for case conversion.
/// All @-transforms are rejected.
pub struct ParamTransform;

impl Rule for ParamTransform {

    fn node_kinds(&self) -> &'static [&'static str] {
        &["expansion"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let text = node.utf8_text(source).unwrap_or("");
        // Match ${var@X} pattern — @ followed by a single letter at end of expansion
        if let Some(at_pos) = text.rfind('@') {
            let after = &text[at_pos + 1..];
            let after = after.trim_end_matches('}');
            if after.len() == 1 && after.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
                // Make sure this isn't an array @-subscript like ${arr[@]}
                if !text.contains("[@]") && !text.contains("[@}") {
                    return vec![Diagnostic::error(
                        node,
                        "SB4015",
                        format!("parameter transformation `${{var@{}}}` is not supported — use `^^`/`,,` for case conversion", after),
                    )];
                }
            }
        }
        vec![]
    }
}
