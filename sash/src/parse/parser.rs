//! Recursive descent parser for sash-native syntax.
//!
//! Produces `ir::Program` directly from tokens — no intermediate CST.
//! Parse-time rejections (SB4001–4024) emit diagnostics immediately.
//! Unknown syntax is a hard error; recovery skips to next terminator.
//!
//! SECURITY: This is the parse-time gate. Every rejected construct
//! produces a diagnostic with an SB* code. Unknown syntax is a hard error.
//! No input can reach IR without passing through `parse_statement()`.
//!
//! Structure:
//!   - `parse()` — top-level entry: statement loop with error recovery
//!   - Control flow: parse_if, parse_while, parse_for, parse_case
//!   - Declarations: parse_function, parse_local, parse_declaration
//!   - Commands: parse_simple_or_assignment, parse_command
//!   - Composition: maybe_wrap_redirects (pipes, list ops, redirects)
//!   - Test expressions: parse_test_expr (recursive descent for [[ ]])
//!   - Postfix rejection: SB4021–4024 on compound commands

use super::arith::{
    collect_arith_content_raw, lower_arith_command, lower_c_style_for, parse_arith_command,
};
use super::error::{parse_deferred, parse_error};
use super::expr::{
    parse_array_literal, parse_command_substitution, parse_expr, parse_sub_script, parse_word,
    span_from, string_parts_to_exprs,
};
use super::lexer::{Scanner, SpannedToken, StringPart, Tok};
use crate::diagnostic::Diagnostic;
use crate::ir::*;

/// Parse sash-native source into IR.
/// Returns the program or a list of diagnostics on failure.
pub fn parse(source: &str) -> Result<(Program, Vec<Diagnostic>), Vec<Diagnostic>> {
    let mut scanner = Scanner::new(source);
    let mut stmts = Vec::new();
    let mut diags = Vec::new();
    let mut end_block: Option<Vec<Statement>> = None;

    while !scanner.at_eof() {
        skip_terminators(&mut scanner);
        if scanner.at_eof() {
            break;
        }

        match parse_statement(&mut scanner) {
            Ok(Some(stmt)) => {
                // Check for rejected constructs that we parsed for error reporting
                check_rejected(&stmt, &mut diags);
                stmts.push(stmt);
            }
            Ok(None) => {}
            Err(d) => {
                diags.push(d);
                // Try to recover: skip to next line
                recover(&mut scanner);
            }
        }
    }

    if !diags.is_empty() {
        // Check if all diagnostics are deferred — those aren't fatal at parse time
        let has_errors = diags
            .iter()
            .any(|d| d.severity == crate::diagnostic::Severity::Error);
        if has_errors {
            return Err(diags);
        }
    }

    Ok((
        Program {
            statements: stmts,
            end_block,
        },
        diags,
    ))
}

fn skip_terminators(scanner: &mut Scanner) {
    while matches!(scanner.peek().token, Tok::Newline | Tok::Semi) {
        scanner.next();
    }
}

fn recover(scanner: &mut Scanner) {
    // Skip to next newline or semicolon
    loop {
        match scanner.peek().token {
            Tok::Newline | Tok::Semi | Tok::Eof => {
                scanner.next();
                break;
            }
            _ => {
                scanner.next();
            }
        }
    }
}

/// Parse a single statement. Returns None for empty/comment-only lines.
pub fn parse_statement(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    skip_terminators(scanner);
    if scanner.at_eof() {
        return Ok(None);
    }

    let start = scanner.peek().span.start;
    let tok = scanner.peek().token.clone();
    let span = span_from(scanner, start);

    let result = match tok {
        Tok::Comment(ref text) => {
            let text = text.clone();
            scanner.next();
            Ok(Some(Statement {
                kind: StmtKind::Comment(text),
                span,
            }))
        }
        Tok::If => parse_if(scanner),
        Tok::While | Tok::Until => parse_while(scanner),
        Tok::For => parse_for(scanner),
        Tok::Case => parse_case(scanner),
        Tok::Fn | Tok::Function => parse_function(scanner, false),
        Tok::Local => parse_local(scanner),
        Tok::Int => parse_declaration(scanner, "int"),
        Tok::Const => parse_declaration(scanner, "const"),
        Tok::Array => parse_declaration(scanner, "array"),
        Tok::Map => parse_declaration(scanner, "map"),
        Tok::End => parse_end_block(scanner),
        Tok::LBrace => parse_group(scanner),
        Tok::DblParen => parse_arith_stmt(scanner),
        Tok::Bang => parse_negated(scanner),
        Tok::Dash => parse_suppressed(scanner),
        Tok::Word(ref w) if w == "-" => parse_suppressed(scanner),
        Tok::DblBracketOpen => parse_test_command(scanner),
        Tok::Bless => parse_bless(scanner),
        _ => parse_simple_or_assignment(scanner),
    };

    // Wrap with trailing redirects/pipes/list-ops if present
    match result {
        Ok(Some(stmt)) => {
            // Reject all postfix operators on compound commands.
            // Sash idiom: wrap in a function for composition.
            if is_compound(&stmt.kind) {
                let next = scanner.peek().token.clone();
                if matches!(
                    next,
                    Tok::Gt
                        | Tok::DblGt
                        | Tok::Lt
                        | Tok::GtAmp
                        | Tok::LtAmp
                        | Tok::LtGt
                        | Tok::AmpGt
                        | Tok::HereString
                ) {
                    return Err(parse_error(
                        stmt.span,
                        "SB4021",
                        "redirects on compound commands are not supported — wrap in a function"
                            .to_string(),
                    ));
                }
                if matches!(next, Tok::Pipe) {
                    return Err(parse_error(
                        stmt.span,
                        "SB4022",
                        "pipes on compound commands are not supported — wrap in a function"
                            .to_string(),
                    ));
                }
                if matches!(next, Tok::AndAnd | Tok::OrOr) {
                    return Err(parse_error(
                        stmt.span,
                        "SB4023",
                        "&& / || on compound commands are not supported — wrap in a function"
                            .to_string(),
                    ));
                }
                if matches!(next, Tok::Amp) {
                    return Err(parse_error(
                        stmt.span,
                        "SB4024",
                        "& on compound commands is not supported — wrap in a function".to_string(),
                    ));
                }
                Ok(Some(stmt))
            } else {
                Ok(Some(maybe_wrap_redirects(scanner, stmt)?))
            }
        }
        other => other,
    }
}

