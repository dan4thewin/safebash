//! Expression parsing: words, strings, parameter expansions, command substitution.

use crate::ir::*;
use super::lexer::{Scanner, Tok, StringPart};
use super::error::parse_error;
use crate::diagnostic::Diagnostic;

/// Parse a single word/expression that produces a value.
/// This handles: literals, $var, ${var...}, "...", '...', $(...), $((...)),
/// concatenation of adjacent tokens, and array literals.
pub fn parse_expr(scanner: &mut Scanner) -> Result<Expr, Diagnostic> {
    let mut parts: Vec<Expr> = Vec::new();

    loop {
        // Only continue concatenating if next token is adjacent (no whitespace)
        if !parts.is_empty() && !scanner.next_is_adjacent() {
            break;
        }
        let start = scanner.peek().span.start;
        let tok = scanner.peek().token.clone();
        let span = span_from(scanner, start);

        match tok {
            Tok::Word(ref w) => {
                let w = w.clone();
                scanner.next();
                // Handle tilde expansion
                if w == "~" {
                    parts.push(Expr {
                        kind: ExprKind::SimpleExpansion("HOME".to_string()),
                        span,
                    });
                } else if w.starts_with("~/") {
                    parts.push(Expr {
                        kind: ExprKind::Concat(vec![
                            Expr {
                                kind: ExprKind::SimpleExpansion("HOME".to_string()),
                                span,
                            },
                            Expr {
                                kind: ExprKind::Literal(w[1..].to_string()),
                                span,
                            },
                        ]),
                        span,
                    });
                } else if w.starts_with('@') && w.len() > 1 {
                    parts.push(Expr {
                        kind: ExprKind::ParameterExpansion(ParameterExpansion {
                            name: w[1..].to_string(),
                            operator: Some("arrvals".to_string()),
                            operand: None,
                        }),
                        span,
                    });
                } else {
                    parts.push(Expr {
                        kind: ExprKind::Literal(w),
                        span,
                    });
                }
            }
            Tok::Number(ref n) => {
                let n = n.clone();
                scanner.next();
                parts.push(Expr {
                    kind: ExprKind::Literal(n),
                    span,
                });
            }
            Tok::SingleQuoted(ref s) => {
                let s = s.clone();
                scanner.next();
                parts.push(Expr {
                    kind: ExprKind::SingleQuoted(s),
                    span,
                });
            }
            Tok::DoubleQuoted(ref parts_tok) => {
                let parts_tok = parts_tok.clone();
                scanner.next();
                let dq_parts = string_parts_to_exprs(&parts_tok, span, scanner);
                parts.push(Expr {
                    kind: ExprKind::DoubleQuoted(dq_parts),
                    span,
                });
            }
            Tok::AnsiCQuoted(ref s) => {
                let s = s.clone();
                scanner.next();
                parts.push(Expr {
                    kind: ExprKind::Literal(s),
                    span,
                });
            }
            Tok::Dollar(ref name) => {
                let name = name.clone();
                scanner.next();
                // Check for $arr[i] subscript syntax (DD#41)
                let next_tok = scanner.peek().token.clone();
                if let Tok::Word(ref w) = next_tok {
                    if w.starts_with('[') {
                        if let Some(close) = w.find(']') {
                            let idx = w[1..close].to_string();
                            let rest = w[close + 1..].to_string();
                            scanner.next();
                            if idx == "@" || idx == "*" {
                                parts.push(Expr {
                                    kind: ExprKind::ParameterExpansion(ParameterExpansion {
                                        name: name.clone(),
                                        operator: Some("arrvals".to_string()),
                                        operand: None,
                                    }),
                                    span,
                                });
                            } else {
                                parts.push(Expr {
                                    kind: ExprKind::Subscript {
                                        name: name.clone(),
                                        index: Box::new(Expr {
                                            kind: ExprKind::Literal(idx),
                                            span,
                                        }),
                                    },
                                    span,
                                });
                            }
                            if !rest.is_empty() {
                                parts.push(Expr {
                                    kind: ExprKind::Literal(rest),
                                    span,
                                });
                            }
                            continue;
                        }
                    }
                }
                // DD#24: $LINENO → compile-time literal line number
                if name == "LINENO" {
                    let (line, _) = scanner.line_col(start);
                    parts.push(Expr {
                        kind: ExprKind::Literal(line.to_string()),
                        span,
                    });
                } else {
                    parts.push(Expr {
                        kind: ExprKind::SimpleExpansion(name),
                        span,
                    });
                }
            }
            Tok::DollarBrace(ref content) => {
                let content = content.clone();
                scanner.next();
                parts.push(parse_parameter_expansion(&content, span));
            }
            Tok::DollarParen => {
                scanner.next();
                parts.push(parse_command_substitution(scanner, span)?);
            }
            Tok::DollarDblParen => {
                scanner.next();
                let arith = super::arith::parse_arith_expansion(scanner, span)?;
                parts.push(arith);
            }
            Tok::Eq => {
                // = in expression context (e.g., FOO=bar as command arg)
                scanner.next();
                parts.push(Expr {
                    kind: ExprKind::Literal("=".to_string()),
                    span,
                });
            }
            // Keywords that can appear as literal arguments
            Tok::If | Tok::Then | Tok::Else | Tok::Elif | Tok::Fi
            | Tok::While | Tok::Until | Tok::Do | Tok::Done
            | Tok::For | Tok::In | Tok::Case | Tok::Esac
            | Tok::Fn | Tok::Function | Tok::Local
            | Tok::Int | Tok::Const | Tok::Array | Tok::Map
            | Tok::Bless | Tok::End => {
                let text = keyword_to_str(&tok);
                scanner.next();
                parts.push(Expr {
                    kind: ExprKind::Literal(text.to_string()),
                    span,
                });
            }
            Tok::Dash => {
                scanner.next();
                parts.push(Expr {
                    kind: ExprKind::Literal("-".to_string()),
                    span,
                });
            }
            _ => break,
        }
    }

    match parts.len() {
        0 => Err(parse_error(
            Span { line: 1, col: 1 },
            "SB4014",
            "expected expression".to_string(),
        )),
        1 => Ok(parts.into_iter().next().unwrap()),
        _ => {
            let span = parts[0].span;
            Ok(Expr {
                kind: ExprKind::Concat(parts),
                span,
            })
        }
    }
}

