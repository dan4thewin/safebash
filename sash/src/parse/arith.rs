//! Arithmetic expression parsing: Pratt parser for $(( )) and (( )).
//!
//! Produces structured `ir::ArithExpr` nodes with correct precedence.
//! Handles: binary ops (15 levels), unary prefix/postfix, ternary,
//! assignment, compound assignment, comma, and DD#58 sexpr calls.
//!
//! Three-phase design:
//!   1. `tokenize_arith` — source string → ATok stream
//!   2. `ArithParser::parse_expr` — Pratt parser with binding power table
//!   3. `classify_arith_expr` + `lower_arith_command` — classify and lower
//!      to IR statements (mutation vs. comparison vs. test)
//!
//! SECURITY: SB5007 enforcement starts here:
//!   - `reject_prefix_incdec` — prefix ++/-- always rejected
//!   - `reject_nested_mutation` — e.g. `x = y++` rejected
//!   - Mutation in expression context ($(())) rejected at parse time

use super::lexer::{Scanner, Tok};
use crate::diagnostic::Diagnostic;
use crate::ir::*;

/// Create a parse error diagnostic for arithmetic expressions.
fn arith_error(span: Span, message: String) -> Diagnostic {
    Diagnostic::error_at(span.line, span.col, "SB4020", message)
}

/// Create a parse error for an out-of-range integer literal (SB4026).
/// Decimal magnitude > i64::MAX, or a hex/octal bit-pattern wider than
/// 64 bits, is rejected at compile time (Policy A / DD#25).
fn arith_range_error(span: Span, literal: &str) -> Diagnostic {
    Diagnostic::error_at(
        span.line,
        span.col,
        "SB4026",
        format!("integer literal out of range: {literal}"),
    )
}

/// Create a parse error for a malformed / unsupported numeric literal (SB4035,
/// DD#72): a leading-zero integer (use `0o…` for octal or drop the zero), a
/// bash arbitrary-base literal `base#digits` (use `0x`/`0o`/`0b`), or an
/// invalid prefix digit (`0o8`). Distinct from SB4026 (out of range).
fn arith_bad_literal_error(span: Span, literal: &str) -> Diagnostic {
    Diagnostic::error_at(
        span.line,
        span.col,
        "SB4035",
        format!(
            "invalid numeric literal '{literal}' — use 0x/0o/0b prefixes; \
             leading-zero octal and base#digits are not supported"
        ),
    )
}

// ── Public API ─────────────────────────────────────────────────────────

/// Parse arithmetic expansion: content between $(( and ))
/// Returns an Expr with Arithmetic kind.
pub fn parse_arith_expansion(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    let content = collect_arith_content(scanner)?;
    let expr = parse_arith_str(&content, span)?;
    // SB5007: reject mutations inside $(( ))
    reject_prefix_incdec(&expr, span)?;
    if let crate::ir::ArithExpr::Assign { var, .. }
    | crate::ir::ArithExpr::CompoundAssign { var, .. } = &expr
    {
        return Err(arith_error(span, format!(
            "mutation {} = ... not allowed in $(( )) expression context — use (( )) statement instead", var
        )));
    }
    if let crate::ir::ArithExpr::PostIncDec { var, inc } = &expr {
        let op = if *inc { "++" } else { "--" };
        return Err(arith_error(span, format!(
            "mutation {}{} not allowed in $(( )) expression context — use (( )) statement instead", var, op
        )));
    }
    Ok(Expr {
        kind: ExprKind::Arithmetic(Box::new(expr)),
        span,
    })
}

/// Parse an arithmetic command: (( expr )) as a statement.
/// Returns a structured ArithExpr for the caller to classify and lower.
pub fn parse_arith_command(scanner: &mut Scanner, span: Span) -> Result<ArithExpr, Diagnostic> {
    let content = collect_arith_content(scanner)?;
    let expr = parse_arith_str(&content, span)?;
    // SB5007: reject prefix ++/-- at parse time
    reject_prefix_incdec(&expr, span)?;
    // SB5007: reject nested mutations (e.g. x = y++)
    reject_nested_mutation(&expr, span)?;
    Ok(expr)
}

/// Reject prefix ++/-- anywhere in an arithmetic expression.
fn reject_prefix_incdec(
    expr: &crate::ir::ArithExpr,
    span: crate::ir::Span,
) -> Result<(), Diagnostic> {
    use crate::ir::ArithExpr;
    match expr {
        ArithExpr::PreIncDec { var, inc } => {
            let op = if *inc { "++" } else { "--" };
            let inc_dec_str = if *inc {
                " += 1 )) instead"
            } else {
                " -= 1 )) instead"
            };
            Err(arith_error(
                span,
                format!(
                    "prefix {}{} is not allowed — use postfix ({}{}) or (( {}{}",
                    op, var, var, op, var, inc_dec_str
                ),
            ))
        }
        ArithExpr::BinOp { left, right, .. } => {
            reject_prefix_incdec(left, span)?;
            reject_prefix_incdec(right, span)
        }
        ArithExpr::Assign { value, .. } => reject_prefix_incdec(value, span),
        ArithExpr::CompoundAssign { value, .. } => reject_prefix_incdec(value, span),
        ArithExpr::Comma(exprs) => {
            for e in exprs {
                reject_prefix_incdec(e, span)?;
            }
            Ok(())
        }
        ArithExpr::Paren(inner) => reject_prefix_incdec(inner, span),
        _ => Ok(()),
    }
}