/// Check if a statement kind is a compound command.
fn is_compound(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::If(_)
            | StmtKind::While(_)
            | StmtKind::For(_)
            | StmtKind::Case(_)
            | StmtKind::Group(_)
    )
}

/// Parse a list of statements until a stop token.
fn parse_body_until(scanner: &mut Scanner, stop: &[Tok]) -> Result<Vec<Statement>, Diagnostic> {
    let mut stmts = Vec::new();
    loop {
        skip_terminators(scanner);
        if scanner.at_eof() {
            break;
        }
        if stop.iter().any(|s| tok_eq(&scanner.peek().token, s)) {
            break;
        }
        match parse_statement(scanner)? {
            Some(stmt) => stmts.push(stmt),
            None => break,
        }
    }
    Ok(stmts)
}

fn tok_eq(a: &Tok, b: &Tok) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Parse a compound list (condition) — one or more statements.
fn parse_compound_list(scanner: &mut Scanner, stop: &[Tok]) -> Result<Vec<Statement>, Diagnostic> {
    parse_body_until(scanner, stop)
}

// ── if/elif/else/fi ──────────────────────────────────────────

fn parse_if(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'if'

    let condition = parse_compound_list(scanner, &[Tok::Then])?;
    expect(scanner, &Tok::Then, span, "expected 'then'")?;

    let then_body = parse_body_until(scanner, &[Tok::Elif, Tok::Else, Tok::Fi])?;

    let mut elif_clauses = Vec::new();
    while matches!(scanner.peek().token, Tok::Elif) {
        scanner.next(); // consume 'elif'
        let cond = parse_compound_list(scanner, &[Tok::Then])?;
        expect(scanner, &Tok::Then, span, "expected 'then' after elif")?;
        let body = parse_body_until(scanner, &[Tok::Elif, Tok::Else, Tok::Fi])?;
        elif_clauses.push(ElifClause {
            condition: cond,
            body,
        });
    }

    let else_body = if matches!(scanner.peek().token, Tok::Else) {
        scanner.next();
        Some(parse_body_until(scanner, &[Tok::Fi])?)
    } else {
        None
    };

    expect(scanner, &Tok::Fi, span, "expected 'fi'")?;

    Ok(Some(Statement {
        kind: StmtKind::If(If {
            condition,
            then_body,
            elif_clauses,
            else_body,
        }),
        span,
    }))
}

// ── while/until ──────────────────────────────────────────────

fn parse_while(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    let until = matches!(scanner.peek().token, Tok::Until);
    scanner.next(); // consume 'while' or 'until'

    let condition = parse_compound_list(scanner, &[Tok::Do])?;
    expect(scanner, &Tok::Do, span, "expected 'do'")?;
    let body = parse_body_until(scanner, &[Tok::Done])?;
    expect(scanner, &Tok::Done, span, "expected 'done'")?;

    Ok(Some(Statement {
        kind: StmtKind::While(While {
            condition,
            body,
            until,
        }),
        span,
    }))
}

// ── for ──────────────────────────────────────────────────────

fn parse_for(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'for'

    // Check for C-style for: for (( init; cond; update ))
    if matches!(scanner.peek().token, Tok::DblParen) {
        return parse_c_style_for(scanner, span);
    }

    // Standard for: for var in words...; do body; done
    let variable = expect_word(scanner, span, "expected variable name after 'for'")?;

    // Optional semicolons/newlines between var and 'in'
    skip_terminators(scanner);

    let mut words = Vec::new();
    if matches!(scanner.peek().token, Tok::In) {
        scanner.next(); // consume 'in'
                        // Collect words until ; or newline or 'do'
        while !scanner.at_eof()
            && !matches!(
                scanner.peek().token,
                Tok::Semi | Tok::Newline | Tok::Do | Tok::Eof
            )
        {
            words.push(parse_expr(scanner)?);
        }
        skip_terminators(scanner);
    }

    expect(scanner, &Tok::Do, span, "expected 'do'")?;
    let body = parse_body_until(scanner, &[Tok::Done])?;
    expect(scanner, &Tok::Done, span, "expected 'done'")?;

    Ok(Some(Statement {
        kind: StmtKind::For(For {
            variable,
            words,
            body,
        }),
        span,
    }))
}

fn parse_c_style_for(scanner: &mut Scanner, span: Span) -> Result<Option<Statement>, Diagnostic> {
    scanner.next(); // consume ((

    // Collect raw content for init; cond; update — split on ;
    let content = collect_arith_content_raw(scanner)?;

    // Split on ';' — should give us 3 parts
    let parts: Vec<&str> = content.splitn(3, ';').collect();
    let init = parts.first().map(|s| s.trim()).unwrap_or("");
    let cond = parts.get(1).map(|s| s.trim()).unwrap_or("");
    let update = parts.get(2).map(|s| s.trim()).unwrap_or("");

    skip_terminators(scanner);
    expect(scanner, &Tok::Do, span, "expected 'do' after for (( ))")?;
    let body = parse_body_until(scanner, &[Tok::Done])?;
    expect(scanner, &Tok::Done, span, "expected 'done'")?;

    Ok(Some(lower_c_style_for(init, cond, update, body, span)))
}

// ── case ─────────────────────────────────────────────────────