/// Parse a word in a context where it won't be concatenated.
pub fn parse_word(scanner: &mut Scanner) -> Result<Expr, Diagnostic> {
    parse_expr(scanner)
}

/// Convert lexer StringParts to IR Exprs for double-quoted strings.
pub fn string_parts_to_exprs(parts: &[StringPart], span: Span, scanner: &Scanner) -> Vec<Expr> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        match &parts[i] {
            StringPart::Literal(s) => {
                result.push(Expr {
                    kind: ExprKind::Literal(s.clone()),
                    span,
                });
            }
            StringPart::Variable(name) => {
                // Check for $arr[i] inside strings (DD#41)
                if i + 1 < parts.len() {
                    if let StringPart::Literal(lit) = &parts[i + 1] {
                        if lit.starts_with('[') {
                            if let Some(close) = lit.find(']') {
                                let idx = &lit[1..close];
                                let rest = &lit[close + 1..];
                                if idx == "@" || idx == "*" {
                                    result.push(Expr {
                                        kind: ExprKind::ParameterExpansion(ParameterExpansion {
                                            name: name.clone(),
                                            operator: Some("arrvals".to_string()),
                                            operand: None,
                                        }),
                                        span,
                                    });
                                } else {
                                    result.push(Expr {
                                        kind: ExprKind::Subscript {
                                            name: name.clone(),
                                            index: Box::new(Expr {
                                                kind: ExprKind::Literal(idx.to_string()),
                                                span,
                                            }),
                                        },
                                        span,
                                    });
                                }
                                if !rest.is_empty() {
                                    result.push(Expr {
                                        kind: ExprKind::Literal(rest.to_string()),
                                        span,
                                    });
                                }
                                i += 2;
                                continue;
                            }
                        }
                    }
                }
                result.push(Expr {
                    kind: ExprKind::SimpleExpansion(name.clone()),
                    span,
                });
            }
            StringPart::Expansion(content) => {
                result.push(parse_parameter_expansion(content, span));
            }
            StringPart::CommandSub(content) => {
                if content.starts_with('`') {
                    result.push(Expr {
                        kind: ExprKind::Literal(content.clone()),
                        span,
                    });
                } else {
                    let stmts = parse_sub_script(content, span);
                    result.push(Expr {
                        kind: ExprKind::CommandSubstitution(stmts),
                        span,
                    });
                }
            }
            StringPart::ArithSub(content) => {
                result.push(Expr {
                    kind: ExprKind::ArithmeticExpansion(content.trim().to_string()),
                    span,
                });
            }
        }
        i += 1;
    }
    result
}

