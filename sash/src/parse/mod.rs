//! Hand-written recursive descent parser for sash-native syntax.
//!
//! Replaces tree-sitter CST + lower/ pass. Produces `ir::Program` directly.
//! Absorbs validator rules that are parse-time rejections (category 1+3).
//!
//! SECURITY: This module is the ONLY entry point from source text to IR.
//! Every rejected construct produces a diagnostic with an SB* code.
//! Unknown syntax is a hard error — nothing passes silently.

mod arith;
mod error;
mod expr;
mod lexer;
mod parser;
mod tests;

pub use parser::parse;
pub use arith::{classify_arith_expr, ArithKind};