fn parse_case(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'case'

    let value = parse_word(scanner)?;
    skip_terminators(scanner);
    expect(scanner, &Tok::In, span, "expected 'in' after case value")?;
    skip_terminators(scanner);

    let mut items = Vec::new();
    while !scanner.at_eof() && !matches!(scanner.peek().token, Tok::Esac) {
        skip_terminators(scanner);
        if matches!(scanner.peek().token, Tok::Esac) {
            break;
        }
        items.push(parse_case_item(scanner, span)?);
    }

    expect(scanner, &Tok::Esac, span, "expected 'esac'")?;

    Ok(Some(Statement {
        kind: StmtKind::Case(Case { value, items }),
        span,
    }))
}

fn parse_case_item(scanner: &mut Scanner, span: Span) -> Result<CaseItem, Diagnostic> {
    // Optional ( before first pattern
    if matches!(scanner.peek().token, Tok::LParen) {
        scanner.next();
    }

    // Parse patterns separated by |
    let mut patterns = Vec::new();
    loop {
        let pat = parse_expr(scanner)?;
        patterns.push(pat);
        if matches!(scanner.peek().token, Tok::Pipe) {
            scanner.next();
        } else {
            break;
        }
    }

    // Expect )
    expect(
        scanner,
        &Tok::RParen,
        span,
        "expected ')' after case pattern",
    )?;

    // Parse body
    let body = parse_body_until(
        scanner,
        &[Tok::DblSemi, Tok::SemiAmp, Tok::DblSemiAmp, Tok::Esac],
    )?;

    let mut fallthrough = false;
    let mut resume = false;
    match scanner.peek().token {
        Tok::DblSemi => {
            scanner.next();
        }
        Tok::SemiAmp => {
            fallthrough = true;
            scanner.next();
        }
        Tok::DblSemiAmp => {
            resume = true;
            scanner.next();
        }
        Tok::Esac => {
            // Last item without terminator — ok
        }
        _ => {}
    }

    Ok(CaseItem {
        patterns,
        body,
        fallthrough,
        resume,
    })
}

// ── function ─────────────────────────────────────────────────

fn parse_function(scanner: &mut Scanner, is_local: bool) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'fn' or 'function'

    let name = expect_word(scanner, span, "expected function name")?;

    // Optional ()
    if matches!(scanner.peek().token, Tok::LParen) {
        scanner.next();
        expect(scanner, &Tok::RParen, span, "expected ')' after '('")?;
    }

    // Optional -> int return type annotation
    let mut return_type = VarType::String;
    if let Tok::Word(ref w) = scanner.peek().token {
        if w == "->" {
            scanner.next(); // consume ->
            if matches!(scanner.peek().token, Tok::Int) {
                scanner.next();
                return_type = VarType::Int;
            }
        } else if w == "-" {
            // Check for - > (separated)
            let next = scanner.peek_n(1);
            if matches!(next.token, Tok::Gt) {
                scanner.next(); // consume -
                scanner.next(); // consume >
                if matches!(scanner.peek().token, Tok::Int) {
                    scanner.next();
                    return_type = VarType::Int;
                }
            }
        }
    }

    // Expect {
    skip_terminators(scanner);
    expect(
        scanner,
        &Tok::LBrace,
        span,
        "expected '{' for function body",
    )?;
    let body = parse_body_until(scanner, &[Tok::RBrace])?;
    expect(scanner, &Tok::RBrace, span, "expected '}'")?;

    let func_name = if is_local {
        format!("__local__{}", name)
    } else {
        name
    };

    Ok(Some(Statement {
        kind: StmtKind::Function(Function {
            name: func_name,
            return_type,
            body,
            is_local,
        }),
        span,
    }))
}

// ── local ────────────────────────────────────────────────────

fn parse_local(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'local'

    // Check for 'local fn name' — DD#18
    if matches!(scanner.peek().token, Tok::Fn) {
        return parse_function(scanner, true);
    }

    // Otherwise it's a local declaration: local x=val or local -i x=val
    let mut var_type = VarType::String;
    let mut flags = Vec::new();

    // Check for type modifiers
    loop {
        match &scanner.peek().token {
            Tok::Int => {
                var_type = VarType::Int;
                scanner.next();
            }
            Tok::Const => {
                flags.push("-r".to_string());
                scanner.next();
            }
            Tok::Array => {
                flags.push("-a".to_string());
                scanner.next();
            }
            Tok::Map => {
                flags.push("-A".to_string());
                scanner.next();
            }
            Tok::Word(w) if w.starts_with('-') => {
                let w = w.clone();
                scanner.next();
                for ch in w[1..].chars() {
                    match ch {
                        'i' => var_type = VarType::Int,
                        'a' => flags.push("-a".to_string()),
                        'A' => flags.push("-A".to_string()),
                        'r' => flags.push("-r".to_string()),
                        _ => flags.push(format!("-{}", ch)),
                    }
                }
            }
            _ => break,
        }
    }

    let assignments = parse_declaration_assignments(scanner, span)?;

    Ok(Some(Statement {
        kind: StmtKind::Declaration(Declaration {
            keyword: DeclKeyword::Local,
            var_type,
            flags,
            assignments,
        }),
        span,
    }))
}

// ── declarations (int, const, array, map) ────────────────────

fn parse_declaration(
    scanner: &mut Scanner,
    keyword: &str,
) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume keyword

    let (decl_keyword, var_type, mut flags) = match keyword {
        "int" => (DeclKeyword::Declare, VarType::Int, vec![]),
        "const" => (
            DeclKeyword::Readonly,
            VarType::String,
            vec!["-r".to_string()],
        ),
        "array" => (
            DeclKeyword::Declare,
            VarType::String,
            vec!["-a".to_string()],
        ),
        "map" => (
            DeclKeyword::Declare,
            VarType::String,
            vec!["-A".to_string()],
        ),
        _ => (DeclKeyword::Declare, VarType::String, vec![]),
    };

    // For "const int x", handle additional type modifiers
    if keyword == "const" {
        if matches!(scanner.peek().token, Tok::Int) {
            scanner.next();
            return Ok(Some(Statement {
                kind: StmtKind::Declaration(Declaration {
                    keyword: DeclKeyword::Readonly,
                    var_type: VarType::Int,
                    flags: vec!["-r".to_string()],
                    assignments: parse_declaration_assignments(scanner, span)?,
                }),
                span,
            }));
        }
    }

    let assignments = parse_declaration_assignments(scanner, span)?;

    Ok(Some(Statement {
        kind: StmtKind::Declaration(Declaration {
            keyword: decl_keyword,
            var_type,
            flags,
            assignments,
        }),
        span,
    }))
}