/// Parse ${...} content into an IR ParameterExpansion.
pub fn parse_parameter_expansion(content: &str, span: Span) -> Expr {
    let bytes = content.as_bytes();

    // ${#var} — string length
    if bytes.first() == Some(&b'#') && bytes.len() > 1 && bytes[1] != b'#' {
        let rest = &content[1..];
        if rest.contains("[@]") || rest.contains("[*]") {
            let name = rest.split('[').next().unwrap_or(rest);
            return Expr {
                kind: ExprKind::ParameterExpansion(ParameterExpansion {
                    name: name.to_string(),
                    operator: Some("arrlen".to_string()),
                    operand: None,
                }),
                span,
            };
        }
        return Expr {
            kind: ExprKind::ParameterExpansion(ParameterExpansion {
                name: rest.to_string(),
                operator: Some("strlen".to_string()),
                operand: None,
            }),
            span,
        };
    }

    // ${!var[@]} — all keys
    if bytes.first() == Some(&b'!') {
        let rest = &content[1..];
        if rest.contains("[@]") || rest.contains("[*]") {
            let name = rest.split('[').next().unwrap_or(rest);
            return Expr {
                kind: ExprKind::ParameterExpansion(ParameterExpansion {
                    name: name.to_string(),
                    operator: Some("arrkeys".to_string()),
                    operand: None,
                }),
                span,
            };
        }
        return Expr {
            kind: ExprKind::ParameterExpansion(ParameterExpansion {
                name: rest.to_string(),
                operator: Some("!".to_string()),
                operand: None,
            }),
            span,
        };
    }

    // Find the variable name
    let mut name_end = 0;
    while name_end < bytes.len()
        && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
    {
        name_end += 1;
    }
    let name = &content[..name_end];

    if name_end >= bytes.len() {
        return Expr {
            kind: ExprKind::ParameterExpansion(ParameterExpansion {
                name: name.to_string(),
                operator: None,
                operand: None,
            }),
            span,
        };
    }

    // ${var[...]} — subscript
    if bytes[name_end] == b'[' {
        if let Some(close) = content[name_end..].find(']') {
            let idx = &content[name_end + 1..name_end + close];
            let after_bracket = &content[name_end + close + 1..];

            if idx == "@" || idx == "*" {
                if !after_bracket.is_empty() {
                    if let Some(op_result) = parse_subscript_operator(name, after_bracket, span) {
                        return op_result;
                    }
                }
                return Expr {
                    kind: ExprKind::ParameterExpansion(ParameterExpansion {
                        name: name.to_string(),
                        operator: Some("arrvals".to_string()),
                        operand: None,
                    }),
                    span,
                };
            }

            return Expr {
                kind: ExprKind::Subscript {
                    name: name.to_string(),
                    index: Box::new(Expr {
                        kind: ExprKind::Literal(idx.to_string()),
                        span,
                    }),
                },
                span,
            };
        }
    }

    let rest = &content[name_end..];

    // ${var:s/regex/replacement/flags} — DD#39 sed expression
    if rest.starts_with(":s/") {
        let sed_content = &rest[3..];
        if let Some(slash_pos) = sed_content.find('/') {
            let pattern = &sed_content[..slash_pos];
            let after = &sed_content[slash_pos + 1..];
            let (replacement, flags) = if let Some(flag_slash) = after.rfind('/') {
                (&after[..flag_slash], &after[flag_slash + 1..])
            } else {
                (after, "")
            };
            let is_global = flags.contains('g');
            let combined = format!("{}\0{}", pattern, replacement);
            let op = if is_global { "//" } else { "/" };
            return Expr {
                kind: ExprKind::ParameterExpansion(ParameterExpansion {
                    name: name.to_string(),
                    operator: Some(op.to_string()),
                    operand: Some(Box::new(Expr {
                        kind: ExprKind::Literal(combined),
                        span,
                    })),
                }),
                span,
            };
        }
    }

    // ${var:offset} or ${var:offset:length} — substring
    if rest.starts_with(':') && !rest.starts_with(":-") && !rest.starts_with(":+")
        && !rest.starts_with(":=") && !rest.starts_with(":?")
    {
        let sub_content = &rest[1..];
        let first_ch = sub_content.as_bytes().first().copied().unwrap_or(0);
        if first_ch.is_ascii_digit() || first_ch == b'-' || first_ch == b'(' {
            let cleaned = sub_content.replace('(', "").replace(')', "");
            return Expr {
                kind: ExprKind::ParameterExpansion(ParameterExpansion {
                    name: name.to_string(),
                    operator: Some("substring".to_string()),
                    operand: Some(Box::new(Expr {
                        kind: ExprKind::Literal(cleaned),
                        span,
                    })),
                }),
                span,
            };
        }
    }

    // ${var@X} — parameter transform (rejected SB4015)
    if let Some(at_pos) = rest.rfind('@') {
        let after = &rest[at_pos + 1..];
        if after.len() == 1 && after.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            if !rest.contains("[@]") && !rest.contains("[@") {
                return Expr {
                    kind: ExprKind::ParameterExpansion(ParameterExpansion {
                        name: name.to_string(),
                        operator: Some(format!("@{}", after)),
                        operand: None,
                    }),
                    span,
                };
            }
        }
    }

    // Standard operators
    let (op, operand_str) = split_operator(rest);
    if let Some(op_str) = op {
        let operand = if (op_str == "/" || op_str == "//" || op_str == "/#" || op_str == "/%")
            && operand_str.is_some()
        {
            let raw = operand_str.unwrap();
            if let Some(sep) = find_unescaped_slash(&raw) {
                let pattern = &raw[..sep];
                let replacement = &raw[sep + 1..];
                let mut pat_regex = glob_to_regex(pattern);
                if op_str == "/#" {
                    pat_regex = format!("^{}", pat_regex);
                }
                if op_str == "/%" {
                    pat_regex = format!("{}$", pat_regex);
                }
                Some(Box::new(Expr {
                    kind: ExprKind::Literal(format!("{}\0{}", pat_regex, replacement)),
                    span,
                }))
            } else {
                let pat_regex = glob_to_regex(&raw);
                Some(Box::new(Expr {
                    kind: ExprKind::Literal(format!("{}\0", pat_regex)),
                    span,
                }))
            }
        } else {
            operand_str.map(|s| {
                Box::new(Expr {
                    kind: ExprKind::Literal(s),
                    span,
                })
            })
        };
        return Expr {
            kind: ExprKind::ParameterExpansion(ParameterExpansion {
                name: name.to_string(),
                operator: Some(op_str),
                operand,
            }),
            span,
        };
    }

    Expr {
        kind: ExprKind::ParameterExpansion(ParameterExpansion {
            name: name.to_string(),
            operator: None,
            operand: None,
        }),
        span,
    }
}