/// Reject nested mutations: e.g. (( x = y++ )) has y++ inside assignment
fn reject_nested_mutation(
    expr: &crate::ir::ArithExpr,
    span: crate::ir::Span,
) -> Result<(), Diagnostic> {
    use crate::ir::ArithExpr;
    match expr {
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            if has_mutation_inner(value) {
                let desc = brief_arith(value);
                return Err(arith_error(span, format!(
                    "mutation {} not allowed in statement (( )) expression context — use (( )) statement instead", desc
                )));
            }
            Ok(())
        }
        ArithExpr::Comma(exprs) => {
            for e in exprs {
                reject_nested_mutation(e, span)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn has_mutation_inner(expr: &crate::ir::ArithExpr) -> bool {
    use crate::ir::ArithExpr;
    match expr {
        ArithExpr::PostIncDec { .. }
        | ArithExpr::PreIncDec { .. }
        | ArithExpr::Assign { .. }
        | ArithExpr::CompoundAssign { .. } => true,
        ArithExpr::BinOp { left, right, .. } => {
            has_mutation_inner(left) || has_mutation_inner(right)
        }
        ArithExpr::Paren(inner) => has_mutation_inner(inner),
        _ => false,
    }
}

fn brief_arith(expr: &crate::ir::ArithExpr) -> String {
    use crate::ir::ArithExpr;
    match expr {
        ArithExpr::PostIncDec { var, inc } => format!("{}{}", var, if *inc { "++" } else { "--" }),
        ArithExpr::PreIncDec { var, inc } => format!("{}{}", if *inc { "++" } else { "--" }, var),
        ArithExpr::Assign { var, .. } => format!("{} = ...", var),
        _ => "...".to_string(),
    }
}

/// Parse an arithmetic string (already extracted from delimiters).
/// This is the entry point for the Pratt parser.
pub fn parse_arith_str(input: &str, span: Span) -> Result<ArithExpr, Diagnostic> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(ArithExpr::Literal(0));
    }
    let tokens = tokenize_arith(trimmed);
    let mut parser = ArithParser {
        tokens,
        pos: 0,
        span,
    };
    let expr = parser.parse_expr(0)?;
    // Handle comma operator at top level
    let result = if parser.peek() == Some(&ATok::Comma) {
        let mut exprs = vec![expr];
        while parser.peek() == Some(&ATok::Comma) {
            parser.advance();
            exprs.push(parser.parse_expr(0)?);
        }
        ArithExpr::Comma(exprs)
    } else {
        expr
    };
    reject_trailing_token(&parser, span)?;
    Ok(result)
}

/// DD#72: after a full parse, any leftover token is a hard error (not a silent
/// tail). A malformed literal (`BadLit`, e.g. a stray `#`) gives SB4035; any
/// other trailing token is an SB4020 syntax error.
fn reject_trailing_token(parser: &ArithParser, span: Span) -> Result<(), Diagnostic> {
    match parser.peek() {
        Some(ATok::BadLit(s)) => Err(arith_bad_literal_error(span, s)),
        Some(_) => Err(arith_error(
            span,
            "unexpected token after arithmetic expression".to_string(),
        )),
        None => Ok(()),
    }
}

/// Classify an arithmetic expression as mutation vs. comparison/test.
pub fn classify_arith_expr(expr: &ArithExpr) -> ArithKind {
    match expr {
        ArithExpr::Assign { .. }
        | ArithExpr::CompoundAssign { .. }
        | ArithExpr::PreIncDec { .. }
        | ArithExpr::PostIncDec { .. } => ArithKind::Mutation,
        ArithExpr::Comma(exprs) => {
            // If any part is a mutation, the whole thing is
            if exprs
                .iter()
                .any(|e| classify_arith_expr(e) == ArithKind::Mutation)
            {
                ArithKind::Mutation
            } else {
                ArithKind::Test
            }
        }
        ArithExpr::BinOp {
            op:
                IrArithOp::Eq
                | IrArithOp::Ne
                | IrArithOp::Lt
                | IrArithOp::Le
                | IrArithOp::Gt
                | IrArithOp::Ge
                | IrArithOp::And
                | IrArithOp::Or,
            ..
        } => ArithKind::Comparison,
        _ => ArithKind::Test,
    }
}

/// Lower an arithmetic command (( expr )) to an IR Statement.
pub fn lower_arith_command(expr: ArithExpr, span: Span) -> Statement {
    let kind = classify_arith_expr(&expr);
    match kind {
        ArithKind::Mutation => lower_arith_mutation_structured(&expr, span),
        ArithKind::Comparison | ArithKind::Test => lower_arith_test_structured(&expr, span),
    }
}

/// Lower a C-style for loop: for (( init; cond; update )); do body; done
pub fn lower_c_style_for(
    init: &str,
    cond: &str,
    update: &str,
    body: Vec<Statement>,
    span: Span,
) -> Statement {
    // Init
    let mut init_stmts = Vec::new();
    let init_trimmed = init.trim();
    if !init_trimmed.is_empty() {
        if let Ok(expr) = parse_arith_str(init_trimmed, span) {
            init_stmts.push(lower_arith_command(expr, span));
        }
    }

    // Condition
    let cond_trimmed = cond.trim();
    let condition = if cond_trimmed.is_empty() {
        vec![Statement::true_cmd(span)]
    } else if let Ok(expr) = parse_arith_str(cond_trimmed, span) {
        vec![lower_arith_test_structured(&expr, span)]
    } else {
        vec![Statement::true_cmd(span)]
    };

    // Update
    let mut loop_body = body;
    let update_trimmed = update.trim();
    if !update_trimmed.is_empty() {
        if let Ok(expr) = parse_arith_str(update_trimmed, span) {
            loop_body.push(lower_arith_command(expr, span));
        }
    }

    let while_stmt = Statement {
        kind: StmtKind::While(While {
            condition,
            body: loop_body,
            until: false,
        }),
        span,
    };

    let mut group_body = init_stmts;
    group_body.push(while_stmt);
    Statement {
        kind: StmtKind::Group(Group { body: group_body }),
        span,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArithKind {
    Mutation,
    Comparison,
    Test,
}

// ── Arithmetic tokenizer ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum ATok {
    Num(i64),
    /// An out-of-range integer literal (raw text retained for the
    /// diagnostic). Rejected with SB4026 when the parser consumes it,
    /// except the `-9223372036854775808` (i64::MIN) case folded by the
    /// unary-neg peephole in `parse_unary`.
    BadNum(String),
    /// A malformed / unsupported numeric literal (DD#72): a leading-zero
    /// integer (`010`), a bash arbitrary-base literal (`16#ff`), an invalid
    /// prefix digit (`0o8`), or an unknown byte in the arith stream. Rejected
    /// with SB4035 when consumed. Distinct from `BadNum` (out-of-range, SB4026).
    BadLit(String),
    Var(String),
    DollarVar(String),
    // Operators (ordered by length for tokenization)
    PlusPlus,
    MinusMinus,
    StarStar,  // **
    LtLt,      // <<
    GtGt,      // >>
    LtEq,      // <=
    GtEq,      // >=
    EqEq,      // ==
    BangEq,    // !=
    AmpAmp,    // &&
    PipePipe,  // ||
    PlusEq,    // +=
    MinusEq,   // -=
    StarEq,    // *=
    SlashEq,   // /=
    PercentEq, // %=
    AmpEq,     // &=
    PipeEq,    // |=
    CaretEq,   // ^=
    LtLtEq,    // <<=
    GtGtEq,    // >>=
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Gt,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Bang,
    Eq, // = (assignment)
    Question,
    Colon,
    Comma,
    LParen,
    RParen,
}

fn tokenize_arith(input: &str) -> Vec<ATok> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if let Some((tok, ni)) = next_arith_token(input, bytes, i) {
            tokens.push(tok);
            i = ni;
        } else {
            // Unknown byte (DD#72): a hard error, not a silent skip. Emit a
            // BadLit spanning the byte so the parser rejects it (SB4035).
            let ch = input[i..].chars().next().unwrap_or('?');
            tokens.push(ATok::BadLit(ch.to_string()));
            i += ch.len_utf8();
        }
    }
    tokens
}