fn parse_declaration_assignments(
    scanner: &mut Scanner,
    span: Span,
) -> Result<Vec<Assignment>, Diagnostic> {
    let mut assignments = Vec::new();

    loop {
        if scanner.at_eof() || matches!(scanner.peek().token, Tok::Newline | Tok::Semi | Tok::Eof) {
            break;
        }

        let name = expect_word(scanner, span, "expected variable name in declaration")?;

        // Check for = or +=
        if matches!(scanner.peek().token, Tok::Eq) {
            scanner.next(); // consume =

            // Check for array literal: =(...)
            if matches!(scanner.peek().token, Tok::LParen) {
                scanner.next();
                let arr = parse_array_literal(scanner, span)?;
                assignments.push(Assignment {
                    name,
                    index: None,
                    op: AssignOp::Set,
                    value: arr,
                });
            } else {
                let value = parse_expr(scanner)?;
                assignments.push(Assignment {
                    name,
                    index: None,
                    op: AssignOp::Set,
                    value,
                });
            }
        } else if matches!(scanner.peek().token, Tok::PlusAssign) {
            scanner.next(); // consume +=
            let value = parse_expr(scanner)?;
            assignments.push(Assignment {
                name,
                index: None,
                op: AssignOp::Append,
                value,
            });
        } else {
            // Bare declaration: int x
            assignments.push(Assignment {
                name,
                index: None,
                op: AssignOp::Set,
                value: Expr {
                    kind: ExprKind::Literal(String::new()),
                    span: Span { line: 0, col: 0 },
                },
            });
        }
    }

    Ok(assignments)
}

// ── END block ────────────────────────────────────────────────

fn parse_end_block(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'END'

    skip_terminators(scanner);
    expect(scanner, &Tok::LBrace, span, "expected '{' after END")?;
    let body = parse_body_until(scanner, &[Tok::RBrace])?;
    expect(scanner, &Tok::RBrace, span, "expected '}'")?;

    // Emit as __sash_end__ function for backward compatibility with transform
    Ok(Some(Statement {
        kind: StmtKind::Function(Function {
            name: "__sash_end__".to_string(),
            return_type: VarType::String,
            body,
            is_local: false,
        }),
        span,
    }))
}

// ── group { ... } ────────────────────────────────────────────

fn parse_group(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume '{'

    let body = parse_body_until(scanner, &[Tok::RBrace])?;
    expect(scanner, &Tok::RBrace, span, "expected '}'")?;

    Ok(Some(Statement {
        kind: StmtKind::Group(Group { body }),
        span,
    }))
}

// ── arithmetic (( )) ─────────────────────────────────────────

fn parse_arith_stmt(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume ((

    let expr = parse_arith_command(scanner, span)?;
    let stmt = lower_arith_command(expr, span);
    Ok(Some(stmt))
}

// ── negated command (! cmd) ──────────────────────────────────

fn parse_negated(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume '!'

    if let Some(stmt) = parse_statement(scanner)? {
        // Wrap in a negated pipeline
        match stmt.kind {
            StmtKind::Pipeline(mut p) => {
                p.negated = true;
                Ok(Some(Statement {
                    kind: StmtKind::Pipeline(p),
                    span,
                }))
            }
            _ => Ok(Some(Statement {
                kind: StmtKind::Pipeline(Pipeline {
                    commands: vec![stmt],
                    negated: true,
                }),
                span,
            })),
        }
    } else {
        Ok(None)
    }
}

// ── suppressed command (- cmd) ───────────────────────────────

fn parse_suppressed(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);

    // Peek ahead: if what follows looks like a command, treat - as suppression
    let next = scanner.peek_n(1);
    if next.token.is_word_like()
        || matches!(
            next.token,
            Tok::If
                | Tok::While
                | Tok::For
                | Tok::Case
                | Tok::DblBracketOpen
                | Tok::DblParen
                | Tok::LBrace
        )
    {
        scanner.next(); // consume '-'
        if let Some(stmt) = parse_statement(scanner)? {
            Ok(Some(Statement {
                kind: StmtKind::Suppressed(Box::new(stmt)),
                span,
            }))
        } else {
            Ok(None)
        }
    } else {
        // Not a suppressed command — treat as simple command starting with -
        parse_simple_or_assignment(scanner)
    }
}

// ── [[ test ]] ───────────────────────────────────────────────

fn parse_test_command(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume [[

    let test_expr = parse_test_expr(scanner, span)?;

    expect(scanner, &Tok::DblBracketClose, span, "expected ']]'")?;

    Ok(Some(Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("[[".to_string()),
                span,
            },
            args: vec![Expr {
                kind: ExprKind::Test(test_expr),
                span,
            }],
        }),
        span,
    }))
}

/// Parse a test expression inside [[ ]].
fn parse_test_expr(scanner: &mut Scanner, span: Span) -> Result<TestExpr, Diagnostic> {
    parse_test_or(scanner, span)
}

fn parse_test_or(scanner: &mut Scanner, span: Span) -> Result<TestExpr, Diagnostic> {
    let mut left = parse_test_and(scanner, span)?;
    while matches!(scanner.peek().token, Tok::OrOr) {
        scanner.next();
        let right = parse_test_and(scanner, span)?;
        left = TestExpr::Or(Box::new(left), Box::new(right));
    }
    Ok(left)
}