fn parse_subscript_operator(name: &str, after: &str, span: Span) -> Option<Expr> {
    let (op, operand_str) = split_operator(after);
    if let Some(op_str) = op {
        if matches!(
            op_str.as_str(),
            "/" | "//" | "#" | "##" | "%" | "%%" | "^" | "^^" | "," | ",,"
        ) {
            let operand = if (op_str == "/" || op_str == "//") && operand_str.is_some() {
                let raw = operand_str.unwrap();
                if let Some(sep) = find_unescaped_slash(&raw) {
                    let pattern = &raw[..sep];
                    let replacement = &raw[sep + 1..];
                    let pat_regex = glob_to_regex(pattern);
                    Some(Box::new(Expr {
                        kind: ExprKind::Literal(format!("{}\0{}", pat_regex, replacement)),
                        span,
                    }))
                } else {
                    let pat_regex = glob_to_regex(&raw);
                    Some(Box::new(Expr {
                        kind: ExprKind::Literal(format!("{}\0", pat_regex)),
                        span,
                    }))
                }
            } else {
                operand_str.map(|s| {
                    Box::new(Expr {
                        kind: ExprKind::Literal(s),
                        span,
                    })
                })
            };
            return Some(Expr {
                kind: ExprKind::ParameterExpansion(ParameterExpansion {
                    name: name.to_string(),
                    operator: Some(format!("arrvals_{}", op_str)),
                    operand,
                }),
                span,
            });
        }
    }
    None
}

