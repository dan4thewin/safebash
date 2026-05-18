//! Arithmetic expression parsing: Pratt parser for $(( )) and (( )).
//!
//! Handles binary operators with precedence, unary operators,
//! ternary (? :), and sexpr function calls (name args...).

use crate::ir::*;
use crate::diagnostic::Diagnostic;
use super::lexer::{Scanner, Tok};
use super::expr::span_from;
use super::error::parse_error;

/// Parse arithmetic expansion: content between $(( and ))
/// Returns an Expr with ArithmeticExpansion kind.
pub fn parse_arith_expansion(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    let content = collect_arith_content(scanner)?;
    Ok(Expr {
        kind: ExprKind::ArithmeticExpansion(content.trim().to_string()),
        span,
    })
}

/// Parse an arithmetic command: (( expr )) as a statement.
/// Returns the content string for the transform pipeline to handle.
pub fn parse_arith_command(scanner: &mut Scanner, span: Span) -> Result<String, Diagnostic> {
    collect_arith_content(scanner)
}

/// Collect raw content between the opening (( or $(( and closing )).
/// Respects nesting of (( )) and ( ).
fn collect_arith_content(scanner: &mut Scanner) -> Result<String, Diagnostic> {
    // We need to collect tokens until we find the matching ))
    // But we can't just count parens because the lexer may have split things up.
    // Instead, collect raw source bytes from current position to matching )).
    // Actually, we're working with the token stream. Let's collect text from tokens.
    let mut content = String::new();
    let mut depth = 1i32; // we're inside one level of (( already

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
                // Single ) — check if next is also )
                let sp = tok.span;
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
                // Reconstruct text from token
                let text = tok_to_text(&tok.token);
                content.push_str(&text);
                scanner.next();
                // Add space after if needed
                if !matches!(scanner.peek().token,
                    Tok::DblParenClose | Tok::RParen | Tok::Semi | Tok::Newline | Tok::Eof
                    | Tok::DblParen | Tok::LParen
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
        Tok::DoubleQuoted(_) => "\"...\"".to_string(), // shouldn't appear in arith
        Tok::LParen => "(".to_string(),
        Tok::RParen => ")".to_string(),
        Tok::DblParen => "((".to_string(),
        Tok::DblParenClose => "))".to_string(),
        Tok::Comment(_) => String::new(),
        _ => String::new(),
    }
}