fn parse_test_and(scanner: &mut Scanner, span: Span) -> Result<TestExpr, Diagnostic> {
    let mut left = parse_test_not(scanner, span)?;
    while matches!(scanner.peek().token, Tok::AndAnd) {
        scanner.next();
        let right = parse_test_not(scanner, span)?;
        left = TestExpr::And(Box::new(left), Box::new(right));
    }
    Ok(left)
}

fn parse_test_not(scanner: &mut Scanner, span: Span) -> Result<TestExpr, Diagnostic> {
    if matches!(scanner.peek().token, Tok::Bang) {
        scanner.next();
        let inner = parse_test_primary(scanner, span)?;
        Ok(TestExpr::Not(Box::new(inner)))
    } else {
        parse_test_primary(scanner, span)
    }
}

fn parse_test_primary(scanner: &mut Scanner, span: Span) -> Result<TestExpr, Diagnostic> {
    // ( grouped expr )
    if matches!(scanner.peek().token, Tok::LParen) {
        scanner.next();
        let inner = parse_test_expr(scanner, span)?;
        expect(
            scanner,
            &Tok::RParen,
            span,
            "expected ')' in test expression",
        )?;
        return Ok(inner);
    }

    // Unary operators: -f, -d, -n, -z, -e, -r, -w, -x, etc.
    if let Tok::Word(ref w) = scanner.peek().token {
        if w.starts_with('-') && w.len() >= 2 {
            let op = w.clone();
            // Check if it's a known unary test operator
            if is_unary_test_op(&op) {
                scanner.next();
                let operand = parse_test_operand(scanner, span)?;
                return Ok(TestExpr::Unary {
                    op,
                    operand: Box::new(operand),
                });
            }
        }
    }

    // Primary: value, possibly followed by binary operator
    let left = parse_test_operand(scanner, span)?;

    // Check for binary operator
    let next = scanner.peek();
    match &next.token {
        Tok::Word(w) if is_binary_test_op(w) => {
            let op = w.clone();
            scanner.next();
            let right = if op == "=~" {
                // Regex RHS: collect raw text until ]] (braces, parens are literal)
                parse_regex_rhs(scanner, span)?
            } else {
                parse_test_operand(scanner, span)?
            };
            Ok(TestExpr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            })
        }
        Tok::Eq => {
            // = or == in test
            scanner.next();
            // Check for == (next char)
            if matches!(scanner.peek().token, Tok::Eq) {
                scanner.next(); // consume second =
            }
            let right = parse_test_operand(scanner, span)?;
            Ok(TestExpr::Binary {
                left: Box::new(left),
                op: "==".to_string(),
                right: Box::new(right),
            })
        }
        Tok::Bang => {
            // !=
            scanner.next();
            if matches!(scanner.peek().token, Tok::Eq) {
                scanner.next();
                let right = parse_test_operand(scanner, span)?;
                Ok(TestExpr::Binary {
                    left: Box::new(left),
                    op: "!=".to_string(),
                    right: Box::new(right),
                })
            } else {
                // Bare ! after expression — shouldn't happen
                Ok(TestExpr::Unary {
                    op: "-n".to_string(),
                    operand: Box::new(left),
                })
            }
        }
        Tok::Lt => {
            scanner.next();
            let right = parse_test_operand(scanner, span)?;
            Ok(TestExpr::Binary {
                left: Box::new(left),
                op: "<".to_string(),
                right: Box::new(right),
            })
        }
        Tok::Gt => {
            scanner.next();
            let right = parse_test_operand(scanner, span)?;
            Ok(TestExpr::Binary {
                left: Box::new(left),
                op: ">".to_string(),
                right: Box::new(right),
            })
        }
        _ => {
            // Bare expression — implicit -n test
            Ok(TestExpr::Unary {
                op: "-n".to_string(),
                operand: Box::new(left),
            })
        }
    }
}

fn parse_test_operand(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    // Don't consume ]] or && or ||
    if matches!(
        scanner.peek().token,
        Tok::DblBracketClose | Tok::AndAnd | Tok::OrOr | Tok::RParen
    ) {
        return Err(parse_error(
            span,
            "SB4014",
            "expected operand in test expression".to_string(),
        ));
    }
    parse_expr(scanner)
}

/// Parse regex RHS in [[ =~ pattern ]]: collect tokens as literal until ]]
fn parse_regex_rhs(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    let mut pattern = String::new();
    loop {
        if scanner.at_eof() {
            break;
        }
        match &scanner.peek().token {
            Tok::DblBracketClose | Tok::AndAnd | Tok::OrOr => break,
            Tok::Newline => break,
            Tok::Word(w) => {
                pattern.push_str(w);
                scanner.next();
            }
            Tok::LBrace => {
                pattern.push('{');
                scanner.next();
            }
            Tok::RBrace => {
                pattern.push('}');
                scanner.next();
            }
            Tok::LParen => {
                pattern.push('(');
                scanner.next();
            }
            Tok::RParen => {
                pattern.push(')');
                scanner.next();
            }
            Tok::Pipe => {
                pattern.push('|');
                scanner.next();
            }
            Tok::Bang => {
                pattern.push('!');
                scanner.next();
            }
            Tok::Dash => {
                pattern.push('-');
                scanner.next();
            }
            Tok::Dollar(v) => {
                pattern.push('$');
                pattern.push_str(v);
                scanner.next();
            }
            Tok::Eq => {
                pattern.push('=');
                scanner.next();
            }
            _ => {
                scanner.next();
            }
        }
    }
    if pattern.is_empty() {
        return Err(parse_error(
            span,
            "SB4014",
            "expected regex pattern after =~".to_string(),
        ));
    }
    Ok(Expr {
        kind: ExprKind::Literal(pattern),
        span,
    })
}

