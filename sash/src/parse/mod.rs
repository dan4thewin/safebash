//! Hand-written recursive descent parser for sash-native syntax.
//!
//! Replaces tree-sitter CST + lower/ pass. Produces `ir::Program` directly.
//! Absorbs validator rules that are parse-time rejections (category 1+3).
//!
//! SECURITY: This module is the ONLY entry point from source text to IR.
//! Every rejected construct produces a diagnostic with an SB* code.
//! Unknown syntax is a hard error — nothing passes silently.
//!
//! Reading order:
//!   1. `lexer.rs`  — byte-level scanner, tokenization, string interpolation
//!   2. `parser.rs` — recursive descent: statements, control flow, compounds
//!   3. `expr.rs`   — word/expression parsing, parameter expansion, command substitution
//!   4. `arith.rs`  — Pratt parser for arithmetic expressions, classification, lowering
//!   5. `error.rs`  — diagnostic constructors

mod arith;
mod error;
mod expr;
mod lexer;
mod logos_tokens;
mod parser;
mod tests;

pub use arith::{classify_arith_expr, parse_arith_str, parse_dd72_literal, ArithKind};
pub use parser::parse;