/// Recognise one non-whitespace token starting at `bytes[i]`. Returns
/// the token and the byte index just past it, or None if no rule matches.
fn next_arith_token(input: &str, bytes: &[u8], i: usize) -> Option<(ATok, usize)> {
    if bytes[i].is_ascii_digit() {
        return Some(scan_arith_number(input, bytes, i));
    }
    if bytes[i] == b'$' {
        return Some(scan_arith_dollar(input, bytes, i));
    }
    if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
        return Some(scan_arith_ident(input, bytes, i));
    }
    match_arith_operator(bytes, i)
}

/// Longest-match operator dispatch: 3-byte → 2-byte → 1-byte.
fn match_arith_operator(bytes: &[u8], i: usize) -> Option<(ATok, usize)> {
    if i + 3 <= bytes.len() {
        if let Some(t) = match_arith_op3(&bytes[i..i + 3]) {
            return Some((t, i + 3));
        }
    }
    if i + 2 <= bytes.len() {
        if let Some(t) = match_arith_op2(&bytes[i..i + 2]) {
            return Some((t, i + 2));
        }
    }
    match_arith_op1(bytes[i]).map(|t| (t, i + 1))
}

/// Scan a number literal at `bytes[i..]` (DD#72). Accepts decimal and the
/// modern prefixes `0x`/`0o`/`0b`. A leading-zero integer (`010`) or a bash
/// arbitrary-base literal (`16#ff`) is a malformed literal (`BadLit` → SB4035).
fn scan_arith_number(input: &str, bytes: &[u8], i: usize) -> (ATok, usize) {
    if bytes[i] == b'0' && i + 1 < bytes.len() {
        match bytes[i + 1] {
            b'x' | b'X' => return scan_radix_literal(input, bytes, i, 16),
            b'o' | b'O' => return scan_radix_literal(input, bytes, i, 8),
            b'b' | b'B' => return scan_radix_literal(input, bytes, i, 2),
            // Leading-zero integer (`010`, `08`): C-octal creep — rejected.
            d if d.is_ascii_digit() => {
                let end = scan_decimal_digits(bytes, i);
                return (ATok::BadLit(input[i..end].to_string()), end);
            }
            _ => {}
        }
    }
    let digits_end = scan_decimal_digits(bytes, i);
    // bash arbitrary base `base#digits` (`16#ff`) — dropped (DD#72).
    if digits_end < bytes.len() && bytes[digits_end] == b'#' {
        let end = scan_base_hash_span(bytes, digits_end);
        return (ATok::BadLit(input[i..end].to_string()), end);
    }
    scan_decimal_literal(input, i, digits_end)
}