fn is_unary_test_op(op: &str) -> bool {
    matches!(
        op,
        "-f" | "-d"
            | "-e"
            | "-r"
            | "-w"
            | "-x"
            | "-s"
            | "-L"
            | "-h"
            | "-n"
            | "-z"
            | "-b"
            | "-c"
            | "-p"
            | "-S"
            | "-t"
            | "-O"
            | "-G"
            | "-N"
            | "-o"
            | "-v"
            | "-R"
            | "-a" // exists (deprecated but still works)
    )
}

fn is_binary_test_op(op: &str) -> bool {
    matches!(
        op,
        "==" | "!=" | "=~" | "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" | "-nt" | "-ot" | "-ef"
    )
}

// ── bless ────────────────────────────────────────────────────

fn parse_bless(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'bless'

    // bless VAR [[ test_expr ]]
    let var_expr = parse_expr(scanner)?;

    // Expect [[ test_expr ]]
    let mut args = vec![var_expr];
    if matches!(scanner.peek().token, Tok::DblBracketOpen) {
        scanner.next();
        let test_expr = parse_test_expr(scanner, span)?;
        expect(
            scanner,
            &Tok::DblBracketClose,
            span,
            "expected ']]' in bless",
        )?;
        args.push(Expr {
            kind: ExprKind::Test(test_expr),
            span,
        });
    }

    Ok(Some(Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("bless".to_string()),
                span,
            },
            args,
        }),
        span,
    }))
}

// ── simple commands and assignments ──────────────────────────

fn parse_simple_or_assignment(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);

    // Check for rejected constructs
    if let Tok::Word(ref w) = scanner.peek().token {
        let w = w.clone();
        match w.as_str() {
            "eval" => {
                scanner.next();
                return Err(parse_error(
                    span,
                    "SB4001",
                    "'eval' is not allowed in the safe subset".to_string(),
                ));
            }
            "source" | "." => {
                scanner.next();
                return Err(parse_error(
                    span,
                    "SB4002",
                    "'source' / '.' is not allowed — use sash-convert to inline source files"
                        .to_string(),
                ));
            }
            "export" => {
                scanner.next();
                return Err(parse_error(
                    span,
                    "SB5001",
                    "'export' is not allowed — use FOO=bar before a command instead".to_string(),
                ));
            }
            "declare" | "typeset" => {
                scanner.next();
                return Err(parse_error(
                    span,
                    "SB5001",
                    format!("'{}' is not allowed — use int/const/array/map instead", w),
                ));
            }
            "readonly" => {
                scanner.next();
                return Err(parse_error(
                    span,
                    "SB5001",
                    "'readonly' is not allowed — use 'const' instead".to_string(),
                ));
            }
            "select" => {
                scanner.next();
                return Err(parse_error(
                    span,
                    "SB5001",
                    "'select' is not supported in sash".to_string(),
                ));
            }
            _ => {}
        }
    }

    // Check if this is an assignment: word = value or word += value
    // Look for: Word Eq or Word PlusAssign or Word[idx] Eq
    let maybe_assignment = match &scanner.peek().token {
        Tok::Word(w) => {
            // Check if next token is = or +=
            let w = w.clone();
            let next = scanner.peek_n(1);
            matches!(next.token, Tok::Eq | Tok::PlusAssign) && is_identifier_or_subscript(&w)
        }
        _ => false,
    };

    if maybe_assignment {
        return parse_assignment(scanner, span);
    }

    // Check for regex mutation: var =~ s/pat/rep/
    if let Tok::Word(ref w) = scanner.peek().token {
        let w = w.clone();
        if is_identifier(&w) {
            if let Tok::Word(ref next_w) = scanner.peek_n(1).token {
                if next_w == "=~" {
                    return parse_regex_mutation(scanner, span, &w);
                }
            }
        }
    }

    // It's a simple command
    parse_command(scanner, span)
}

fn parse_assignment(scanner: &mut Scanner, span: Span) -> Result<Option<Statement>, Diagnostic> {
    let name_tok = scanner.next();
    let raw_name = match name_tok.token {
        Tok::Word(w) => w,
        _ => {
            return Err(parse_error(
                span,
                "SB4014",
                "expected variable name".to_string(),
            ))
        }
    };

    // Parse optional subscript: name[idx]
    let (name, index) = if let Some(bracket_pos) = raw_name.find('[') {
        let n = raw_name[..bracket_pos].to_string();
        let rest = &raw_name[bracket_pos + 1..];
        let idx = if let Some(close) = rest.find(']') {
            rest[..close].to_string()
        } else {
            rest.to_string()
        };
        (
            n,
            Some(Expr {
                kind: ExprKind::Literal(idx),
                span,
            }),
        )
    } else {
        (raw_name, None)
    };

    let op = if matches!(scanner.peek().token, Tok::PlusAssign) {
        scanner.next();
        AssignOp::Append
    } else {
        scanner.next(); // consume =
        AssignOp::Set
    };

    // Check for array literal: =(...)
    if matches!(scanner.peek().token, Tok::LParen) {
        scanner.next();
        let arr = parse_array_literal(scanner, span)?;
        return Ok(Some(Statement {
            kind: StmtKind::Assignment(Assignment {
                name,
                index,
                op,
                value: arr,
            }),
            span,
        }));
    }

    // Check for empty assignment (newline/semi/eof immediately)
    if scanner.at_eof() || matches!(scanner.peek().token, Tok::Newline | Tok::Semi) {
        return Ok(Some(Statement {
            kind: StmtKind::Assignment(Assignment {
                name,
                index,
                op,
                value: Expr {
                    kind: ExprKind::Literal(String::new()),
                    span,
                },
            }),
            span,
        }));
    }

    let value = parse_expr(scanner)?;

    Ok(Some(Statement {
        kind: StmtKind::Assignment(Assignment {
            name,
            index,
            op,
            value,
        }),
        span,
    }))
}