fn split_operator(s: &str) -> (Option<String>, Option<String>) {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return (None, None);
    }

    let two_char_ops = &[":-", ":=", ":+", ":?", "##", "%%", "//", "/#", "/%", "^^", ",,"];
    for op in two_char_ops {
        if s.starts_with(op) {
            let rest = &s[op.len()..];
            let operand = if rest.is_empty() { None } else { Some(rest.to_string()) };
            return (Some(op.to_string()), operand);
        }
    }

    let one_char_ops = &["%", "#", "/", "^", ","];
    for op in one_char_ops {
        if s.starts_with(op) {
            let rest = &s[op.len()..];
            let operand = if rest.is_empty() { None } else { Some(rest.to_string()) };
            return (Some(op.to_string()), operand);
        }
    }

    (None, None)
}

fn find_unescaped_slash(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
        } else if bytes[i] == b'/' {
            return Some(i);
        } else {
            i += 1;
        }
    }
    None
}

fn glob_to_regex(glob: &str) -> String {
    let mut re = String::with_capacity(glob.len() * 2);
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => re.push_str(".*"),
            '?' => re.push('.'),
            '\\' => {
                if let Some(next) = chars.next() {
                    re.push('\\');
                    re.push(next);
                }
            }
            '.' | '+' | '(' | ')' | '|' | '^' | '$' => {
                re.push('\\');
                re.push(c);
            }
            _ => re.push(c),
        }
    }
    re
}

/// Parse a sub-script (for command substitution or subshell contents).
pub fn parse_sub_script(content: &str, span: Span) -> Vec<Statement> {
    let mut sub_scanner = Scanner::new(content);
    let mut stmts = Vec::new();
    while !sub_scanner.at_eof() {
        while matches!(sub_scanner.peek().token, Tok::Newline | Tok::Semi) {
            sub_scanner.next();
        }
        if sub_scanner.at_eof() {
            break;
        }
        match super::parser::parse_statement(&mut sub_scanner) {
            Ok(Some(stmt)) => stmts.push(stmt),
            Ok(None) => break,
            Err(_) => break,
        }
    }
    stmts
}