/// Scan a `0x`/`0o`/`0b` prefixed literal (radix 16/8/2). The alphanumeric run
/// after the two-byte prefix is the digit span; an empty or invalid-digit span
/// is a `BadLit` (SB4035); an in-range value is `Num`; out of 64 bits is
/// `BadNum` (SB4026). `0xFFFFFFFFFFFFFFFF` == -1 (bash bit-pattern parity).
fn scan_radix_literal(input: &str, bytes: &[u8], start: usize, radix: u32) -> (ATok, usize) {
    let digit_start = start + 2;
    let mut j = digit_start;
    while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
        j += 1;
    }
    let digits = &input[digit_start..j];
    let valid = !digits.is_empty() && digits.bytes().all(|b| (b as char).is_digit(radix));
    if !valid {
        return (ATok::BadLit(input[start..j].to_string()), j);
    }
    match u64::from_str_radix(digits, radix) {
        Ok(v) => (ATok::Num(v as i64), j),
        Err(_) => (ATok::BadNum(input[start..j].to_string()), j),
    }
}

/// Extent of a `base#digits` payload starting at the `#` (for the SB4035 span):
/// the `#` plus the following bash base-digit run (alphanumerics / `_` / `@`).
fn scan_base_hash_span(bytes: &[u8], hash: usize) -> usize {
    let mut j = hash + 1;
    while j < bytes.len()
        && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'@')
    {
        j += 1;
    }
    j
}

fn scan_decimal_digits(bytes: &[u8], start: usize) -> usize {
    let mut j = start;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    j
}

fn scan_decimal_literal(input: &str, start: usize, end: usize) -> (ATok, usize) {
    let text = &input[start..end];
    // Decimal magnitude must fit i64 (Policy A: out of range -> SB4026).
    // The i64::MIN case `-9223372036854775808` is handled by the unary-neg
    // peephole in parse_unary, which folds `-<BadNum(9223372036854775808)>`.
    let tok = match text.parse::<i64>() {
        Ok(v) => ATok::Num(v),
        Err(_) => ATok::BadNum(text.to_string()),
    };
    (tok, end)
}

/// Scan a `$`-prefixed variable: `$name` or `${name}`. Returns the
/// token and the byte index just past it.
fn scan_arith_dollar(input: &str, bytes: &[u8], i: usize) -> (ATok, usize) {
    let j = i + 1; // skip '$'
    if j < bytes.len() && bytes[j] == b'{' {
        return scan_arith_braced_var(input, bytes, j + 1);
    }
    // Special single-char vars: $#, $?, $$, $!, $@, $* (none are
    // ascii-alphanumeric, so the identifier scan would miss them and leave
    // the sigil char stray). Positionals $1..$9 are digits, handled by the
    // alphanumeric scan in scan_arith_named_var.
    if j < bytes.len() && matches!(bytes[j], b'#' | b'?' | b'$' | b'!' | b'@' | b'*') {
        return (ATok::DollarVar((bytes[j] as char).to_string()), j + 1);
    }
    scan_arith_named_var(input, bytes, j)
}

/// Scan a braced `${name}` body starting just past the `{`.
fn scan_arith_braced_var(input: &str, bytes: &[u8], start: usize) -> (ATok, usize) {
    let mut j = start;
    while j < bytes.len() && bytes[j] != b'}' {
        j += 1;
    }
    let name = input[start..j].to_string();
    if j < bytes.len() {
        j += 1; // skip '}'
    }
    (ATok::DollarVar(name), j)
}

/// Scan a bare `$name` (alphanumeric/`_`, including positional digits).
fn scan_arith_named_var(input: &str, bytes: &[u8], start: usize) -> (ATok, usize) {
    let mut j = start;
    while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        j += 1;
    }
    (ATok::DollarVar(input[start..j].to_string()), j)
}

/// Scan a bare identifier: starts alpha/`_`, continues alphanumeric/`_`.
fn scan_arith_ident(input: &str, bytes: &[u8], i: usize) -> (ATok, usize) {
    let start = i;
    let mut j = i;
    while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        j += 1;
    }
    (ATok::Var(input[start..j].to_string()), j)
}

/// Match a three-byte arithmetic operator: `<<=` or `>>=`.
fn match_arith_op3(s: &[u8]) -> Option<ATok> {
    match s {
        b"<<=" => Some(ATok::LtLtEq),
        b">>=" => Some(ATok::GtGtEq),
        _ => None,
    }
}