/// Classify an arithmetic expression as mutation vs. comparison/test.
/// Used by the parser to decide how to lower (( expr )).
pub fn classify_arith(content: &str) -> ArithKind {
    let trimmed = content.trim();

    // Check for assignment operators: =, +=, -=, *=, /=, %=
    // But not ==, !=, <=, >=
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    let mut has_comparison = false;
    let mut has_assignment = false;
    let mut has_incdec = false;

    while i < bytes.len() {
        match bytes[i] {
            b'+' if i + 1 < bytes.len() && bytes[i + 1] == b'+' => {
                has_incdec = true;
                i += 2;
            }
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                has_incdec = true;
                i += 2;
            }
            b'+' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                has_assignment = true;
                i += 2;
            }
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                has_assignment = true;
                i += 2;
            }
            b'*' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                has_assignment = true;
                i += 2;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                has_assignment = true;
                i += 2;
            }
            b'%' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                has_assignment = true;
                i += 2;
            }
            b'=' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                has_comparison = true;
                i += 2;
            }
            b'!' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                has_comparison = true;
                i += 2;
            }
            b'<' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                has_comparison = true;
                i += 2;
            }
            b'>' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                has_comparison = true;
                i += 2;
            }
            b'<' => {
                has_comparison = true;
                i += 1;
            }
            b'>' => {
                has_comparison = true;
                i += 1;
            }
            b'=' => {
                has_assignment = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    if has_assignment || has_incdec {
        ArithKind::Mutation
    } else if has_comparison {
        ArithKind::Comparison
    } else {
        // Bare expression like (( x )) or (( 5 )) — used as test
        ArithKind::Test
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArithKind {
    Mutation,    // x=5, x+=3, x++
    Comparison,  // x > 0, x == 5
    Test,        // bare expression (( x ))
}

/// Lower an arithmetic command (( expr )) to an IR Statement.
/// Dispatches based on whether it's mutation, comparison, or bare test.
pub fn lower_arith_command(content: &str, span: Span) -> Statement {
    let trimmed = content.trim();

    // Handle comma operator: a, b, c → Group of individual expressions
    if trimmed.contains(',') && !trimmed.contains(",,") {
        let parts: Vec<&str> = trimmed.split(',').collect();
        if parts.len() > 1 {
            let stmts: Vec<Statement> = parts
                .iter()
                .map(|p| lower_arith_command(p, span))
                .collect();
            return Statement {
                kind: StmtKind::Group(Group { body: stmts }),
                span,
            };
        }
    }

    let kind = classify_arith(trimmed);
    match kind {
        ArithKind::Mutation => lower_arith_mutation(trimmed, span),
        ArithKind::Comparison | ArithKind::Test => lower_arith_test(trimmed, span),
    }
}

fn lower_arith_mutation(content: &str, span: Span) -> Statement {
    let trimmed = content.trim();

    // x++ / x--
    if trimmed.ends_with("++") {
        let var = trimmed[..trimmed.len() - 2].trim();
        return Statement {
            kind: StmtKind::Assignment(Assignment {
                name: var.to_string(),
                index: None,
                op: AssignOp::Set,
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(format!("{} + 1", var)),
                    span,
                },
            }),
            span,
        };
    }
    if trimmed.ends_with("--") {
        let var = trimmed[..trimmed.len() - 2].trim();
        return Statement {
            kind: StmtKind::Assignment(Assignment {
                name: var.to_string(),
                index: None,
                op: AssignOp::Set,
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(format!("{} - 1", var)),
                    span,
                },
            }),
            span,
        };
    }
    // ++x / --x
    if trimmed.starts_with("++") {
        let var = trimmed[2..].trim();
        return Statement {
            kind: StmtKind::Assignment(Assignment {
                name: var.to_string(),
                index: None,
                op: AssignOp::Set,
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(format!("{} + 1", var)),
                    span,
                },
            }),
            span,
        };
    }
    if trimmed.starts_with("--") && !trimmed.starts_with("---") {
        let var = trimmed[2..].trim();
        return Statement {
            kind: StmtKind::Assignment(Assignment {
                name: var.to_string(),
                index: None,
                op: AssignOp::Set,
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(format!("{} - 1", var)),
                    span,
                },
            }),
            span,
        };
    }

    // Compound assignment: x += expr, x -= expr, x *= expr, x /= expr, x %= expr
    for (op_tok, arith_op) in &[("+=", "+"), ("-=", "-"), ("*=", "*"), ("/=", "/"), ("%=", "%")] {
        if let Some(pos) = trimmed.find(op_tok) {
            let var = trimmed[..pos].trim();
            let rhs = trimmed[pos + op_tok.len()..].trim();
            return Statement {
                kind: StmtKind::Assignment(Assignment {
                    name: var.to_string(),
                    index: None,
                    op: AssignOp::Set,
                    value: Expr {
                        kind: ExprKind::ArithmeticExpansion(format!("{} {} {}", var, arith_op, rhs)),
                        span,
                    },
                }),
                span,
            };
        }
    }

    // Simple assignment: x = expr (but not ==)
    if let Some(pos) = find_assignment_eq(trimmed) {
        let var = trimmed[..pos].trim();
        let rhs = trimmed[pos + 1..].trim();
        return Statement {
            kind: StmtKind::Assignment(Assignment {
                name: var.to_string(),
                index: None,
                op: AssignOp::Set,
                value: Expr {
                    kind: ExprKind::ArithmeticExpansion(rhs.to_string()),
                    span,
                },
            }),
            span,
        };
    }

    // Fallback: emit as let command
    Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("let".to_string()),
                span,
            },
            args: vec![Expr {
                kind: ExprKind::Literal(trimmed.to_string()),
                span,
            }],
        }),
        span,
    }
}

