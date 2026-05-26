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
            Err(arith_error(
                span,
                format!(
                    "prefix {}{} is not allowed — use postfix ({}{}){}",
                    op,
                    var,
                    var,
                    op,
                    if *inc { " or (( " } else { " or (( " }.to_string()
                        + var
                        + if *inc {
                            " += 1 )) instead"
                        } else {
                            " -= 1 )) instead"
                        }
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
    if parser.peek() == Some(&ATok::Comma) {
        let mut exprs = vec![expr];
        while parser.peek() == Some(&ATok::Comma) {
            parser.advance();
            exprs.push(parser.parse_expr(0)?);
        }
        Ok(ArithExpr::Comma(exprs))
    } else {
        Ok(expr)
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
        ArithExpr::BinOp { op, .. } => match op {
            IrArithOp::Eq
            | IrArithOp::Ne
            | IrArithOp::Lt
            | IrArithOp::Le
            | IrArithOp::Gt
            | IrArithOp::Ge
            | IrArithOp::And
            | IrArithOp::Or => ArithKind::Comparison,
            _ => ArithKind::Test,
        },
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
        // Skip whitespace
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Numbers (decimal, hex, octal, base#digits)
        if bytes[i].is_ascii_digit() {
            let start = i;
            // Hex: 0x...
            if bytes[i] == b'0'
                && i + 1 < bytes.len()
                && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X')
            {
                i += 2;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let s = &input[start..i];
                let val = i64::from_str_radix(&s[2..], 16).unwrap_or(0);
                tokens.push(ATok::Num(val));
            }
            // Octal: 0...
            else if bytes[i] == b'0' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                i += 1;
                while i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                    i += 1;
                }
                let s = &input[start + 1..i];
                let val = i64::from_str_radix(s, 8).unwrap_or(0);
                tokens.push(ATok::Num(val));
            } else {
                // Decimal or base#digits
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'#' {
                    // Base literal: base#digits
                    let base: u32 = input[start..i].parse().unwrap_or(10);
                    i += 1; // skip #
                    let digit_start = i;
                    while i < bytes.len()
                        && (bytes[i].is_ascii_alphanumeric()
                            || bytes[i] == b'_'
                            || bytes[i] == b'@')
                    {
                        i += 1;
                    }
                    let digits = &input[digit_start..i];
                    let val = digits.chars().fold(0i64, |acc, c| {
                        let d = match c {
                            '0'..='9' => c as i64 - '0' as i64,
                            'a'..='z' => c as i64 - 'a' as i64 + 10,
                            'A'..='Z' => c as i64 - 'A' as i64 + 36,
                            '@' => 62,
                            '_' => 63,
                            _ => 0,
                        };
                        acc * base as i64 + d
                    });
                    tokens.push(ATok::Num(val));
                } else {
                    let val: i64 = input[start..i].parse().unwrap_or(0);
                    tokens.push(ATok::Num(val));
                }
            }
            continue;
        }

        // $-prefixed variable
        if bytes[i] == b'$' {
            i += 1;
            let start = i;
            if i < bytes.len() && bytes[i] == b'{' {
                // ${var} — extract name
                i += 1;
                let name_start = i;
                while i < bytes.len() && bytes[i] != b'}' {
                    i += 1;
                }
                let name = input[name_start..i].to_string();
                if i < bytes.len() {
                    i += 1;
                } // skip }
                tokens.push(ATok::DollarVar(name));
            } else {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let name = input[start..i].to_string();
                tokens.push(ATok::DollarVar(name));
            }
            continue;
        }

        // Variable names (alphanumeric + underscore, starting with alpha/_)
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            tokens.push(ATok::Var(input[start..i].to_string()));
            continue;
        }

        // Multi-char operators (try longest first)
        if i + 2 < bytes.len() {
            let three = &input[i..i + 3];
            match three {
                "<<=" => {
                    tokens.push(ATok::LtLtEq);
                    i += 3;
                    continue;
                }
                ">>=" => {
                    tokens.push(ATok::GtGtEq);
                    i += 3;
                    continue;
                }
                _ => {}
            }
        }
        if i + 1 < bytes.len() {
            let two = &input[i..i + 2];
            match two {
                "++" => {
                    tokens.push(ATok::PlusPlus);
                    i += 2;
                    continue;
                }
                "--" => {
                    tokens.push(ATok::MinusMinus);
                    i += 2;
                    continue;
                }
                "**" => {
                    tokens.push(ATok::StarStar);
                    i += 2;
                    continue;
                }
                "<<" => {
                    tokens.push(ATok::LtLt);
                    i += 2;
                    continue;
                }
                ">>" => {
                    tokens.push(ATok::GtGt);
                    i += 2;
                    continue;
                }
                "<=" => {
                    tokens.push(ATok::LtEq);
                    i += 2;
                    continue;
                }
                ">=" => {
                    tokens.push(ATok::GtEq);
                    i += 2;
                    continue;
                }
                "==" => {
                    tokens.push(ATok::EqEq);
                    i += 2;
                    continue;
                }
                "!=" => {
                    tokens.push(ATok::BangEq);
                    i += 2;
                    continue;
                }
                "&&" => {
                    tokens.push(ATok::AmpAmp);
                    i += 2;
                    continue;
                }
                "||" => {
                    tokens.push(ATok::PipePipe);
                    i += 2;
                    continue;
                }
                "+=" => {
                    tokens.push(ATok::PlusEq);
                    i += 2;
                    continue;
                }
                "-=" => {
                    tokens.push(ATok::MinusEq);
                    i += 2;
                    continue;
                }
                "*=" => {
                    tokens.push(ATok::StarEq);
                    i += 2;
                    continue;
                }
                "/=" => {
                    tokens.push(ATok::SlashEq);
                    i += 2;
                    continue;
                }
                "%=" => {
                    tokens.push(ATok::PercentEq);
                    i += 2;
                    continue;
                }
                "&=" => {
                    tokens.push(ATok::AmpEq);
                    i += 2;
                    continue;
                }
                "|=" => {
                    tokens.push(ATok::PipeEq);
                    i += 2;
                    continue;
                }
                "^=" => {
                    tokens.push(ATok::CaretEq);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        // Single-char operators
        let tok = match bytes[i] {
            b'+' => ATok::Plus,
            b'-' => ATok::Minus,
            b'*' => ATok::Star,
            b'/' => ATok::Slash,
            b'%' => ATok::Percent,
            b'<' => ATok::Lt,
            b'>' => ATok::Gt,
            b'&' => ATok::Amp,
            b'|' => ATok::Pipe,
            b'^' => ATok::Caret,
            b'~' => ATok::Tilde,
            b'!' => ATok::Bang,
            b'=' => ATok::Eq,
            b'?' => ATok::Question,
            b':' => ATok::Colon,
            b',' => ATok::Comma,
            b'(' => ATok::LParen,
            b')' => ATok::RParen,
            _ => {
                i += 1;
                continue;
            } // skip unknown
        };
        tokens.push(tok);
        i += 1;
    }

    tokens
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
        // Prefix / atom
        let mut lhs = self.parse_prefix()?;

        loop {
            // Postfix: ++ and --
            if let Some(tok) = self.peek() {
                match tok {
                    ATok::PlusPlus => {
                        if let ArithExpr::Var(name) = &lhs {
                            let name = name.clone();
                            self.advance();
                            lhs = ArithExpr::PostIncDec {
                                var: name,
                                inc: true,
                            };
                            continue;
                        }
                        // Not a variable, treat ++ as end of expression
                        break;
                    }
                    ATok::MinusMinus => {
                        if let ArithExpr::Var(name) = &lhs {
                            let name = name.clone();
                            self.advance();
                            lhs = ArithExpr::PostIncDec {
                                var: name,
                                inc: false,
                            };
                            continue;
                        }
                        break;
                    }
                    _ => {}
                }
            }

            // Assignment operators (right-associative, lowest precedence above comma)
            if let Some(tok) = self.peek() {
                if let ArithExpr::Var(name) = &lhs {
                    let assign_op = match tok {
                        ATok::Eq => Some(None),
                        ATok::PlusEq => Some(Some(IrArithOp::Add)),
                        ATok::MinusEq => Some(Some(IrArithOp::Sub)),
                        ATok::StarEq => Some(Some(IrArithOp::Mul)),
                        ATok::SlashEq => Some(Some(IrArithOp::Div)),
                        ATok::PercentEq => Some(Some(IrArithOp::Mod)),
                        ATok::AmpEq => Some(Some(IrArithOp::BitAnd)),
                        ATok::PipeEq => Some(Some(IrArithOp::BitOr)),
                        ATok::CaretEq => Some(Some(IrArithOp::BitXor)),
                        ATok::LtLtEq => Some(Some(IrArithOp::Shl)),
                        ATok::GtGtEq => Some(Some(IrArithOp::Shr)),
                        _ => None,
                    };
                    if let Some(op) = assign_op {
                        let name = name.clone();
                        self.advance();
                        let rhs = self.parse_expr(1)?; // right-associative
                        lhs = match op {
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
                        continue;
                    }
                }
            }

            // Ternary: ? : (binding power 2)
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

            // Binary operators
            let (op, bp) = match self.peek() {
                Some(ATok::PipePipe) => (IrArithOp::Or, (3, 4)),
                Some(ATok::AmpAmp) => (IrArithOp::And, (5, 6)),
                Some(ATok::Pipe) => (IrArithOp::BitOr, (7, 8)),
                Some(ATok::Caret) => (IrArithOp::BitXor, (9, 10)),
                Some(ATok::Amp) => (IrArithOp::BitAnd, (11, 12)),
                Some(ATok::EqEq) => (IrArithOp::Eq, (13, 14)),
                Some(ATok::BangEq) => (IrArithOp::Ne, (13, 14)),
                Some(ATok::Lt) => (IrArithOp::Lt, (15, 16)),
                Some(ATok::LtEq) => (IrArithOp::Le, (15, 16)),
                Some(ATok::Gt) => (IrArithOp::Gt, (15, 16)),
                Some(ATok::GtEq) => (IrArithOp::Ge, (15, 16)),
                Some(ATok::LtLt) => (IrArithOp::Shl, (17, 18)),
                Some(ATok::GtGt) => (IrArithOp::Shr, (17, 18)),
                Some(ATok::Plus) => (IrArithOp::Add, (19, 20)),
                Some(ATok::Minus) => (IrArithOp::Sub, (19, 20)),
                Some(ATok::Star) => (IrArithOp::Mul, (21, 22)),
                Some(ATok::Slash) => (IrArithOp::Div, (21, 22)),
                Some(ATok::Percent) => (IrArithOp::Mod, (21, 22)),
                Some(ATok::StarStar) => (IrArithOp::Pow, (24, 23)), // right-associative
                _ => break,
            };

            let (l_bp, r_bp) = bp;
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

    /// Parse prefix expressions and atoms.
    fn parse_prefix(&mut self) -> Result<ArithExpr, Diagnostic> {
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
            Some(ATok::PlusPlus) => {
                self.advance();
                if let Some(ATok::Var(name)) = self.peek().cloned() {
                    self.advance();
                    Ok(ArithExpr::PreIncDec {
                        var: name,
                        inc: true,
                    })
                } else {
                    Err(arith_error(
                        self.span,
                        "expected variable after ++".to_string(),
                    ))
                }
            }
            Some(ATok::MinusMinus) => {
                self.advance();
                if let Some(ATok::Var(name)) = self.peek().cloned() {
                    self.advance();
                    Ok(ArithExpr::PreIncDec {
                        var: name,
                        inc: false,
                    })
                } else {
                    Err(arith_error(
                        self.span,
                        "expected variable after --".to_string(),
                    ))
                }
            }
            Some(ATok::Minus) => {
                self.advance();
                let operand = self.parse_expr(25)?; // unary binds tighter than binary
                Ok(ArithExpr::UnaryOp {
                    op: IrUnaryArithOp::Neg,
                    operand: Box::new(operand),
                })
            }
            Some(ATok::Plus) => {
                self.advance();
                let operand = self.parse_expr(25)?;
                Ok(ArithExpr::UnaryOp {
                    op: IrUnaryArithOp::Pos,
                    operand: Box::new(operand),
                })
            }
            Some(ATok::Bang) => {
                self.advance();
                let operand = self.parse_expr(25)?;
                Ok(ArithExpr::UnaryOp {
                    op: IrUnaryArithOp::Not,
                    operand: Box::new(operand),
                })
            }
            Some(ATok::Tilde) => {
                self.advance();
                let operand = self.parse_expr(25)?;
                Ok(ArithExpr::UnaryOp {
                    op: IrUnaryArithOp::BitNot,
                    operand: Box::new(operand),
                })
            }
            Some(ATok::LParen) => {
                self.advance();
                // DD#58: Check if this is a sexpr function call: (name args...)
                if let Some(ATok::Var(name)) = self.peek().cloned() {
                    // Could be (name ...) sexpr or (expr) grouping
                    // Peek ahead: if next after name is a number, var, $var, or (
                    // then it's a function call. If it's an operator or ), it's grouping.
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
            Some(_) => {
                // Unknown token — try to recover
                self.advance();
                Ok(ArithExpr::Literal(0))
            }
            None => Ok(ArithExpr::Literal(0)),
        }
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

/// Stringify an ArithExpr back to text (for compatibility with transform's
/// text-based arithmetic handling in let commands and conditions).
fn arith_expr_to_text(expr: &ArithExpr) -> String {
    match expr {
        ArithExpr::Literal(n) => n.to_string(),
        ArithExpr::Var(name) => name.clone(),
        ArithExpr::DollarVar(name) => format!("${}", name),
        ArithExpr::BinOp { left, op, right } => {
            let op_str = match op {
                IrArithOp::Add => "+",
                IrArithOp::Sub => "-",
                IrArithOp::Mul => "*",
                IrArithOp::Div => "/",
                IrArithOp::Mod => "%",
                IrArithOp::Pow => "**",
                IrArithOp::Eq => "==",
                IrArithOp::Ne => "!=",
                IrArithOp::Lt => "<",
                IrArithOp::Le => "<=",
                IrArithOp::Gt => ">",
                IrArithOp::Ge => ">=",
                IrArithOp::And => "&&",
                IrArithOp::Or => "||",
                IrArithOp::Shl => "<<",
                IrArithOp::Shr => ">>",
                IrArithOp::BitAnd => "&",
                IrArithOp::BitOr => "|",
                IrArithOp::BitXor => "^",
            };
            format!(
                "{} {} {}",
                arith_expr_to_text(left),
                op_str,
                arith_expr_to_text(right)
            )
        }
        ArithExpr::UnaryOp { op, operand } => {
            let op_str = match op {
                IrUnaryArithOp::Neg => "-",
                IrUnaryArithOp::Pos => "+",
                IrUnaryArithOp::Not => "!",
                IrUnaryArithOp::BitNot => "~",
            };
            format!("{}{}", op_str, arith_expr_to_text(operand))
        }
        ArithExpr::PreIncDec { var, inc } => {
            format!("{}{}", if *inc { "++" } else { "--" }, var)
        }
        ArithExpr::PostIncDec { var, inc } => {
            format!("{}{}", var, if *inc { "++" } else { "--" })
        }
        ArithExpr::Assign { var, value } => {
            format!("{} = {}", var, arith_expr_to_text(value))
        }
        ArithExpr::CompoundAssign { var, op, value } => {
            let op_str = match op {
                IrArithOp::Add => "+=",
                IrArithOp::Sub => "-=",
                IrArithOp::Mul => "*=",
                IrArithOp::Div => "/=",
                IrArithOp::Mod => "%=",
                _ => "?=",
            };
            format!("{} {} {}", var, op_str, arith_expr_to_text(value))
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            format!(
                "{} ? {} : {}",
                arith_expr_to_text(cond),
                arith_expr_to_text(true_val),
                arith_expr_to_text(false_val)
            )
        }
        ArithExpr::Comma(exprs) => exprs
            .iter()
            .map(arith_expr_to_text)
            .collect::<Vec<_>>()
            .join(", "),
        ArithExpr::FuncCall { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(arith_expr_to_text).collect();
            format!("({} {})", name, arg_strs.join(" "))
        }
        ArithExpr::Paren(inner) => format!("({})", arith_expr_to_text(inner)),
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

    loop {
        if scanner.at_eof() {
            break;
        }
        let tok = scanner.peek().clone();
        match &tok.token {
            Tok::DblParenClose => {
                depth -= 1;
                if depth == 0 {
                    scanner.next();
                    break;
                }
                content.push_str("))");
                scanner.next();
            }
            Tok::DblParen => {
                depth += 1;
                content.push_str("((");
                scanner.next();
            }
            Tok::DollarDblParen => {
                depth += 1;
                content.push_str("$((");
                scanner.next();
            }
            Tok::LParen => {
                content.push('(');
                scanner.next();
            }
            Tok::RParen => {
                scanner.next();
                if matches!(scanner.peek().token, Tok::RParen) {
                    depth -= 1;
                    scanner.next();
                    if depth == 0 {
                        break;
                    }
                    content.push_str("))");
                } else {
                    content.push(')');
                }
            }
            _ => {
                let text = tok_to_text(&tok.token);
                content.push_str(&text);
                scanner.next();
                if !matches!(
                    scanner.peek().token,
                    Tok::DblParenClose
                        | Tok::RParen
                        | Tok::Semi
                        | Tok::Newline
                        | Tok::Eof
                        | Tok::DblParen
                        | Tok::LParen
                ) && !text.ends_with(' ')
                {
                    content.push(' ');
                }
            }
        }
    }

    Ok(content)
}

/// Convert a token back to source text for arithmetic content collection.
fn tok_to_text(tok: &Tok) -> String {
    match tok {
        Tok::Word(w) => w.clone(),
        Tok::Number(n) => n.clone(),
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
