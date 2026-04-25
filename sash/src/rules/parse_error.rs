use tree_sitter::Node;
use crate::diagnostic::Diagnostic;
use crate::rule::Rule;

/// SB4014: tree-sitter parse error — source contains syntax that
/// tree-sitter couldn't parse. This catches malformed input before
/// lowering, where ERROR nodes would be silently skipped.
pub struct ParseError;

impl Rule for ParseError {
    fn name(&self) -> &'static str { "parse-error" }

    fn node_kinds(&self) -> &'static [&'static str] {
        &["ERROR"]
    }

    fn check(&self, node: Node, source: &[u8]) -> Vec<Diagnostic> {
        let text = node.utf8_text(source).unwrap_or("").trim();
        let snippet = if text.len() > 40 {
            format!("{}...", &text[..40])
        } else {
            text.to_string()
        };
        vec![Diagnostic::error(
            node,
            "SB4014",
            format!("parse error: `{}` — source could not be parsed", snippet),
        )]
    }
}
