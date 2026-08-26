/// Severity of a diagnostic.
#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    /// Hard error — script cannot compile.
    Error,
    /// Deferred — must be resolved by SSA in phase 4; error if unresolved.
    Deferred,
}

/// A diagnostic emitted by a validation rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub line: usize, // 1-based
    pub col: usize,  // 1-based
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    pub fn error_at(line: usize, col: usize, code: &'static str, message: String) -> Self {
        Self {
            line,
            col,
            code,
            severity: Severity::Error,
            message,
        }
    }

    /// Create an error diagnostic from a Span (avoids `.line`/`.col` extraction at call sites).
    pub fn error_span(span: crate::ir::Span, code: &'static str, message: String) -> Self {
        Self::error_at(span.line, span.col, code, message)
    }
}
