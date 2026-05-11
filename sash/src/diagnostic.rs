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
    pub line: usize,      // 1-based
    pub col: usize,       // 1-based
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    pub fn error(node: tree_sitter::Node, code: &'static str, message: String) -> Self {
        Self {
            line: node.start_position().row + 1,
            col: node.start_position().column + 1,
            code,
            severity: Severity::Error,
            message,
        }
    }

    pub fn deferred(node: tree_sitter::Node, code: &'static str, message: String) -> Self {
        Self {
            line: node.start_position().row + 1,
            col: node.start_position().column + 1,
            code,
            severity: Severity::Deferred,
            message,
        }
    }
}