/// Parse regex mutation: var =~ s/pat/rep/ or var =~ s/pat/rep/flags
fn parse_regex_mutation(
    scanner: &mut Scanner,
    span: Span,
    var_name: &str,
) -> Result<Option<Statement>, Diagnostic> {
    scanner.next(); // consume variable name
    scanner.next(); // consume =~
                    // Collect the regex expression (s/pat/rep/ or s/pat/rep/flags)
    let regex_expr = if let Tok::Word(ref w) = scanner.peek().token {
        let w = w.clone();
        scanner.next();
        w
    } else {
        // Try to collect from multiple adjacent tokens
        let mut s = String::new();
        while !scanner.at_eof()
            && !matches!(
                scanner.peek().token,
                Tok::Semi | Tok::Newline | Tok::Pipe | Tok::AndAnd | Tok::OrOr | Tok::Eof
            )
        {
            if !s.is_empty() && !scanner.next_is_adjacent() {
                break;
            }
            match &scanner.peek().token {
                Tok::Word(w) => {
                    s.push_str(w);
                    scanner.next();
                }
                _ => break,
            }
        }
        s
    };
    // Emit as Command { name: var, args: [regex] } — transform recognizes
    // this pattern: command name is variable, single arg starts with s/
    Ok(Some(Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal(var_name.to_string()),
                span,
            },
            args: vec![Expr {
                kind: ExprKind::Literal(regex_expr),
                span,
            }],
        }),
        span,
    }))
}

fn parse_command(scanner: &mut Scanner, span: Span) -> Result<Option<Statement>, Diagnostic> {
    // Parse the command name
    let name_expr = parse_expr(scanner)?;

    // Parse arguments until terminator or pipe/list operator
    let mut args = Vec::new();
    let mut redirects: Vec<Redirect> = Vec::new();

    loop {
        if scanner.at_eof() {
            break;
        }

        match &scanner.peek().token {
            Tok::Newline
            | Tok::Semi
            | Tok::Pipe
            | Tok::AndAnd
            | Tok::OrOr
            | Tok::Amp
            | Tok::RParen
            | Tok::RBrace
            | Tok::DblSemi
            | Tok::SemiAmp
            | Tok::DblSemiAmp
            | Tok::DblBracketClose
            | Tok::Eof => break,

            // Redirects
            Tok::Gt
            | Tok::DblGt
            | Tok::Lt
            | Tok::GtAmp
            | Tok::LtAmp
            | Tok::LtGt
            | Tok::AmpGt
            | Tok::HereString => {
                redirects.push(parse_redirect(scanner, span)?);
            }

            // File descriptor before redirect: 2>, 1>, etc.
            Tok::Word(w) if is_fd_number(w) => {
                let w = w.clone();
                // Check if next token is a redirect operator
                let next = scanner.peek_n(1);
                if matches!(
                    next.token,
                    Tok::Gt | Tok::DblGt | Tok::GtAmp | Tok::LtAmp | Tok::Lt
                ) {
                    let fd: u32 = w.parse().unwrap_or(0);
                    scanner.next(); // consume FD number
                    let mut redir = parse_redirect(scanner, span)?;
                    redir.fd = Some(fd);
                    redirects.push(redir);
                } else {
                    args.push(parse_expr(scanner)?);
                }
            }

            _ => {
                args.push(parse_expr(scanner)?);
            }
        }
    }

    // Build the command statement
    let mut stmt = Statement {
        kind: StmtKind::Command(Command {
            name: name_expr,
            args,
        }),
        span,
    };

    // Wrap with redirects if any
    if !redirects.is_empty() {
        stmt = Statement {
            kind: StmtKind::Redirected(Redirected {
                body: Box::new(stmt),
                redirects,
            }),
            span,
        };
    }

    Ok(Some(stmt))
}

// ── Redirects ────────────────────────────────────────────────