/// Match a two-byte arithmetic operator (compound assignment, comparison,
/// shift, increment/decrement, logical-and/or, exponent).
fn match_arith_op2(s: &[u8]) -> Option<ATok> {
    match s {
        b"++" => Some(ATok::PlusPlus),
        b"--" => Some(ATok::MinusMinus),
        b"**" => Some(ATok::StarStar),
        b"<<" => Some(ATok::LtLt),
        b">>" => Some(ATok::GtGt),
        b"<=" => Some(ATok::LtEq),
        b">=" => Some(ATok::GtEq),
        b"==" => Some(ATok::EqEq),
        b"!=" => Some(ATok::BangEq),
        b"&&" => Some(ATok::AmpAmp),
        b"||" => Some(ATok::PipePipe),
        b"+=" => Some(ATok::PlusEq),
        b"-=" => Some(ATok::MinusEq),
        b"*=" => Some(ATok::StarEq),
        b"/=" => Some(ATok::SlashEq),
        b"%=" => Some(ATok::PercentEq),
        b"&=" => Some(ATok::AmpEq),
        b"|=" => Some(ATok::PipeEq),
        b"^=" => Some(ATok::CaretEq),
        _ => None,
    }
}

/// Match a single-byte arithmetic operator.
fn match_arith_op1(b: u8) -> Option<ATok> {
    match b {
        b'+' => Some(ATok::Plus),
        b'-' => Some(ATok::Minus),
        b'*' => Some(ATok::Star),
        b'/' => Some(ATok::Slash),
        b'%' => Some(ATok::Percent),
        b'<' => Some(ATok::Lt),
        b'>' => Some(ATok::Gt),
        b'&' => Some(ATok::Amp),
        b'|' => Some(ATok::Pipe),
        b'^' => Some(ATok::Caret),
        b'~' => Some(ATok::Tilde),
        b'!' => Some(ATok::Bang),
        b'=' => Some(ATok::Eq),
        b'?' => Some(ATok::Question),
        b':' => Some(ATok::Colon),
        b',' => Some(ATok::Comma),
        b'(' => Some(ATok::LParen),
        b')' => Some(ATok::RParen),
        _ => None,
    }
}

// ── Pratt parser ──────────────────────────────────────────────────────

struct ArithParser {
    tokens: Vec<ATok>,
    pos: usize,
    span: Span,
}

impl ArithParser {
    fn peek(&self) -> Option<&ATok> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&ATok> {
        let tok = self.tokens.get(self.pos);
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &ATok) -> Result<(), Diagnostic> {
        if self.peek() == Some(expected) {
            self.advance();
            Ok(())
        } else {
            Err(arith_error(
                self.span,
                format!("expected {:?} in arithmetic expression", expected),
            ))
        }
    }