/// Parse a command substitution after the opening $(
pub fn parse_command_substitution(
    scanner: &mut Scanner,
    span: Span,
) -> Result<Expr, Diagnostic> {
    // Check for $(<file) file slurp
    if matches!(scanner.peek().token, Tok::Lt) {
        scanner.next();
        let file_expr = parse_expr(scanner)?;
        expect_rparen(scanner, span)?;
        return Ok(Expr {
            kind: ExprKind::CommandSubstitution(vec![Statement {
                kind: StmtKind::Command(Command {
                    name: Expr {
                        kind: ExprKind::Literal("cat".to_string()),
                        span: Span { line: 0, col: 0 },
                    },
                    args: vec![file_expr],
                }),
                span: Span { line: 0, col: 0 },
            }]),
            span,
        });
    }

    // Collect statements until matching )
    let mut stmts = Vec::new();

    loop {
        if scanner.at_eof() {
            break;
        }
        let tok = scanner.peek().token.clone();
        match tok {
            Tok::RParen => {
                scanner.next();
                break;
            }
            Tok::Newline | Tok::Semi => {
                scanner.next();
            }
            _ => {
                // Check for special single-word commands
                if stmts.is_empty() {
                    if let Tok::Word(ref w) = tok {
                        match w.as_str() {
                            "status" => {
                                scanner.next();
                                if matches!(scanner.peek().token, Tok::RParen) {
                                    scanner.next();
                                    return Ok(Expr {
                                        kind: ExprKind::SimpleExpansion("?".to_string()),
                                        span,
                                    });
                                }
                                // Reconstruct as command
                                let mut args = Vec::new();
                                while !scanner.at_eof()
                                    && !matches!(scanner.peek().token, Tok::RParen | Tok::Newline | Tok::Semi | Tok::Pipe | Tok::AndAnd | Tok::OrOr)
                                {
                                    args.push(parse_expr(scanner)?);
                                }
                                stmts.push(Statement {
                                    kind: StmtKind::Command(Command {
                                        name: Expr {
                                            kind: ExprKind::Literal("status".to_string()),
                                            span,
                                        },
                                        args,
                                    }),
                                    span,
                                });
                                continue;
                            }
                            "getcpid" => {
                                scanner.next();
                                if matches!(scanner.peek().token, Tok::RParen) {
                                    scanner.next();
                                    return Ok(Expr {
                                        kind: ExprKind::SimpleExpansion("!".to_string()),
                                        span,
                                    });
                                }
                                stmts.push(Statement {
                                    kind: StmtKind::Command(Command {
                                        name: Expr {
                                            kind: ExprKind::Literal("getcpid".to_string()),
                                            span,
                                        },
                                        args: vec![],
                                    }),
                                    span,
                                });
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
                match super::parser::parse_statement(scanner)? {
                    Some(stmt) => stmts.push(stmt),
                    None => break,
                }
            }
        }
    }

    Ok(Expr {
        kind: ExprKind::CommandSubstitution(stmts),
        span,
    })
}

fn expect_rparen(scanner: &mut Scanner, span: Span) -> Result<(), Diagnostic> {
    if matches!(scanner.peek().token, Tok::RParen) {
        scanner.next();
        Ok(())
    } else {
        Err(parse_error(span, "SB4014", "expected ')'".to_string()))
    }
}

/// Parse an array literal after = in assignment: =(a b c) or =([k1]=v1 [k2]=v2)
pub fn parse_array_literal(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    let mut elems = Vec::new();
    let mut assoc_pairs: Vec<(String, Expr)> = Vec::new();
    let mut is_assoc = false;

    loop {
        while matches!(scanner.peek().token, Tok::Newline) {
            scanner.next();
        }
        if matches!(scanner.peek().token, Tok::RParen) {
            scanner.next();
            break;
        }
        if scanner.at_eof() {
            break;
        }

        let tok = scanner.peek().token.clone();
        if let Tok::Word(ref w) = tok {
            if w.starts_with('[') {
                // Check for [key]=value (old format with = inside word)
                if let Some(close) = w.find("]=") {
                    is_assoc = true;
                    let key = w[1..close].to_string();
                    let val_str = w[close + 2..].to_string();
                    scanner.next();
                    let val = if val_str.is_empty() {
                        parse_expr(scanner)?
                    } else {
                        let val_str = if (val_str.starts_with('"') && val_str.ends_with('"'))
                            || (val_str.starts_with('\'') && val_str.ends_with('\''))
                        {
                            val_str[1..val_str.len() - 1].to_string()
                        } else {
                            val_str
                        };
                        Expr {
                            kind: ExprKind::Literal(val_str),
                            span,
                        }
                    };
                    assoc_pairs.push((key, val));
                    continue;
                }
                // Check for [key] followed by = token (scanner splits on =)
                if w.ends_with(']') && w.len() > 2 {
                    let next_tok = scanner.peek_n(1).token.clone();
                    if matches!(next_tok, Tok::Eq) {
                        is_assoc = true;
                        let key = w[1..w.len()-1].to_string();
                        scanner.next(); // consume [key]
                        scanner.next(); // consume =
                        let val = parse_expr(scanner)?;
                        assoc_pairs.push((key, val));
                        continue;
                    }
                }
            }
        }

        let elem = parse_expr(scanner)?;
        elems.push(elem);
    }

    if is_assoc {
        Ok(Expr {
            kind: ExprKind::AssocArray(assoc_pairs),
            span,
        })
    } else {
        Ok(Expr {
            kind: ExprKind::Array(elems),
            span,
        })
    }
}

/// Compute an ir::Span from a byte offset using the scanner.
pub fn span_from(scanner: &Scanner, offset: usize) -> Span {
    let (line, col) = scanner.line_col(offset);
    Span { line, col }
}

/// Convert a keyword token to its string representation.
fn keyword_to_str(tok: &Tok) -> &'static str {
    match tok {
        Tok::If => "if",
        Tok::Then => "then",
        Tok::Else => "else",
        Tok::Elif => "elif",
        Tok::Fi => "fi",
        Tok::While => "while",
        Tok::Until => "until",
        Tok::Do => "do",
        Tok::Done => "done",
        Tok::For => "for",
        Tok::In => "in",
        Tok::Case => "case",
        Tok::Esac => "esac",
        Tok::Fn => "fn",
        Tok::Function => "function",
        Tok::Local => "local",
        Tok::Int => "int",
        Tok::Const => "const",
        Tok::Array => "array",
        Tok::Map => "map",
        Tok::Bless => "bless",
        Tok::End => "END",
        _ => "",
    }
}
