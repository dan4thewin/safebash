//! Parse-time diagnostics with SB* error codes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::ir::Span;

/// Create an error diagnostic from a span.
pub fn parse_error(span: Span, code: &'static str, message: String) -> Diagnostic {
    Diagnostic {
        line: span.line,
        col: span.col,
        code,
        severity: Severity::Error,
        message,
    }
}
