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
use super::error::parse_error;
use super::expr::{parse_array_literal, parse_expr, parse_word, span_from};
use super::lexer::{Scanner, Tok};
use crate::diagnostic::Diagnostic;
use crate::ir::*;

/// Parse sash-native source into IR.
/// Returns the program or a list of diagnostics on failure.
pub fn parse(source: &str) -> Result<(Program, Vec<Diagnostic>), Vec<Diagnostic>> {
    let mut scanner = Scanner::new(source);
    let mut stmts = Vec::new();
    let mut diags = Vec::new();
    let end_block: Option<Vec<Statement>> = None;

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

    let stmt = match dispatch_statement(scanner, tok, span)? {
        Some(s) => s,
        None => return Ok(None),
    };

    if is_compound(&stmt.kind) {
        reject_compound_postfix(stmt, scanner)
    } else {
        Ok(Some(maybe_wrap_redirects(scanner, stmt)?))
    }
}

/// Dispatch on the leading token of a statement. Pure routing — each
/// arm calls the corresponding `parse_*` subparser, which is responsible
/// for advancing the scanner.
fn dispatch_statement(
    scanner: &mut Scanner,
    tok: Tok,
    span: Span,
) -> Result<Option<Statement>, Diagnostic> {
    match tok {
        Tok::Comment(text) => parse_comment_stmt(scanner, text, span),
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
        Tok::Word(_) if is_paren_function(scanner) => parse_paren_function(scanner),
        _ => parse_simple_or_assignment(scanner),
    }
}

fn parse_comment_stmt(
    scanner: &mut Scanner,
    text: String,
    span: Span,
) -> Result<Option<Statement>, Diagnostic> {
    scanner.next();
    Ok(Some(Statement {
        kind: StmtKind::Comment(text),
        span,
    }))
}

/// SB4021–SB4024 reject all postfix composition on compound commands.
/// Sash idiom: wrap the compound in a function for composition.
fn reject_compound_postfix(
    stmt: Statement,
    scanner: &mut Scanner,
) -> Result<Option<Statement>, Diagnostic> {
    let next = scanner.peek().token.clone();
    if let Some((code, msg)) = compound_postfix_rejection(&next) {
        return Err(parse_error(stmt.span, code, msg));
    }
    Ok(Some(stmt))
}

/// Lookup table: which SB-code/message a particular trailing token should
/// raise after a compound command. None ⇒ allowed (statement terminator).
fn compound_postfix_rejection(next: &Tok) -> Option<(&'static str, String)> {
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
        return Some((
            "SB4021",
            "redirects on compound commands are not supported — wrap in a function".to_string(),
        ));
    }
    if matches!(next, Tok::Pipe) {
        return Some((
            "SB4022",
            "pipes on compound commands are not supported — wrap in a function".to_string(),
        ));
    }
    if matches!(next, Tok::AndAnd | Tok::OrOr) {
        return Some((
            "SB4023",
            "&& / || on compound commands are not supported — wrap in a function".to_string(),
        ));
    }
    if matches!(next, Tok::Amp) {
        return Some((
            "SB4024",
            "& on compound commands is not supported — wrap in a function".to_string(),
        ));
    }
    None
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

/// DD#68: parse a `{ … }` block body (the `{` is at the cursor).
fn parse_brace_block(scanner: &mut Scanner, span: Span) -> Result<Vec<Statement>, Diagnostic> {
    expect(scanner, &Tok::LBrace, span, "expected '{' block body")?;
    let body = parse_body_until(scanner, &[Tok::RBrace])?;
    expect(
        scanner,
        &Tok::RBrace,
        span,
        "expected '}' to close the block body",
    )?;
    Ok(body)
}

/// DD#68: a `{ … }` group cannot be a control-flow condition (it is
/// ambiguous with the brace body). The condition must be a `;`-list.
fn brace_group_cond_error(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4027",
        "a `{ }` group cannot be a condition — use a `;`-list (e.g. `if a; b { … }`)".to_string(),
    )
}

fn parse_if(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'if'

    let condition = parse_compound_list(scanner, &[Tok::Then, Tok::LBrace])?;
    // DD#68: branch on the delimiter — `{` (brace form) or `then` (sugar).
    match scanner.peek().token {
        Tok::LBrace if condition.is_empty() => Err(brace_group_cond_error(span)),
        Tok::LBrace => parse_if_brace(scanner, span, condition),
        Tok::Then => parse_if_keyword(scanner, span, condition),
        _ => Err(parse_error(
            span,
            "SB4027",
            "expected a `{ }` body or `then` after the `if` condition".to_string(),
        )),
    }
}

