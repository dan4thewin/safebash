/// A diagnostic emitted by a validation rule.
///
/// Every diagnostic is a hard error. There is no severity axis: a `Deferred`
/// variant existed until 2026-10-04 for SB4004 (variable-as-command, to be
/// resolved by SSA), but SB4004 was retired and nothing ever constructed
/// `Deferred` again. DD#116 — it answered no question the pass ordering had
/// not already settled, so it was a rival, not a backstop.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub line: usize, // 1-based
    pub col: usize,  // 1-based
    pub code: &'static str,
    pub message: String,
}

impl Diagnostic {
    pub fn error_at(line: usize, col: usize, code: &'static str, message: String) -> Self {
        Self {
            line,
            col,
            code,
            message,
        }
    }

    /// Create an error diagnostic from a Span (avoids `.line`/`.col` extraction at call sites).
    pub fn error_span(span: crate::ir::Span, code: &'static str, message: String) -> Self {
        Self::error_at(span.line, span.col, code, message)
    }
}