    /// Parse an expression with given minimum binding power.
    fn parse_expr(&mut self, min_bp: u8) -> Result<ArithExpr, Diagnostic> {
        let mut lhs = self.parse_prefix()?;

        loop {
            if let Some(new_lhs) = self.try_postfix(&lhs) {
                lhs = new_lhs;
                continue;
            }

            if let Some(result) = self.try_assignment(&lhs)? {
                lhs = result;
                continue;
            }

            if self.peek() == Some(&ATok::Question) {
                if min_bp > 2 {
                    break;
                }
                self.advance();
                let true_val = self.parse_expr(0)?;
                self.expect(&ATok::Colon)?;
                let false_val = self.parse_expr(2)?; // right-associative
                lhs = ArithExpr::Ternary {
                    cond: Box::new(lhs),
                    true_val: Box::new(true_val),
                    false_val: Box::new(false_val),
                };
                continue;
            }

            let Some((op, (l_bp, r_bp))) = self.peek().and_then(binary_op_for) else {
                break;
            };
            if l_bp < min_bp {
                break;
            }
            self.advance();
            let rhs = self.parse_expr(r_bp)?;
            lhs = ArithExpr::BinOp {
                left: Box::new(lhs),
                op,
                right: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    /// Handle `var++` and `var--` postfix. Returns the new LHS if consumed.
    fn try_postfix(&mut self, lhs: &ArithExpr) -> Option<ArithExpr> {
        let inc = match self.peek()? {
            ATok::PlusPlus => true,
            ATok::MinusMinus => false,
            _ => return None,
        };
        let ArithExpr::Var(name) = lhs else {
            return None;
        };
        let name = name.clone();
        self.advance();
        Some(ArithExpr::PostIncDec { var: name, inc })
    }

    /// Handle assignment operators (`=`, `+=`, `-=`, ...). Returns the new
    /// LHS if an assignment was consumed; `None` if the next token isn't an
    /// assignment operator or LHS isn't a Var.
    fn try_assignment(&mut self, lhs: &ArithExpr) -> Result<Option<ArithExpr>, Diagnostic> {
        let ArithExpr::Var(name) = lhs else {
            return Ok(None);
        };
        let Some(tok) = self.peek() else {
            return Ok(None);
        };
        let assign_op = match assign_op_for(tok) {
            Some(op) => op,
            None => return Ok(None),
        };
        let name = name.clone();
        self.advance();
        let rhs = self.parse_expr(1)?; // right-associative
        let new_lhs = match assign_op {
            None => ArithExpr::Assign {
                var: name,
                value: Box::new(rhs),
            },
            Some(op) => ArithExpr::CompoundAssign {
                var: name,
                op,
                value: Box::new(rhs),
            },
        };
        Ok(Some(new_lhs))
    }

    /// Parse prefix expressions and atoms.
    fn parse_prefix(&mut self) -> Result<ArithExpr, Diagnostic> {
        match self.peek().cloned() {
            Some(ATok::PlusPlus) => self.parse_pre_incdec(true),
            Some(ATok::MinusMinus) => self.parse_pre_incdec(false),
            Some(ATok::Minus) => self.parse_unary(IrUnaryArithOp::Neg),
            Some(ATok::Plus) => self.parse_unary(IrUnaryArithOp::Pos),
            Some(ATok::Bang) => self.parse_unary(IrUnaryArithOp::Not),
            Some(ATok::Tilde) => self.parse_unary(IrUnaryArithOp::BitNot),
            _ => self.parse_prefix_atom(),
        }
    }

    /// Atoms and operands: literals, variables, parenthesised forms. An
    /// out-of-range `BadNum` is rejected here (SB4026); the `-i64::MIN`
    /// case is folded earlier by the unary-neg peephole in `parse_unary`.
    fn parse_prefix_atom(&mut self) -> Result<ArithExpr, Diagnostic> {
        match self.peek().cloned() {
            Some(ATok::Num(n)) => {
                self.advance();
                Ok(ArithExpr::Literal(n))
            }
            Some(ATok::Var(name)) => {
                self.advance();
                Ok(ArithExpr::Var(name))
            }
            Some(ATok::DollarVar(name)) => {
                self.advance();
                Ok(ArithExpr::DollarVar(name))
            }
            Some(ATok::LParen) => self.parse_lparen_form(),
            Some(ATok::BadNum(s)) => {
                self.advance();
                Err(arith_range_error(self.span, &s))
            }
            Some(ATok::BadLit(s)) => {
                self.advance();
                Err(arith_bad_literal_error(self.span, &s))
            }
            Some(_) => {
                // Unknown token — try to recover
                self.advance();
                Ok(ArithExpr::Literal(0))
            }
            None => Ok(ArithExpr::Literal(0)),
        }
    }

    /// `++var` / `--var` — pre-increment or pre-decrement. The
    /// `++`/`--` token is at `self.pos`; consumes both it and the
    /// following `Var`.
    fn parse_pre_incdec(&mut self, inc: bool) -> Result<ArithExpr, Diagnostic> {
        self.advance();
        let Some(ATok::Var(name)) = self.peek().cloned() else {
            let op = if inc { "++" } else { "--" };
            return Err(arith_error(
                self.span,
                format!("expected variable after {op}"),
            ));
        };
        self.advance();
        Ok(ArithExpr::PreIncDec { var: name, inc })
    }

    /// `-x` / `+x` / `!x` / `~x` — unary arithmetic. The operator
    /// token is at `self.pos`; consumes it and parses the operand at
    /// precedence 25 (binds tighter than any binary).
    fn parse_unary(&mut self, op: IrUnaryArithOp) -> Result<ArithExpr, Diagnostic> {
        self.advance();
        // i64::MIN peephole (Policy A): the bare literal 9223372036854775808
        // is i64::MAX+1 (out of range as BadNum), but `-9223372036854775808`
        // is the valid i64::MIN. Fold it here so i64::MIN is expressible as
        // an arith literal (matches bash; the arith lexer splits the `-`).
        if matches!(op, IrUnaryArithOp::Neg) {
            if let Some(ATok::BadNum(s)) = self.peek() {
                if s == "9223372036854775808" {
                    self.advance();
                    return Ok(ArithExpr::Literal(i64::MIN));
                }
            }
        }
        let operand = self.parse_expr(25)?;
        Ok(ArithExpr::UnaryOp {
            op,
            operand: Box::new(operand),
        })
    }

    /// `(...)` — either a DD#58 sexpr-style function call `(name args...)`
    /// or ordinary parenthesised grouping. The opening `(` is at
    /// `self.pos`; consumes through the matching `)`.
    fn parse_lparen_form(&mut self) -> Result<ArithExpr, Diagnostic> {
        self.advance();
        // DD#58: peek for a sexpr function call.
        if let Some(ATok::Var(name)) = self.peek().cloned() {
            let saved_pos = self.pos;
            self.advance(); // consume name
            if self.peek() == Some(&ATok::RParen) {
                // (name) — zero-arg function call
                self.advance();
                return Ok(ArithExpr::FuncCall { name, args: vec![] });
            }
            if self.is_func_arg_start() {
                // (name expr...) — function call with args
                let mut args = Vec::new();
                while self.peek() != Some(&ATok::RParen) && self.peek().is_some() {
                    args.push(self.parse_expr(0)?);
                }
                self.expect(&ATok::RParen)?;
                return Ok(ArithExpr::FuncCall { name, args });
            }
            // Not a function call — restore and parse as grouping
            self.pos = saved_pos;
        }
        let expr = self.parse_expr(0)?;
        self.expect(&ATok::RParen)?;
        Ok(ArithExpr::Paren(Box::new(expr)))
    }

    /// Check if current token could start a function argument (for sexpr detection).
    /// More restrictive than is_expr_start: excludes binary operators that
    /// would indicate this is grouping, not a function call.
    fn is_func_arg_start(&self) -> bool {
        matches!(
            self.peek(),
            Some(ATok::Num(_) | ATok::Var(_) | ATok::DollarVar(_) | ATok::LParen)
        )
    }
}

// ── Lowering helpers ──────────────────────────────────────────────────

/// Map a token to its binary operator + Pratt binding power.
/// Returned tuple is `(op, (left_bp, right_bp))`.
fn binary_op_for(tok: &ATok) -> Option<(IrArithOp, (u8, u8))> {
    Some(match tok {
        ATok::PipePipe => (IrArithOp::Or, (3, 4)),
        ATok::AmpAmp => (IrArithOp::And, (5, 6)),
        ATok::Pipe => (IrArithOp::BitOr, (7, 8)),
        ATok::Caret => (IrArithOp::BitXor, (9, 10)),
        ATok::Amp => (IrArithOp::BitAnd, (11, 12)),
        ATok::EqEq => (IrArithOp::Eq, (13, 14)),
        ATok::BangEq => (IrArithOp::Ne, (13, 14)),
        ATok::Lt => (IrArithOp::Lt, (15, 16)),
        ATok::LtEq => (IrArithOp::Le, (15, 16)),
        ATok::Gt => (IrArithOp::Gt, (15, 16)),
        ATok::GtEq => (IrArithOp::Ge, (15, 16)),
        ATok::LtLt => (IrArithOp::Shl, (17, 18)),
        ATok::GtGt => (IrArithOp::Shr, (17, 18)),
        ATok::Plus => (IrArithOp::Add, (19, 20)),
        ATok::Minus => (IrArithOp::Sub, (19, 20)),
        ATok::Star => (IrArithOp::Mul, (21, 22)),
        ATok::Slash => (IrArithOp::Div, (21, 22)),
        ATok::Percent => (IrArithOp::Mod, (21, 22)),
        ATok::StarStar => (IrArithOp::Pow, (24, 23)), // right-associative
        _ => return None,
    })
}

/// Map a token to its assignment operator. The outer Option distinguishes
/// "not an assignment" from "is an assignment". The inner Option is the
/// compound operator: `None` for plain `=`, `Some(op)` for `+=`, `*=`, etc.
fn assign_op_for(tok: &ATok) -> Option<Option<IrArithOp>> {
    Some(match tok {
        ATok::Eq => None,
        ATok::PlusEq => Some(IrArithOp::Add),
        ATok::MinusEq => Some(IrArithOp::Sub),
        ATok::StarEq => Some(IrArithOp::Mul),
        ATok::SlashEq => Some(IrArithOp::Div),
        ATok::PercentEq => Some(IrArithOp::Mod),
        ATok::AmpEq => Some(IrArithOp::BitAnd),
        ATok::PipeEq => Some(IrArithOp::BitOr),
        ATok::CaretEq => Some(IrArithOp::BitXor),
        ATok::LtLtEq => Some(IrArithOp::Shl),
        ATok::GtGtEq => Some(IrArithOp::Shr),
        _ => return None,
    })
}

/// Lower a mutation arithmetic expression to a Statement.
fn lower_arith_mutation_structured(expr: &ArithExpr, span: Span) -> Statement {
    match expr {
        ArithExpr::Assign { var, value } => Statement {
            kind: StmtKind::Assignment(Assignment {
                name: var.clone(),
                index: None,
                op: AssignOp::Set,
                value: Expr {
                    kind: ExprKind::Arithmetic(Box::new(*value.clone())),
                    span,
                },
            }),
            span,
        },
        ArithExpr::CompoundAssign { var, op, value } => {
            let full_expr = ArithExpr::BinOp {
                left: Box::new(ArithExpr::Var(var.clone())),
                op: *op,
                right: value.clone(),
            };
            Statement {
                kind: StmtKind::Assignment(Assignment {
                    name: var.clone(),
                    index: None,
                    op: AssignOp::Set,
                    value: Expr {
                        kind: ExprKind::Arithmetic(Box::new(full_expr)),
                        span,
                    },
                }),
                span,
            }
        }
        ArithExpr::PreIncDec { var, inc } | ArithExpr::PostIncDec { var, inc } => {
            let op = if *inc { IrArithOp::Add } else { IrArithOp::Sub };
            let full_expr = ArithExpr::BinOp {
                left: Box::new(ArithExpr::Var(var.clone())),
                op,
                right: Box::new(ArithExpr::Literal(1)),
            };
            Statement {
                kind: StmtKind::Assignment(Assignment {
                    name: var.clone(),
                    index: None,
                    op: AssignOp::Set,
                    value: Expr {
                        kind: ExprKind::Arithmetic(Box::new(full_expr)),
                        span,
                    },
                }),
                span,
            }
        }
        ArithExpr::Comma(exprs) => {
            let stmts: Vec<Statement> = exprs
                .iter()
                .map(|e| lower_arith_command(e.clone(), span))
                .collect();
            Statement {
                kind: StmtKind::Group(Group { body: stmts }),
                span,
            }
        }
        _ => {
            // Should not reach here for valid code (SB5007 rejects bare expressions)
            Statement {
                kind: StmtKind::ArithTest(Box::new(expr.clone())),
                span,
            }
        }
    }
}

/// Lower a test/comparison arithmetic expression to a Statement.
fn lower_arith_test_structured(expr: &ArithExpr, span: Span) -> Statement {
    Statement {
        kind: StmtKind::ArithTest(Box::new(expr.clone())),
        span,
    }
}

// ── Content collection ────────────────────────────────────────────────

/// Collect raw arithmetic content as a string (for C-style for loops
/// where we need to split on `;` before parsing).
pub fn collect_arith_content_raw(scanner: &mut Scanner) -> Result<String, Diagnostic> {
    collect_arith_content(scanner)
}

/// Collect raw content between the opening (( or $(( and closing )).
/// Respects nesting of (( )) and ( ).
fn collect_arith_content(scanner: &mut Scanner) -> Result<String, Diagnostic> {
    let mut content = String::new();
    let mut depth = 1i32;

    // Scan the body in arithmetic mode so `<<` lexes as left-shift, not a
    // heredoc (bug-arith-left-shift-heredoc). Set before the first peek so
    // the leading content token is tokenized in arith mode.
    scanner.enter_arith();
    while !scanner.at_eof() {
        let tok = scanner.peek().clone();
        if step_arith_collect(scanner, &tok.token, &mut content, &mut depth) {
            break;
        }
    }
    scanner.exit_arith();

    Ok(content)
}

/// One iteration of collect_arith_content. Mutates `content` and `depth`;
/// returns true when collection should terminate (closing `))` consumed).
fn step_arith_collect(
    scanner: &mut Scanner,
    tok: &Tok,
    content: &mut String,
    depth: &mut i32,
) -> bool {
    match tok {
        Tok::DblParenClose => step_dbl_paren_close(scanner, content, depth),
        Tok::DblParen => step_open_paren(scanner, content, depth, "(("),
        Tok::DollarDblParen => step_open_paren(scanner, content, depth, "$(("),
        Tok::LParen => {
            content.push('(');
            scanner.next();
            false
        }
        Tok::RParen => step_rparen(scanner, content, depth),
        _ => step_default_token(scanner, tok, content),
    }
}

/// `))` token — already-paired; decrement depth and (when depth hits 0) terminate.
fn step_dbl_paren_close(scanner: &mut Scanner, content: &mut String, depth: &mut i32) -> bool {
    *depth -= 1;
    scanner.next();
    if *depth == 0 {
        return true;
    }
    content.push_str("))");
    false
}

/// Opening `((` or `$((` — increment depth, append the text.
fn step_open_paren(
    scanner: &mut Scanner,
    content: &mut String,
    depth: &mut i32,
    text: &str,
) -> bool {
    *depth += 1;
    content.push_str(text);
    scanner.next();
    false
}

/// `)` token — if followed by another `)`, treat as `))`; otherwise single `)`.
fn step_rparen(scanner: &mut Scanner, content: &mut String, depth: &mut i32) -> bool {
    scanner.next();
    if !matches!(scanner.peek().token, Tok::RParen) {
        content.push(')');
        return false;
    }
    *depth -= 1;
    scanner.next();
    if *depth == 0 {
        return true;
    }
    content.push_str("))");
    false
}

fn step_default_token(scanner: &mut Scanner, tok: &Tok, content: &mut String) -> bool {
    let text = tok_to_text(tok);
    content.push_str(&text);
    scanner.next();
    if needs_separator_space(&scanner.peek().token, &text) {
        content.push(' ');
    }
    false
}

fn needs_separator_space(next: &Tok, prev_text: &str) -> bool {
    // The bash lexer splits compound operators whose second byte is `=`
    // into `<op>` + `=` (it has no arith context): `<=` `>=` and the
    // arithmetic compound-assignments `-=` `*=` `/=` `%=` `&=` `|=` `^=`
    // (also `<<=`/`>>=`, whose prev ends in `<`/`>`). A separator space
    // here would turn `x -= 3` into `x - = 3`, which the arith byte-lexer
    // mis-parses as `x - (=→0)` — dropping the operand and the write-back.
    // Keep them adjacent so the arith lexer recombines the operator.
    // (`+=` is a single `PlusAssign` shell token, so it never hits this;
    // `==`/`!=` are single `Word` tokens.)
    if matches!(next, Tok::Eq)
        && prev_text
            .chars()
            .next_back()
            .is_some_and(|c| matches!(c, '<' | '>' | '-' | '*' | '/' | '%' | '&' | '|' | '^'))
    {
        return false;
    }
    !matches!(
        next,
        Tok::DblParenClose
            | Tok::RParen
            | Tok::Semi
            | Tok::Newline
            | Tok::Eof
            | Tok::DblParen
            | Tok::LParen
    ) && !prev_text.ends_with(' ')
}

/// Convert a token back to source text for arithmetic content collection.
fn tok_to_text(tok: &Tok) -> String {
    match tok {
        Tok::Word(w) => w.clone(),
        Tok::Dollar(v) => format!("${}", v),
        Tok::DollarBrace(c) => format!("${{{}}}", c),
        Tok::DollarParen => "$(".to_string(),
        Tok::Eq => "=".to_string(),
        Tok::PlusAssign => "+=".to_string(),
        Tok::Bang => "!".to_string(),
        Tok::Dash => "-".to_string(),
        Tok::Pipe => "|".to_string(),
        Tok::Amp => "&".to_string(),
        Tok::AndAnd => "&&".to_string(),
        Tok::OrOr => "||".to_string(),
        Tok::Lt => "<".to_string(),
        Tok::Gt => ">".to_string(),
        Tok::DblGt => ">>".to_string(),
        Tok::Semi => ";".to_string(),
        Tok::Newline => " ".to_string(),
        Tok::SingleQuoted(s) => format!("'{}'", s),
        Tok::DoubleQuoted(_) => "\"...\"".to_string(),
        Tok::LParen => "(".to_string(),
        Tok::RParen => ")".to_string(),
        Tok::DblParen => "((".to_string(),
        Tok::DblParenClose => "))".to_string(),
        Tok::Comment(_) => String::new(),
        _ => String::new(),
    }
}