fn lower_arith_test(content: &str, span: Span) -> Statement {
    let trimmed = content.trim();

    // Emit as [[ arith_expr -ne 0 ]] test
    let test_val = if trimmed.chars().all(|c| c.is_ascii_digit())
        || (trimmed.starts_with('-') && trimmed[1..].chars().all(|c| c.is_ascii_digit()))
    {
        // Literal number
        ExprKind::Literal(trimmed.to_string())
    } else if trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !trimmed.is_empty()
        && trimmed.as_bytes()[0].is_ascii_alphabetic()
    {
        // Simple variable name
        ExprKind::SimpleExpansion(trimmed.to_string())
    } else {
        // Complex expression — emit as ArithmeticExpansion
        ExprKind::ArithmeticExpansion(trimmed.to_string())
    };

    // For comparisons with explicit operators, emit directly as let
    let kind = classify_arith(trimmed);
    if kind == ArithKind::Comparison {
        return Statement {
            kind: StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("let".to_string()),
                    span,
                },
                args: vec![Expr {
                    kind: ExprKind::Literal(trimmed.to_string()),
                    span,
                }],
            }),
            span,
        };
    }

    Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("[[".to_string()),
                span,
            },
            args: vec![Expr {
                kind: ExprKind::Test(TestExpr::Binary {
                    left: Box::new(Expr {
                        kind: test_val,
                        span,
                    }),
                    op: "-ne".to_string(),
                    right: Box::new(Expr {
                        kind: ExprKind::Literal("0".to_string()),
                        span,
                    }),
                }),
                span,
            }],
        }),
        span,
    }
}

/// Find = that is an assignment (not == or !=, <=, >=, +=, -=, *=, /=, %=)
fn find_assignment_eq(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            // Not preceded by !, <, >, +, -, *, /, %
            let prev_ok = i == 0 || !matches!(bytes[i - 1], b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%');
            // Not followed by =
            let next_ok = i + 1 >= bytes.len() || bytes[i + 1] != b'=';
            if prev_ok && next_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Lower a C-style for loop: for (( init; cond; update )); do body; done
pub fn lower_c_style_for(init: &str, cond: &str, update: &str, body: Vec<Statement>, span: Span) -> Statement {
    // Init
    let mut init_stmts = Vec::new();
    let init_trimmed = init.trim();
    if !init_trimmed.is_empty() {
        init_stmts.push(lower_arith_mutation(init_trimmed, span));
    }

    // Condition → [[ ]]
    let cond_trimmed = cond.trim();
    let condition = if cond_trimmed.is_empty() {
        vec![Statement {
            kind: StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal("true".to_string()),
                    span,
                },
                args: vec![],
            }),
            span,
        }]
    } else {
        vec![lower_arith_cond(cond_trimmed, span)]
    };

    // Update
    let mut loop_body = body;
    let update_trimmed = update.trim();
    if !update_trimmed.is_empty() {
        loop_body.push(lower_arith_mutation(update_trimmed, span));
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

/// Lower an arithmetic condition for C-style for loops.
fn lower_arith_cond(content: &str, span: Span) -> Statement {
    // Parse comparison operators
    let ops = [("<=", "-le"), (">=", "-ge"), ("==", "-eq"), ("!=", "-ne"), ("<", "-lt"), (">", "-gt")];
    for (op_str, test_op) in ops {
        if let Some(pos) = content.find(op_str) {
            // Make sure we don't match <= when looking for <
            if op_str.len() == 1 {
                // Single char op — ensure not part of <=, >=, etc.
                if pos + 1 < content.len() && content.as_bytes()[pos + 1] == b'=' {
                    continue;
                }
                if pos > 0 && (content.as_bytes()[pos - 1] == b'!' || content.as_bytes()[pos - 1] == b'<' || content.as_bytes()[pos - 1] == b'>') {
                    continue;
                }
            }
            let left = content[..pos].trim();
            let right = content[pos + op_str.len()..].trim();
            return Statement {
                kind: StmtKind::Command(Command {
                    name: Expr {
                        kind: ExprKind::Literal("[[".to_string()),
                        span,
                    },
                    args: vec![Expr {
                        kind: ExprKind::Test(TestExpr::Binary {
                            left: Box::new(Expr {
                                kind: ExprKind::SimpleExpansion(left.to_string()),
                                span,
                            }),
                            op: test_op.to_string(),
                            right: Box::new(Expr {
                                kind: ExprKind::Literal(right.to_string()),
                                span,
                            }),
                        }),
                        span,
                    }],
                }),
                span,
            };
        }
    }

    // Fallback: use -ne 0 test
    lower_arith_test(content, span)
}