/// Check for trailing redirects, pipes, and list operators after any statement.
fn maybe_wrap_redirects(scanner: &mut Scanner, stmt: Statement) -> Result<Statement, Diagnostic> {
    let mut redirects = Vec::new();
    loop {
        let tok = scanner.peek().token.clone();
        match tok {
            Tok::Gt
            | Tok::DblGt
            | Tok::Lt
            | Tok::GtAmp
            | Tok::LtAmp
            | Tok::LtGt
            | Tok::AmpGt
            | Tok::HereString => {
                redirects.push(parse_redirect(scanner, stmt.span)?);
            }
            Tok::Word(ref w) if is_fd_number(w) => {
                let next = scanner.peek_n(1).token.clone();
                if matches!(
                    next,
                    Tok::Gt | Tok::DblGt | Tok::GtAmp | Tok::LtAmp | Tok::Lt
                ) {
                    let fd: u32 = w.parse().unwrap_or(0);
                    scanner.next();
                    let mut redir = parse_redirect(scanner, stmt.span)?;
                    redir.fd = Some(fd);
                    redirects.push(redir);
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    let mut result = if redirects.is_empty() {
        stmt
    } else {
        let span = stmt.span;
        Statement {
            kind: StmtKind::Redirected(Redirected {
                body: Box::new(stmt),
                redirects,
            }),
            span,
        }
    };

    // Handle trailing pipeline
    if matches!(scanner.peek().token, Tok::Pipe) {
        let span = result.span;
        let mut commands = vec![result];
        while matches!(scanner.peek().token, Tok::Pipe) {
            scanner.next();
            skip_terminators(scanner);
            if let Some(next_stmt) = parse_statement(scanner)? {
                match next_stmt.kind {
                    StmtKind::Pipeline(p) => commands.extend(p.commands),
                    _ => commands.push(next_stmt),
                }
                break;
            }
        }
        if commands.len() > 1 {
            result = Statement {
                kind: StmtKind::Pipeline(Pipeline {
                    commands,
                    negated: false,
                }),
                span,
            };
        } else {
            result = commands.into_iter().next().unwrap();
        }
    }

    // Handle trailing list operators
    match scanner.peek().token {
        Tok::AndAnd => {
            let span = result.span;
            scanner.next();
            skip_terminators(scanner);
            let right = parse_statement(scanner)?.unwrap_or(Statement::true_cmd(span));
            result = Statement {
                kind: StmtKind::List(List {
                    op: ListOp::And,
                    left: Box::new(result),
                    right: Box::new(right),
                }),
                span,
            };
        }
        Tok::OrOr => {
            let span = result.span;
            scanner.next();
            skip_terminators(scanner);
            let right = parse_statement(scanner)?.unwrap_or(Statement::true_cmd(span));
            result = Statement {
                kind: StmtKind::List(List {
                    op: ListOp::Or,
                    left: Box::new(result),
                    right: Box::new(right),
                }),
                span,
            };
        }
        Tok::Amp => {
            let span = result.span;
            scanner.next();
            result = Statement {
                kind: StmtKind::List(List {
                    op: ListOp::Background,
                    left: Box::new(result),
                    right: Box::new(Statement::true_cmd(span)),
                }),
                span,
            };
        }
        _ => {}
    }

    Ok(result)
}

fn parse_redirect(scanner: &mut Scanner, span: Span) -> Result<Redirect, Diagnostic> {
    let tok = scanner.next();
    let (fd, op) = match tok.token {
        Tok::Gt => (None, RedirectOp::Write),
        Tok::DblGt => (None, RedirectOp::Append),
        Tok::Lt => (None, RedirectOp::Read),
        Tok::GtAmp => (None, RedirectOp::DupOutput),
        Tok::LtAmp => (None, RedirectOp::DupInput),
        Tok::LtGt => (None, RedirectOp::ReadWrite),
        Tok::AmpGt => {
            // &>file → >file 2>&1 (handled at IR level)
            // For now, emit as write redirect
            let target = parse_expr(scanner)?;
            return Ok(Redirect {
                fd: Some(1),
                op: RedirectOp::Write,
                target,
            });
        }
        Tok::HereString => {
            let target_expr = parse_expr(scanner)?;
            // Herestring: add trailing newline to match bash behavior
            let target = match target_expr.kind {
                ExprKind::Literal(s) => Expr {
                    kind: ExprKind::Literal(format!("{}\n", s)),
                    span: target_expr.span,
                },
                ExprKind::SingleQuoted(s) => Expr {
                    kind: ExprKind::Literal(format!("{}\n", s)),
                    span: target_expr.span,
                },
                ExprKind::DoubleQuoted(parts) => {
                    let mut new_parts = parts;
                    new_parts.push(Expr {
                        kind: ExprKind::Literal("\n".to_string()),
                        span: target_expr.span,
                    });
                    Expr {
                        kind: ExprKind::DoubleQuoted(new_parts),
                        span: target_expr.span,
                    }
                }
                _ => target_expr,
            };
            return Ok(Redirect {
                fd: None,
                op: RedirectOp::HereString,
                target,
            });
        }
        _ => {
            return Err(parse_error(
                span,
                "SB4014",
                "expected redirect operator".to_string(),
            ));
        }
    };

    let target = parse_expr(scanner)?;
    Ok(Redirect { fd, op, target })
}

// ── Helpers ──────────────────────────────────────────────────

/// A word that is a valid identifier: [a-zA-Z_][a-zA-Z0-9_]*
fn is_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A word that is a valid identifier or subscript: name or name[idx]
fn is_identifier_or_subscript(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '[' || c == ']')
}

/// A word that looks like a file descriptor number (1-2 digits).
fn is_fd_number(s: &str) -> bool {
    s.len() <= 2 && !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn expect(scanner: &mut Scanner, expected: &Tok, span: Span, msg: &str) -> Result<(), Diagnostic> {
    skip_terminators(scanner);
    if tok_eq(&scanner.peek().token, expected) {
        scanner.next();
        Ok(())
    } else {
        Err(parse_error(span, "SB4014", msg.to_string()))
    }
}

fn expect_word(scanner: &mut Scanner, span: Span, msg: &str) -> Result<String, Diagnostic> {
    match &scanner.peek().token {
        Tok::Word(w) => {
            let w = w.clone();
            scanner.next();
            Ok(w)
        }
        // Some keywords can also be used as identifiers in certain contexts
        Tok::In | Tok::Do | Tok::Done | Tok::Then | Tok::Fi | Tok::Else | Tok::Elif | Tok::Esac => {
            let name = format!("{:?}", scanner.peek().token).to_lowercase();
            scanner.next();
            Ok(name)
        }
        _ => Err(parse_error(span, "SB4014", msg.to_string())),
    }
}

/// Check for rejected constructs in a parsed statement.
fn check_rejected(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    // Check for backtick command substitution in expressions
    walk_exprs(stmt, &mut |expr| {
        if let ExprKind::Literal(ref s) = expr.kind {
            if s == "`backtick`" {
                diags.push(parse_error(
                    expr.span,
                    "SB5020",
                    "backtick command substitution is not allowed — use $(...) instead".to_string(),
                ));
            }
        }
        if let ExprKind::ParameterExpansion(ref pe) = expr.kind {
            if pe.operator.as_deref() == Some("!") {
                diags.push(parse_error(
                    expr.span,
                    "SB4003",
                    format!(
                        "indirect expansion ${{!{}}} is not allowed in the safe subset",
                        pe.name
                    ),
                ));
            }
            // SB4015: reject @-transforms
            if let Some(ref op) = pe.operator {
                if op.starts_with('@') && op.len() == 2 {
                    diags.push(parse_error(
                        expr.span,
                        "SB4015",
                        format!("parameter transformation ${{var{}}} is not supported — use ^/^^/,/,, instead", op),
                    ));
                }
            }
        }
    });
}

/// Walk all expressions in a statement, calling f for each.
fn walk_exprs(stmt: &Statement, f: &mut impl FnMut(&Expr)) {
    crate::ir::walk_all_exprs(stmt, f);
}