/// The `then … [elif …] [else …] fi` sugar form of `if`.
fn parse_if_keyword(
    scanner: &mut Scanner,
    span: Span,
    condition: Vec<Statement>,
) -> Result<Option<Statement>, Diagnostic> {
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

/// DD#68: the brace form of `if` — `if COND { … } [elif COND { … }]…
/// [else if COND { … }] [else { … }]`. Builds the same `If` IR as the
/// `then/fi` sugar. `else if` desugars to `else` + a nested `if`.
fn parse_if_brace(
    scanner: &mut Scanner,
    span: Span,
    condition: Vec<Statement>,
) -> Result<Option<Statement>, Diagnostic> {
    let then_body = parse_brace_block(scanner, span)?;
    let mut elif_clauses = Vec::new();
    let mut else_body = None;
    loop {
        match scanner.peek().token {
            Tok::Elif => {
                scanner.next();
                elif_clauses.push(parse_brace_elif(scanner, span)?);
            }
            Tok::Else => {
                scanner.next();
                else_body = Some(parse_brace_else(scanner, span)?);
                break;
            }
            _ => break,
        }
    }
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

/// DD#68: one `elif COND { … }` clause of a brace `if` (the `elif` is
/// already consumed).
fn parse_brace_elif(scanner: &mut Scanner, span: Span) -> Result<ElifClause, Diagnostic> {
    let cond = parse_compound_list(scanner, &[Tok::LBrace])?;
    if cond.is_empty() {
        return Err(brace_group_cond_error(span));
    }
    let body = parse_brace_block(scanner, span)?;
    Ok(ElifClause {
        condition: cond,
        body,
    })
}

/// DD#68: the else clause of a brace `if` (the `else` is already
/// consumed). `else if …` → a nested `if` statement; plain `else { … }`
/// → a brace block.
fn parse_brace_else(scanner: &mut Scanner, span: Span) -> Result<Vec<Statement>, Diagnostic> {
    if matches!(scanner.peek().token, Tok::If) {
        let nested = parse_if(scanner)?
            .ok_or_else(|| parse_error(span, "SB4027", "expected `if`".to_string()))?;
        Ok(vec![nested])
    } else {
        parse_brace_block(scanner, span)
    }
}

// ── while/until ──────────────────────────────────────────────

fn parse_while(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    let until = matches!(scanner.peek().token, Tok::Until);
    scanner.next(); // consume 'while' or 'until'

    let condition = parse_compound_list(scanner, &[Tok::Do, Tok::LBrace])?;
    // DD#68: `{` (brace form) or `do` (sugar).
    if matches!(scanner.peek().token, Tok::LBrace) && condition.is_empty() {
        return Err(brace_group_cond_error(span));
    }
    let body = parse_loop_body(scanner, span)?;

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
                        // Collect words until ; or newline or 'do' or '{' (DD#68)
        while !scanner.at_eof()
            && !matches!(
                scanner.peek().token,
                Tok::Semi | Tok::Newline | Tok::Do | Tok::Eof | Tok::LBrace
            )
        {
            words.push(parse_expr(scanner)?);
        }
        skip_terminators(scanner);
    }

    let body = parse_loop_body(scanner, span)?;

    Ok(Some(Statement {
        kind: StmtKind::For(For {
            variable,
            words,
            body,
        }),
        span,
    }))
}

/// DD#68: parse a loop body — `{ … }` (brace form) or `do … done` (sugar).
/// Both produce the same body `Vec<Statement>`.
fn parse_loop_body(scanner: &mut Scanner, span: Span) -> Result<Vec<Statement>, Diagnostic> {
    if matches!(scanner.peek().token, Tok::LBrace) {
        parse_brace_block(scanner, span)
    } else {
        expect(scanner, &Tok::Do, span, "expected a `{ }` body or `do`")?;
        let body = parse_body_until(scanner, &[Tok::Done])?;
        expect(scanner, &Tok::Done, span, "expected 'done'")?;
        Ok(body)
    }
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
    let body = parse_loop_body(scanner, span)?;

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

    parse_optional_empty_parens(scanner, span)?;
    Ok(Some(finish_function_def(scanner, name, span, is_local)?))
}

/// DD#67: bash `name() { ... }` function-definition sugar (no `fn`/
/// `function` keyword). Produces the same `Function` IR as `fn name { }`.
/// The leading `name ( )` shape is recognised by [`is_paren_function`].
fn parse_paren_function(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    let name = expect_word(scanner, span, "expected function name")?;
    expect(
        scanner,
        &Tok::LParen,
        span,
        "expected '(' in function definition",
    )?;
    expect(
        scanner,
        &Tok::RParen,
        span,
        "expected ')' in function definition",
    )?;
    Ok(Some(finish_function_def(scanner, name, span, false)?))
}

/// True if the upcoming tokens are `WORD ( )` — the bash `name()`
/// function-definition prefix. `peek()` is the name word.
fn is_paren_function(scanner: &mut Scanner) -> bool {
    matches!(scanner.peek_n(1).token, Tok::LParen) && matches!(scanner.peek_n(2).token, Tok::RParen)
}

/// Shared tail of a function definition: optional `-> type`, then the
/// `{ body }`. `name` and the leading parens (if any) are already consumed.
fn finish_function_def(
    scanner: &mut Scanner,
    name: String,
    span: Span,
    is_local: bool,
) -> Result<Statement, Diagnostic> {
    let return_type = parse_optional_return_type(scanner);

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

    Ok(Statement {
        kind: StmtKind::Function(Function {
            name: func_name,
            return_type,
            body,
            is_local,
        }),
        span,
    })
}

/// Consume an optional `()` after the function name. Bash style:
/// `name() { ... }` vs sash style `fn name { ... }`.
fn parse_optional_empty_parens(scanner: &mut Scanner, span: Span) -> Result<(), Diagnostic> {
    if matches!(scanner.peek().token, Tok::LParen) {
        scanner.next();
        expect(scanner, &Tok::RParen, span, "expected ')' after '('")?;
    }
    Ok(())
}

/// Consume an optional `-> int` return-type annotation. Accepts both
/// the single `->` word token and the split `-` `>` form depending on
/// how the lexer classified the bytes.
fn parse_optional_return_type(scanner: &mut Scanner) -> VarType {
    let Tok::Word(w) = &scanner.peek().token else {
        return VarType::String;
    };
    if w == "->" {
        scanner.next();
        return consume_int_token(scanner);
    }
    if w == "-" && matches!(scanner.peek_n(1).token, Tok::Gt) {
        scanner.next(); // consume -
        scanner.next(); // consume >
        return consume_int_token(scanner);
    }
    VarType::String
}

fn consume_int_token(scanner: &mut Scanner) -> VarType {
    if matches!(scanner.peek().token, Tok::Int) {
        scanner.next();
        VarType::Int
    } else {
        VarType::String
    }
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

    let (decl_keyword, var_type, flags) = match keyword {
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
    if keyword == "const" && matches!(scanner.peek().token, Tok::Int) {
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
    if matches!(scanner.peek().token, Tok::LParen) {
        return parse_test_group(scanner, span);
    }
    if let Some(unary) = try_parse_leading_unary(scanner, span)? {
        return Ok(unary);
    }
    let left = parse_test_operand(scanner, span)?;
    parse_test_binary_tail(left, scanner, span)
}

fn parse_test_group(scanner: &mut Scanner, span: Span) -> Result<TestExpr, Diagnostic> {
    scanner.next();
    let inner = parse_test_expr(scanner, span)?;
    expect(
        scanner,
        &Tok::RParen,
        span,
        "expected ')' in test expression",
    )?;
    Ok(inner)
}

fn try_parse_leading_unary(
    scanner: &mut Scanner,
    span: Span,
) -> Result<Option<TestExpr>, Diagnostic> {
    if let Tok::Word(ref w) = scanner.peek().token {
        if w.starts_with('-') && w.len() >= 2 && is_unary_test_op(w) {
            let op = w.clone();
            scanner.next();
            let operand = parse_test_operand(scanner, span)?;
            return Ok(Some(TestExpr::Unary {
                op,
                operand: Box::new(operand),
            }));
        }
    }
    Ok(None)
}

fn parse_test_binary_tail(
    left: Expr,
    scanner: &mut Scanner,
    span: Span,
) -> Result<TestExpr, Diagnostic> {
    match &scanner.peek().token {
        Tok::Word(w) if is_binary_test_op(w) => {
            let op = w.clone();
            scanner.next();
            parse_binary_rhs(left, op, scanner, span)
        }
        Tok::Eq => {
            scanner.next();
            if matches!(scanner.peek().token, Tok::Eq) {
                scanner.next();
            }
            parse_binary_rhs(left, "==".to_string(), scanner, span)
        }
        Tok::Bang => {
            scanner.next();
            if matches!(scanner.peek().token, Tok::Eq) {
                scanner.next();
                parse_binary_rhs(left, "!=".to_string(), scanner, span)
            } else {
                Ok(implicit_nonempty(left))
            }
        }
        Tok::Lt => {
            scanner.next();
            parse_binary_rhs(left, "<".to_string(), scanner, span)
        }
        Tok::Gt => {
            scanner.next();
            parse_binary_rhs(left, ">".to_string(), scanner, span)
        }
        _ => Ok(implicit_nonempty(left)),
    }
}

fn parse_binary_rhs(
    left: Expr,
    op: String,
    scanner: &mut Scanner,
    span: Span,
) -> Result<TestExpr, Diagnostic> {
    let right = if op == "=~" {
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

fn implicit_nonempty(operand: Expr) -> TestExpr {
    TestExpr::Unary {
        op: "-n".to_string(),
        operand: Box::new(operand),
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

    // First identifier: a scalar variable, or the first bless-all binder.
    let first = expect_word(scanner, span, "expected variable name after 'bless'")?;

    let lit = |s: String| Expr {
        kind: ExprKind::Literal(s),
        span,
    };

    // Scalar form `bless VAR [[…]]` is just the var; bless-all forms
    // (`bless BINDER… in ARRAY [[…]]`) collect binders then the array.
    let mut args: Vec<Expr> = if matches!(scanner.peek().token, Tok::DblBracketOpen) {
        vec![lit(first)]
    } else {
        parse_bless_all_lead(scanner, first, span)?
    };

    // Expect [[ test_expr ]]
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

/// DD#61 bless-all lead: `BINDER… in ARRAY`. Returns the binder literals
/// followed by the array literal (the trailing `[[ test ]]` is parsed by
/// the caller).
fn parse_bless_all_lead(
    scanner: &mut Scanner,
    first: String,
    span: Span,
) -> Result<Vec<Expr>, Diagnostic> {
    let mut binders = vec![first];
    while !matches!(scanner.peek().token, Tok::In) {
        let Tok::Word(_) = &scanner.peek().token else {
            return Err(parse_error(
                span,
                "SB4014",
                "expected 'in' in bless-all (bless x in a [[ … ]])".to_string(),
            ));
        };
        binders.push(expect_word(scanner, span, "expected binder name in bless")?);
    }
    scanner.next(); // consume 'in'
    let array = expect_word(scanner, span, "expected array name after 'in' in bless")?;
    binders.push(array);
    Ok(binders
        .into_iter()
        .map(|s| Expr {
            kind: ExprKind::Literal(s),
            span,
        })
        .collect())
}

// ── simple commands and assignments ──────────────────────────

fn parse_simple_or_assignment(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);

    if let Some(diag) = try_rejected_keyword(scanner, span) {
        return Err(diag);
    }
    if looks_like_assignment(scanner) {
        return parse_assignment(scanner, span);
    }
    if let Some(name) = looks_like_regex_mutation(scanner) {
        return parse_regex_mutation(scanner, span, &name);
    }
    parse_command(scanner, span)
}

/// If the next word is a banned builtin, consume it and synthesize the
/// rejection diagnostic. Returns `None` to fall through to normal
/// command parsing.
fn try_rejected_keyword(scanner: &mut Scanner, span: Span) -> Option<Diagnostic> {
    let Tok::Word(w) = &scanner.peek().token else {
        return None;
    };
    let w = w.clone();
    let (code, msg) = rejected_keyword_msg(&w)?;
    scanner.next();
    Some(parse_error(span, code, msg))
}

/// Lookup table for banned builtins (eval, source/., export, declare/
/// typeset, readonly, select). Returns the diagnostic code and the
/// human message, or `None` if the word is not banned. `declare` and
/// `typeset` share a format string that interpolates the actual word.
fn rejected_keyword_msg(w: &str) -> Option<(&'static str, String)> {
    match w {
        "eval" => Some((
            "SB4001",
            "'eval' is not allowed in the safe subset".to_string(),
        )),
        "source" | "." => Some((
            "SB4002",
            "'source' / '.' is not allowed — use sash-convert to inline source files".to_string(),
        )),
        "export" => Some((
            "SB5001",
            "'export' is not allowed — use FOO=bar before a command instead".to_string(),
        )),
        "declare" | "typeset" => Some((
            "SB5001",
            format!("'{}' is not allowed — use int/const/array/map instead", w),
        )),
        "readonly" => Some((
            "SB5001",
            "'readonly' is not allowed — use 'const' instead".to_string(),
        )),
        "select" => Some(("SB5001", "'select' is not supported in sash".to_string())),
        _ => None,
    }
}

/// `name = value` / `name += value` / `name[idx] = value` shape probe.
/// Doesn't consume any tokens.
fn looks_like_assignment(scanner: &mut Scanner) -> bool {
    let Tok::Word(w) = &scanner.peek().token else {
        return false;
    };
    let w = w.clone();
    let next = scanner.peek_n(1);
    if matches!(next.token, Tok::Eq | Tok::PlusAssign) && is_identifier_or_subscript(&w) {
        return true;
    }
    looks_like_dynamic_subscript_assign(scanner, &w)
}

/// `name[$var]=` / `name[${var}]=` (and `+=`): a subscript assignment
/// whose `$var` index the lexer split off as its own token(s), leaving
/// `name[` as the first word. The closed single-Word form `name[lit]=`
/// is handled by the `is_identifier_or_subscript` path above.
fn looks_like_dynamic_subscript_assign(scanner: &mut Scanner, w: &str) -> bool {
    let Some(name) = w.strip_suffix('[') else {
        return false;
    };
    if !is_identifier(name) {
        return false;
    }
    if !matches!(
        scanner.peek_n(1).token,
        Tok::Dollar(_) | Tok::DollarBrace(_)
    ) {
        return false;
    }
    let Tok::Word(rb) = &scanner.peek_n(2).token else {
        return false;
    };
    rb == "]" && matches!(scanner.peek_n(3).token, Tok::Eq | Tok::PlusAssign)
}

/// `var =~ s/pat/rep/` shape probe. Returns the var name (cloned) if
/// the pattern matches; doesn't consume any tokens.
fn looks_like_regex_mutation(scanner: &mut Scanner) -> Option<String> {
    let Tok::Word(w) = &scanner.peek().token else {
        return None;
    };
    if !is_identifier(w) {
        return None;
    }
    let w = w.clone();
    let Tok::Word(next_w) = &scanner.peek_n(1).token else {
        return None;
    };
    if next_w == "=~" {
        Some(w)
    } else {
        None
    }
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

    let (name, index) = parse_assignment_lhs(scanner, raw_name, span)?;

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

/// Parse the optional subscript on an assignment LHS. A bare `name[`
/// means the `]` and a `$var` index were split into following tokens by
/// the lexer (dynamic subscript, DD#33); the closed single-Word
/// `name[lit]` form is handled by the `find('[')` branch; a plain
/// `name` has no subscript.
fn parse_assignment_lhs(
    scanner: &mut Scanner,
    raw_name: String,
    span: Span,
) -> Result<(String, Option<Expr>), Diagnostic> {
    if let Some(base) = raw_name.strip_suffix('[') {
        let idx = parse_split_subscript_index(scanner, span)?;
        return Ok((base.to_string(), Some(idx)));
    }
    let Some(bracket_pos) = raw_name.find('[') else {
        return Ok((raw_name, None));
    };
    let name = raw_name[..bracket_pos].to_string();
    let rest = &raw_name[bracket_pos + 1..];
    let idx = match rest.find(']') {
        Some(close) => rest[..close].to_string(),
        None => rest.to_string(),
    };
    Ok((
        name,
        Some(Expr {
            kind: ExprKind::Literal(idx),
            span,
        }),
    ))
}

/// Consume a dynamic subscript index that the lexer split into its own
/// tokens — `$var` (`Tok::Dollar`) or `${var}` (`Tok::DollarBrace` with a
/// bare identifier) — followed by the closing `]` word. Used for
/// `name[$var]=value` assignments (DD#33).
fn parse_split_subscript_index(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    let idx = match scanner.next().token {
        Tok::Dollar(v) => Expr {
            kind: ExprKind::SimpleExpansion(v),
            span,
        },
        Tok::DollarBrace(content) if is_identifier(&content) => Expr {
            kind: ExprKind::SimpleExpansion(content),
            span,
        },
        _ => {
            return Err(parse_error(
                span,
                "SB4014",
                "unsupported dynamic subscript index in assignment".to_string(),
            ))
        }
    };
    match &scanner.peek().token {
        Tok::Word(w) if w == "]" => {
            scanner.next();
        }
        _ => {
            return Err(parse_error(
                span,
                "SB4014",
                "expected ']' after subscript index".to_string(),
            ))
        }
    }
    Ok(idx)
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
        if !parse_command_element(scanner, span, &mut args, &mut redirects)? {
            break;
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

/// Parse one element of a command's argument/redirect list. Returns
/// `false` at a terminator (`;`, newline, pipe, `{`, `]]`, …) so the
/// caller stops the loop; `true` after consuming an arg or redirect.
fn parse_command_element(
    scanner: &mut Scanner,
    span: Span,
    args: &mut Vec<Expr>,
    redirects: &mut Vec<Redirect>,
) -> Result<bool, Diagnostic> {
    match &scanner.peek().token {
        Tok::Newline
        | Tok::Semi
        | Tok::Pipe
        | Tok::AndAnd
        | Tok::OrOr
        | Tok::Amp
        | Tok::RParen
        | Tok::RBrace
        | Tok::LBrace
        | Tok::DblSemi
        | Tok::SemiAmp
        | Tok::DblSemiAmp
        | Tok::DblBracketClose
        | Tok::Eof => return Ok(false),

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
            parse_fd_prefixed(scanner, &w, span, args, redirects)?;
        }

        _ => {
            args.push(parse_expr(scanner)?);
        }
    }
    Ok(true)
}

/// Handle a leading file-descriptor word (`2` in `2>err`): if a redirect
/// operator follows, parse an fd-tagged redirect; otherwise it is a plain
/// argument.
fn parse_fd_prefixed(
    scanner: &mut Scanner,
    w: &str,
    span: Span,
    args: &mut Vec<Expr>,
    redirects: &mut Vec<Redirect>,
) -> Result<(), Diagnostic> {
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
    Ok(())
}

// ── Redirects ────────────────────────────────────────────────

/// Check for trailing redirects, pipes, and list operators after any statement.
fn maybe_wrap_redirects(scanner: &mut Scanner, stmt: Statement) -> Result<Statement, Diagnostic> {
    let result = wrap_pipeline_part(scanner, stmt)?;
    parse_trailing_list_op(scanner, result)
}

/// Collect zero or more trailing redirect operators (`>`, `>>`,
/// `<`, `>&`, `<&`, `<>`, `&>`, `<<<`, and the bare-fd-prefixed
/// `N>` / `N<` family). Stops at the first non-redirect token.
fn parse_trailing_redirects(
    scanner: &mut Scanner,
    span: Span,
) -> Result<Vec<Redirect>, Diagnostic> {
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
                redirects.push(parse_redirect(scanner, span)?);
            }
            Tok::Word(ref w) if is_fd_number(w) => {
                let next = scanner.peek_n(1).token.clone();
                if matches!(
                    next,
                    Tok::Gt | Tok::DblGt | Tok::GtAmp | Tok::LtAmp | Tok::Lt
                ) {
                    let fd: u32 = w.parse().unwrap_or(0);
                    scanner.next();
                    let mut redir = parse_redirect(scanner, span)?;
                    redir.fd = Some(fd);
                    redirects.push(redir);
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    Ok(redirects)
}

/// Wrap `stmt` in a `Redirected` node iff there are any redirects;
/// otherwise return it unchanged.
fn wrap_with_redirects(stmt: Statement, redirects: Vec<Redirect>) -> Statement {
    if redirects.is_empty() {
        return stmt;
    }
    let span = stmt.span;
    Statement {
        kind: StmtKind::Redirected(Redirected {
            body: Box::new(stmt),
            redirects,
        }),
        span,
    }
}

/// If a `|` follows, consume one more pipeline stage and wrap as a
/// `Pipeline`. Flattens nested pipelines from the right-hand side
/// so `a | b | c` produces a flat 3-stage pipeline.
fn parse_trailing_pipeline(
    scanner: &mut Scanner,
    stmt: Statement,
) -> Result<Statement, Diagnostic> {
    if !matches!(scanner.peek().token, Tok::Pipe) {
        return Ok(stmt);
    }
    let span = stmt.span;
    let mut commands = vec![stmt];
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
    Ok(if commands.len() > 1 {
        Statement {
            kind: StmtKind::Pipeline(Pipeline {
                commands,
                negated: false,
            }),
            span,
        }
    } else {
        commands.into_iter().next().unwrap()
    })
}

/// If `&&`, `||`, or `&` follows, wrap `stmt` in a `List` with the
/// matching `ListOp`. `&` produces `Background` with a synthetic
/// `true` right-hand side; the binary ops parse one more statement
/// (defaulting to `true` if the input is malformed).
fn parse_trailing_list_op(
    scanner: &mut Scanner,
    mut left: Statement,
) -> Result<Statement, Diagnostic> {
    // `&&` / `||` are equal-precedence and LEFT-associative:
    // `A && B || C` is `(A && B) || C`. Loop, folding left.
    loop {
        let op = match scanner.peek().token {
            Tok::AndAnd => ListOp::And,
            Tok::OrOr => ListOp::Or,
            Tok::Amp => return Ok(background_list_op(scanner, left)),
            _ => return Ok(left),
        };
        let span = left.span;
        scanner.next(); // consume && / ||
        skip_terminators(scanner);
        let right = parse_list_operand(scanner)?.unwrap_or_else(|| Statement::true_cmd(span));
        left = Statement {
            kind: StmtKind::List(List {
                op,
                left: Box::new(left),
                right: Box::new(right),
            }),
            span,
        };
    }
}

/// Parse one `&&`/`||` chain operand: a command + its pipeline/redirects,
/// but NOT a trailing list operator (the caller loops, left-associative).
fn parse_list_operand(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    skip_terminators(scanner);
    if scanner.at_eof() {
        return Ok(None);
    }
    let start = scanner.peek().span.start;
    let tok = scanner.peek().token.clone();
    let span = span_from(scanner, start);
    let stmt = match dispatch_statement(scanner, tok, span)? {
        Some(s) => s,
        None => return Ok(None),
    };
    if is_compound(&stmt.kind) {
        // Compounds can't take a postfix `&&`/`||`/redirect/pipe (SB4023/…).
        reject_compound_postfix(stmt, scanner)
    } else {
        Ok(Some(wrap_pipeline_part(scanner, stmt)?))
    }
}

/// Redirects + pipeline (`|`), but NOT trailing list operators.
fn wrap_pipeline_part(scanner: &mut Scanner, stmt: Statement) -> Result<Statement, Diagnostic> {
    let redirects = parse_trailing_redirects(scanner, stmt.span)?;
    let result = wrap_with_redirects(stmt, redirects);
    parse_trailing_pipeline(scanner, result)
}

fn background_list_op(scanner: &mut Scanner, left: Statement) -> Statement {
    let span = left.span;
    scanner.next();
    Statement {
        kind: StmtKind::List(List {
            op: ListOp::Background,
            left: Box::new(left),
            right: Box::new(Statement::true_cmd(span)),
        }),
        span,
    }
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
    walk_exprs(stmt, &mut |expr| {
        check_rejected_expr(expr, diags);
    });
}

fn check_rejected_expr(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::Literal(s) if s == "`backtick`" => {
            diags.push(parse_error(
                expr.span,
                "SB5020",
                "nested backticks are not allowed — use $(...) for nesting".to_string(),
            ));
        }
        ExprKind::ParameterExpansion(pe) => check_rejected_param_expansion(pe, expr.span, diags),
        _ => {}
    }
}

fn check_rejected_param_expansion(
    pe: &ParameterExpansion,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    if pe.operator.as_deref() == Some("!") {
        diags.push(parse_error(
            span,
            "SB4003",
            format!(
                "indirect expansion ${{!{}}} is not allowed in the safe subset",
                pe.name
            ),
        ));
    }
    // SB4015: reject @-transforms (${var@P}, ${var@Q}, ${var@A} etc.)
    if let Some(op) = pe.operator.as_deref() {
        if op.starts_with('@') && op.len() == 2 {
            diags.push(parse_error(
                span,
                "SB4015",
                format!(
                    "parameter transformation ${{var{}}} is not supported — use ^/^^/,/,, instead",
                    op
                ),
            ));
        }
    }
}

/// Walk all expressions in a statement, calling f for each.
fn walk_exprs(stmt: &Statement, f: &mut impl FnMut(&Expr)) {
    crate::ir::walk_all_exprs(stmt, f);
}
