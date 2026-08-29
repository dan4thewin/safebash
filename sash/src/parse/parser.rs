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
use super::expr::{
    parse_array_literal, parse_assignment_value, parse_expr, parse_operand_word, span_from,
};
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
        Tok::Case => Err(case_removed_error(span)),
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
        Tok::Word(ref w) if w == "return" => parse_return(scanner),
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
            "a redirect on a compound command isn't allowed — wrap the compound \
             in a function"
                .to_string(),
        ));
    }
    if matches!(next, Tok::Pipe) {
        return Some((
            "SB4022",
            "a pipe on a compound command isn't allowed — wrap the compound in a \
             function"
                .to_string(),
        ));
    }
    if matches!(next, Tok::AndAnd | Tok::OrOr) {
        return Some((
            "SB4023",
            "`&&` / `||` on a compound command isn't allowed — wrap the compound \
             in a function"
                .to_string(),
        ));
    }
    if matches!(next, Tok::Amp) {
        return Some((
            "SB4024",
            "`&` on a compound command isn't allowed — wrap the compound in a \
             function"
                .to_string(),
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
    // DD#66: the dispatch form is statement-only — not an `if` condition.
    if condition.iter().any(stmt_is_dispatch) {
        return Err(dispatch_stmt_only_error(span));
    }
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
    // DD#66: the dispatch form is statement-only — not a loop condition.
    if condition.iter().any(stmt_is_dispatch) {
        return Err(dispatch_stmt_only_error(span));
    }
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

    let (decl_keyword, var_type, flags) = declaration_axes(scanner, keyword);
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

/// Resolve a declaration's (keyword, type, structure-flags) axes, consuming any
/// stacked modifier keyword. Handles `const int` and — DD#76 — the top-level
/// `int array` / `int map` stack (DD#35 order: type then structure; a
/// `local int array` already stacks via parse_local). const+structure is
/// intentionally left as a compile error (spec 35.2), so it is not stacked
/// here.
fn declaration_axes(scanner: &mut Scanner, keyword: &str) -> (DeclKeyword, VarType, Vec<String>) {
    if keyword == "const" && matches!(scanner.peek().token, Tok::Int) {
        scanner.next();
        return (DeclKeyword::Readonly, VarType::Int, vec!["-r".to_string()]);
    }
    if keyword == "int" {
        match scanner.peek().token {
            Tok::Array => {
                scanner.next();
                return (DeclKeyword::Declare, VarType::Int, vec!["-a".to_string()]);
            }
            Tok::Map => {
                scanner.next();
                return (DeclKeyword::Declare, VarType::Int, vec!["-A".to_string()]);
            }
            _ => {}
        }
    }
    match keyword {
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
    }
}

fn parse_declaration_assignments(
    scanner: &mut Scanner,
    span: Span,
) -> Result<Vec<Assignment>, Diagnostic> {
    let mut assignments = Vec::new();

    loop {
        if scanner.at_eof()
            || matches!(
                scanner.peek().token,
                Tok::Newline
                    | Tok::Semi
                    | Tok::Eof
                    // Stop at a trailing redirect so `int x=5 > f` returns a clean
                    // Declaration; `wrap_pipeline_part` then rejects the redirect
                    // (SB4046) instead of the misleading SB4014 that greedy
                    // consumption of `>` used to produce.
                    | Tok::Gt
                    | Tok::DblGt
                    | Tok::Lt
                    | Tok::GtAmp
                    | Tok::LtAmp
                    | Tok::LtGt
                    | Tok::AmpGt
                    | Tok::HereString
            )
        {
            break;
        }
        assignments.push(parse_one_decl_assignment(scanner, span)?);
    }

    Ok(assignments)
}

/// Parse one `name`, `name=value`, `name=(array)`, or `name+=value` within a
/// declaration list. A scalar value uses `parse_assignment_value` (DD#71
/// value-word absorb); `=(…)` stays an array literal.
fn parse_one_decl_assignment(scanner: &mut Scanner, span: Span) -> Result<Assignment, Diagnostic> {
    let name = expect_word(scanner, span, "expected variable name in declaration")?;

    if matches!(scanner.peek().token, Tok::Eq) {
        scanner.next(); // consume =
        if matches!(scanner.peek().token, Tok::LParen) {
            scanner.next();
            let arr = parse_array_literal(scanner, span)?;
            return Ok(Assignment {
                name,
                index: None,
                op: AssignOp::Set,
                value: arr,
            });
        }
        let value = parse_assignment_value(scanner)?;
        return Ok(Assignment {
            name,
            index: None,
            op: AssignOp::Set,
            value,
        });
    }

    if matches!(scanner.peek().token, Tok::PlusAssign) {
        scanner.next(); // consume +=
        let value = parse_assignment_value(scanner)?;
        return Ok(Assignment {
            name,
            index: None,
            op: AssignOp::Append,
            value,
        });
    }

    // Bare declaration: int x
    Ok(Assignment {
        name,
        index: None,
        op: AssignOp::Set,
        value: Expr {
            kind: ExprKind::Literal(String::new()),
            span: Span { line: 0, col: 0 },
        },
    })
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

/// DD#66: the dispatch form is statement-only. True if `stmt` is a `[[ ]]`
/// dispatch — every `Case` is a dispatch now that `case…esac` is removed.
fn stmt_is_dispatch(stmt: &Statement) -> bool {
    matches!(&stmt.kind, StmtKind::Case(_))
}

fn dispatch_stmt_only_error(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4028",
        "the `[[ ]]` dispatch form is a statement, not a boolean — use it as a \
         standalone statement (or `if BOOLEAN; then DISPATCH; fi`)"
            .to_string(),
    )
}

/// DD#66: `case`/`esac` are removed — the `[[ ]]` dispatch form replaces them.
/// `case` remains a reserved word so its use produces this diagnostic instead
/// of a confusing "unexpected word" error.
fn case_removed_error(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4029",
        "the `case` construct has been removed — use the `[[ ]]` dispatch \
         form: `[[ $x == pat { … } other { … } || { … } ]]` (exact match) or \
         `[[ $x =~ regex { … } || { … } ]]` (regex match)"
            .to_string(),
    )
}

/// DD#74: `return` — either the value form (`return N` / `return $?` / bare,
/// the existing command path) or a producer form (`return [[ … ]]` /
/// `return (( … ))` / `return CMD`). The producer's status becomes the
/// function's; it is consumed (never aborts). Disambiguation: `[[`/`((`, or a
/// non-numeric command word after `return`, is a producer; a numeric literal,
/// a `$`-expansion, or end-of-statement is a value.
fn parse_return(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let after = scanner.peek_n(1).token.clone();
    let is_producer = matches!(after, Tok::DblBracketOpen | Tok::DblParen)
        || matches!(&after, Tok::Word(w) if !w.chars().all(|c| c.is_ascii_digit()));
    if !is_producer {
        return parse_simple_or_assignment(scanner);
    }
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'return'
    let producer = match scanner.peek().token {
        Tok::DblBracketOpen => parse_test_command(scanner)?,
        Tok::DblParen => parse_arith_stmt(scanner)?,
        _ => parse_simple_or_assignment(scanner)?,
    };
    match producer {
        Some(p) => Ok(Some(Statement {
            kind: StmtKind::ReturnProducer(Box::new(p)),
            span,
        })),
        None => Err(parse_error(
            span,
            "SB4014",
            "return: expected a status producer (a command, `[[ … ]]`, or `(( … ))`)".to_string(),
        )),
    }
}

fn parse_negated(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume '!'
    skip_terminators(scanner);

    // `!` negates a single reviewer-atomic unit — one command, `[[ ]]`, or
    // `(( ))` — NOT a multi-stage pipeline (a reviewer shouldn't have to know
    // how far `!` reaches; sash is easier to review than bash). Parse just the
    // unit via `dispatch_statement` (which does NOT fold `|`/`&&`); the caller's
    // `maybe_wrap_redirects` then folds any trailing `&&`/`||`, so `! X && Y`
    // reassociates to `(!X) && Y` (bash-consistent) instead of `!(X && Y)`.
    let start = scanner.peek().span.start;
    let tok = scanner.peek().token.clone();
    let unit_span = span_from(scanner, start);
    let Some(stmt) = dispatch_statement(scanner, tok, unit_span)? else {
        return Ok(None);
    };
    // DD#66: `! [[ dispatch ]]` — the dispatch is statement-only.
    if stmt_is_dispatch(&stmt) {
        return Err(dispatch_stmt_only_error(span));
    }
    // Reviewability (SB4040): `!` on a pipeline (`! a | b`) is rejected — a
    // trailing `|`, or an already-folded multi-stage pipeline. Escape hatch:
    // wrap the pipeline in a function and negate the function.
    let pipeline_operand = matches!(scanner.peek().token, Tok::Pipe)
        || matches!(&stmt.kind, StmtKind::Pipeline(p) if p.commands.len() > 1);
    if pipeline_operand {
        return Err(negate_pipeline_error(span));
    }
    // Wrap the single unit in a negated 1-command pipeline.
    Ok(Some(Statement {
        kind: StmtKind::Pipeline(Pipeline {
            commands: vec![stmt],
            negated: true,
        }),
        span,
    }))
}

/// SB4040: `!` cannot negate a multi-stage pipeline (reviewability — a reviewer
/// can't see how far `!` reaches). Direct to the function escape hatch.
fn negate_pipeline_error(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4040",
        "`!` cannot negate a pipeline — a reviewer can't tell how far `!` \
         reaches. Wrap the pipeline in a predicate function and negate that: \
         `f() { if a | b; then return 0; fi; return 1 }` then `if ! f; then …`"
            .to_string(),
    )
}

// ── suppressed command (- cmd) ───────────────────────────────

fn parse_suppressed(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);

    // Peek ahead: if what follows looks like a command, treat - as suppression
    let next = scanner.peek_n(1);
    // DD#70: `- !` suppresses a boolean → SB4019 directly (else it falls through
    // to `parse_simple_or_assignment` and reports the garbage SB4014). `-`
    // cannot suppress `!`.
    if matches!(next.token, Tok::Bang) {
        return Err(parse_error(
            span,
            "SB4019",
            "`-` cannot suppress `!` — it is a boolean that must be consumed \
             (if/||) or discarded with `|| :`"
                .to_string(),
        ));
    }
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
            Ok(Some(push_suppression_into_pipeline(stmt, span)))
        } else {
            Ok(None)
        }
    } else {
        // Not a suppressed command — treat as simple command starting with -
        parse_simple_or_assignment(scanner)
    }
}

/// A leading `-` on a pipeline suppresses only the FIRST segment (DD#55),
/// so normalize `Suppressed(Pipeline[s0, s1..])` to
/// `Pipeline[Suppressed(s0), s1..]` — a uniform per-segment representation
/// matching the non-first `cmd1 | - cmd2` form. A `-` on any other statement
/// suppresses that whole statement, as before.
fn push_suppression_into_pipeline(stmt: Statement, span: Span) -> Statement {
    let stmt_span = stmt.span;
    match stmt.kind {
        StmtKind::Pipeline(mut p) if !p.commands.is_empty() => {
            let first = p.commands.remove(0);
            let fspan = first.span;
            p.commands.insert(
                0,
                Statement {
                    kind: StmtKind::Suppressed(Box::new(first)),
                    span: fspan,
                },
            );
            Statement {
                kind: StmtKind::Pipeline(p),
                span: stmt_span,
            }
        }
        other => Statement {
            kind: StmtKind::Suppressed(Box::new(Statement {
                kind: other,
                span: stmt_span,
            })),
            span,
        },
    }
}

// ── [[ test ]] ───────────────────────────────────────────────

fn parse_test_command(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume [[

    let test_expr = parse_test_expr(scanner, span)?;

    // DD#66/#71: dispatch form — a `{`, or (for `==`) additional bare-word
    // patterns, after the first `SUBJECT ==|=~ PATTERN` switches from the
    // boolean test to a multi-way dispatch statement.
    if matches!(scanner.peek().token, Tok::LBrace) || dispatch_multi_follows(&test_expr, scanner) {
        return parse_dispatch(scanner, span, test_expr);
    }

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

/// DD#71: multi-exact dispatch signal — a bare word (another pattern)
/// following a `==` binary test, as in `[[ $x == a b { … } ]]`. `=~` (and
/// any other op) require an explicit `{` to enter dispatch; a bare word
/// after them is not a second pattern (a `=~` arm is a single regex).
fn dispatch_multi_follows(test_expr: &TestExpr, scanner: &mut Scanner) -> bool {
    matches!(test_expr, TestExpr::Binary { op, .. } if op == "==")
        && scanner.peek().token.is_word_like()
}

/// Collect the patterns of one dispatch arm (up to the arm `{`), starting
/// from `pats` (the first, already-parsed pattern). For `==`, multiple
/// space-separated exact patterns are allowed (first-match-wins within the
/// arm); a `=~` arm is a single regex, so no more are collected.
fn collect_arm_patterns(
    scanner: &mut Scanner,
    mut pats: Vec<Expr>,
    op: &str,
) -> Result<Vec<Expr>, Diagnostic> {
    if op == "==" {
        while !matches!(
            scanner.peek().token,
            Tok::LBrace | Tok::DblBracketClose | Tok::OrOr | Tok::AndAnd | Tok::Newline | Tok::Eof
        ) {
            pats.push(parse_operand_word(scanner)?);
        }
    }
    Ok(pats)
}

/// DD#66: parse the `[[ ]]` dispatch form. The first
/// `SUBJECT (==|=~) PATTERN` was already parsed as `first` (a Binary
/// test); the cursor is at the first arm's `{`. Grammar:
/// `PATTERN { group }` (repeated) then an optional `|| { group }` default,
/// closed by `]]`. Lowers to `ir::Case` carrying the `==`/`=~` op.
fn parse_dispatch(
    scanner: &mut Scanner,
    span: Span,
    first: TestExpr,
) -> Result<Option<Statement>, Diagnostic> {
    let TestExpr::Binary { left, op, right } = first else {
        return Err(parse_error(
            span,
            "SB4028",
            "the `[[ ]]` dispatch form needs `SUBJECT ==|=~ PATTERN { … }`".to_string(),
        ));
    };
    if op != "==" && op != "=~" {
        return Err(parse_error(
            span,
            "SB4028",
            format!("`[[ ]]` dispatch uses `==` or `=~`, not `{op}`"),
        ));
    }

    let value = *left;
    let first_pats = collect_arm_patterns(scanner, vec![*right], &op)?;
    let mut items = vec![CaseItem {
        patterns: first_pats,
        body: parse_brace_block(scanner, span)?,
        is_default: false,
    }];

    loop {
        match scanner.peek().token {
            Tok::OrOr => {
                // `|| { group }` — the default arm (must be last). No synthetic
                // `*` pattern: DD#71 makes `*` an ordinary literal, so the
                // default is flagged instead.
                scanner.next();
                items.push(CaseItem {
                    patterns: vec![],
                    body: parse_brace_block(scanner, span)?,
                    is_default: true,
                });
                break;
            }
            Tok::DblBracketClose => break,
            _ => {
                let first = parse_operand_word(scanner)?;
                let pats = collect_arm_patterns(scanner, vec![first], &op)?;
                items.push(CaseItem {
                    patterns: pats,
                    body: parse_brace_block(scanner, span)?,
                    is_default: false,
                });
            }
        }
    }

    expect(scanner, &Tok::DblBracketClose, span, "expected ']]'")?;

    Ok(Some(Statement {
        kind: StmtKind::Case(Case { value, items, op }),
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

/// Try to extract a compact `==` or `=~` operator from an Expr.
/// Returns Option<(left_expr, op, right_expr)> if successful.
fn extract_compact_operator(expr: &Expr) -> Option<(Expr, String, Expr)> {
    match &expr.kind {
        ExprKind::Literal(s) => extract_from_literal(s, expr.span),
        ExprKind::Concat(parts) => extract_from_concat(parts, expr.span),
        _ => None,
    }
}

/// Extract compact operator from a literal string (e.g., "a==b").
fn extract_from_literal(s: &str, span: Span) -> Option<(Expr, String, Expr)> {
    if let Some(pos) = s.find("==") {
        let (left, right) = (&s[..pos], &s[pos + 2..]);
        if !left.is_empty() && !right.is_empty() {
            return Some((
                Expr {
                    kind: ExprKind::Literal(left.to_string()),
                    span,
                },
                "==".to_string(),
                Expr {
                    kind: ExprKind::Literal(right.to_string()),
                    span,
                },
            ));
        }
    }
    if let Some(pos) = s.find("=~") {
        let (left, right) = (&s[..pos], &s[pos + 2..]);
        if !left.is_empty() && !right.is_empty() {
            return Some((
                Expr {
                    kind: ExprKind::Literal(left.to_string()),
                    span,
                },
                "=~".to_string(),
                Expr {
                    kind: ExprKind::Literal(right.to_string()),
                    span,
                },
            ));
        }
    }
    None
}

/// Extract compact operator from a Concat expression.
/// Handles Concat([..., Literal("=="), ...]) and Concat([..., Literal("==rest")])
fn extract_from_concat(parts: &[Expr], span: Span) -> Option<(Expr, String, Expr)> {
    // Check for operator as a middle part: Concat([a, Literal("=="), b])
    for (i, part) in parts.iter().enumerate() {
        if i > 0 && i < parts.len() - 1 {
            if let ExprKind::Literal(s) = &part.kind {
                if let Some(op) = match_operator(s) {
                    let left = build_expr_from_parts(&parts[..i], span);
                    let right = build_expr_from_parts(&parts[i + 1..], span);
                    return Some((left, op, right));
                }
            }
        }
    }

    // Check for operator in last part: Concat([..., Literal("==rest")])
    if let Some(last_part) = parts.last() {
        if let ExprKind::Literal(s) = &last_part.kind {
            if let Some((op_str, rest_start)) = extract_leading_operator(s) {
                let left = build_expr_from_parts(&parts[..parts.len() - 1], span);
                return Some((
                    left,
                    op_str,
                    Expr {
                        kind: ExprKind::Literal(s[rest_start..].to_string()),
                        span,
                    },
                ));
            }
        }
    }
    None
}

/// Match if a literal is exactly an operator ("==" or "=~").
fn match_operator(s: &str) -> Option<String> {
    if s == "==" {
        Some("==".to_string())
    } else if s == "=~" {
        Some("=~".to_string())
    } else {
        None
    }
}

/// Check if a literal starts with an operator; return (op, rest_index).
fn extract_leading_operator(s: &str) -> Option<(String, usize)> {
    if s.starts_with("==") && s.len() > 2 {
        Some(("==".to_string(), 2))
    } else if s.starts_with("=~") && s.len() > 2 {
        Some(("=~".to_string(), 2))
    } else {
        None
    }
}

/// Build an Expr from a slice of Expr parts.
fn build_expr_from_parts(parts: &[Expr], span: Span) -> Expr {
    if parts.is_empty() {
        Expr {
            kind: ExprKind::Literal(String::new()),
            span,
        }
    } else if parts.len() == 1 {
        parts[0].clone()
    } else {
        Expr {
            kind: ExprKind::Concat(parts.to_vec()),
            span,
        }
    }
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
        _ => {
            // Try to extract a compact `==` or `=~` operator from the left operand
            // (e.g., `$x==a` in `[[ $x==a { ... } ]]`)
            if let Some((left_expr, op, right_expr)) = extract_compact_operator(&left) {
                return Ok(TestExpr::Binary {
                    left: Box::new(left_expr),
                    op,
                    right: Box::new(right_expr),
                });
            }
            Ok(implicit_nonempty(left))
        }
    }
}

fn parse_binary_rhs(
    left: Expr,
    op: String,
    scanner: &mut Scanner,
    span: Span,
) -> Result<TestExpr, Diagnostic> {
    let right = if op == "=~" {
        // DD#71: the `=~` RHS is an operand word (metachars literal, quote/
        // $-aware, absorbed to the next whitespace/`]]`). No bespoke regex
        // collector.
        parse_operand_word(scanner)?
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
            | "-i"
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
            "`eval` is rejected — a sash program's behavior is fixed at compile \
             time; there is no runtime interpreter to run constructed code"
                .to_string(),
        )),
        "source" | "." => Some((
            "SB4002",
            "`source` / `.` is rejected — a sash program's behavior is fixed at \
             compile time"
                .to_string(),
        )),
        "export" => Some((
            "SB5001",
            "sash has no `export` — use `/usr/bin/env NAME=value …` to set a \
             variable for a command"
                .to_string(),
        )),
        "declare" | "typeset" => Some((
            "SB5001",
            format!(
                "sash declares with `int` / `const` / `array` / `map`, not `{}`",
                w
            ),
        )),
        "readonly" => Some((
            "SB5001",
            "sash declares read-only values with `const`, not `readonly`".to_string(),
        )),
        "select" => Some(("SB5001", "sash has no `select`".to_string())),
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

    let value = parse_assignment_value(scanner)?;

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

/// Parse regex mutation: `var =~ s/RE/SUB/[flags]`. Desugars to
/// `var = $(sub|gsub "$var" RE SUB)` (DD#63) — RE/SUB are re-parsed as
/// interpolating words, so `$x` in a field expands. The `s/…` text is
/// stitched back from the adjacent tokens (the lexer split any embedded
/// `$x`/`${x}` into their own tokens). A payload missing a mandatory `/`
/// delimiter is SB4032.
fn parse_regex_mutation(
    scanner: &mut Scanner,
    span: Span,
    var_name: &str,
) -> Result<Option<Statement>, Diagnostic> {
    scanner.next(); // consume variable name
    scanner.next(); // consume =~
    let raw = reconstruct_sed_raw(scanner);
    let body = raw
        .strip_prefix("s/")
        .ok_or_else(|| sed_malformed_err(span))?;
    validate_static_sed_re(body, span)?;
    let subject = quoted_var_subject(var_name, span);
    let value =
        super::expr::build_sed_sub(body, subject, span).ok_or_else(|| sed_malformed_err(span))?;
    Ok(Some(Statement {
        kind: StmtKind::Assignment(Assignment {
            name: var_name.to_string(),
            index: None,
            op: AssignOp::Set,
            value,
        }),
        span,
    }))
}

/// Stitch the `s/RE/SUB/flags` text back from the adjacent tokens after
/// `=~` (the lexer split any embedded `$x`/`${x}` into their own tokens).
fn reconstruct_sed_raw(scanner: &mut Scanner) -> String {
    let mut raw = String::new();
    while !scanner.at_eof() {
        if !raw.is_empty() && !scanner.next_is_adjacent() {
            break;
        }
        match &scanner.peek().token {
            Tok::Word(w) => raw.push_str(w),
            Tok::Dollar(v) => {
                raw.push('$');
                raw.push_str(v);
            }
            Tok::DollarBrace(c) => raw.push_str(&format!("${{{}}}", c)),
            _ => break,
        }
        scanner.next();
    }
    raw
}

fn sed_malformed_err(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4032",
        "malformed `=~ s/…` substitution — expected s/RE/SUB/[flags] with three `/` delimiters"
            .to_string(),
    )
}

/// Preserve the DD#39 compile-time regex check on the `=~` mutation form: a
/// STATIC (non-interpolating) pattern is validated now; an interpolating one
/// (`$x` in the pattern) is a runtime regcomp check.
fn validate_static_sed_re(body: &str, span: Span) -> Result<(), Diagnostic> {
    let Some((re, _, _)) = super::expr::split_sed_fields(body) else {
        return Ok(());
    };
    if !re.contains('$') {
        if let Err(msg) = crate::regex_validate::validate_regex(re) {
            return Err(parse_error(
                span,
                "SB5023",
                format!("invalid regex in substitution: {}", msg),
            ));
        }
    }
    Ok(())
}

/// `"$var"` as a single interpolating substitution subject.
fn quoted_var_subject(var_name: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::DoubleQuoted(vec![Expr {
            kind: ExprKind::SimpleExpansion(var_name.to_string()),
            span,
        }]),
        span,
    }
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
            append_pipeline_stage(&mut commands, next_stmt);
            break;
        }
    }
    // DD#55 / SB4023: `pipeline && C` / `pipeline || C` is rejected — see
    // reject_pipeline_and_or_postfix.
    reject_pipeline_and_or_postfix(&commands, span)?;
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

/// Append a parsed `|`-stage, flattening a nested pipeline (right-associative
/// `a | b | c` → one flat stage list).
fn append_pipeline_stage(commands: &mut Vec<Statement>, next_stmt: Statement) {
    match next_stmt.kind {
        StmtKind::Pipeline(p) => commands.extend(p.commands),
        _ => commands.push(next_stmt),
    }
}

/// DD#55 / SB4023: `pipeline && C` / `pipeline || C` is rejected — a `&&`/
/// `||` after a pipeline is ambiguous postfix (what does it test?), the same
/// rule as `&&`/`||` on a compound. The recursive stage parse absorbs the
/// trailing `&&`/`||` into the last stage as a `List`, so a `List(And|Or)`
/// last stage is exactly that rejected form. (`A && a | b` — a pipeline as
/// the *consequent* — parses cleanly and is not caught here.)
fn reject_pipeline_and_or_postfix(commands: &[Statement], span: Span) -> Result<(), Diagnostic> {
    let is_and_or = commands.last().is_some_and(|last| {
        matches!(
            last.kind,
            StmtKind::List(List {
                op: ListOp::And | ListOp::Or,
                ..
            })
        )
    });
    if is_and_or {
        return Err(parse_error(
            span,
            "SB4023",
            "`&&` / `||` on a pipeline isn't allowed — use `if pipeline; then …; fi`".to_string(),
        ));
    }
    Ok(())
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
    if !redirects.is_empty() {
        if let Some(what) = noncommand_redirect_target(&stmt.kind) {
            return Err(parse_error(
                stmt.span,
                "SB4046",
                format!("a redirect cannot attach to {what} — redirects attach to a command"),
            ));
        }
    }
    let result = wrap_with_redirects(stmt, redirects);
    parse_trailing_pipeline(scanner, result)
}

/// A redirect may only attach to a command. If `kind` is a function
/// definition, a variable declaration, or a variable assignment, a trailing
/// redirect is illegal (SB4046) — returns the noun for the diagnostic; `None`
/// means a redirect is allowed on this statement kind. (Compound commands are
/// handled separately by `reject_compound_postfix`, SB4021.)
fn noncommand_redirect_target(kind: &StmtKind) -> Option<&'static str> {
    match kind {
        StmtKind::Function(_) => Some("a function definition"),
        StmtKind::Declaration(_) => Some("a variable declaration"),
        StmtKind::Assignment(_) => Some("a variable assignment"),
        _ => None,
    }
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
                "nested backticks are ambiguous — use $(...) for nesting".to_string(),
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
    if let Some(d) = rejected_param_operator_diag(pe, span) {
        diags.push(d);
    }
}

/// The reject diagnostic for a rejected `${var:OP}` operator, or `None`. The
/// operator is a single value, so at most one applies: indirect `${!x}`
/// (SB4003), `@`-transforms (SB4015), and the DD#65 pruned modifier family
/// (SB4034).
fn rejected_param_operator_diag(pe: &ParameterExpansion, span: Span) -> Option<Diagnostic> {
    let op = pe.operator.as_deref()?;
    if op == "!" {
        return Some(parse_error(
            span,
            "SB4003",
            format!(
                "indirect expansion ${{!{}}} is rejected — sash resolves every \
                 variable reference at compile time",
                pe.name
            ),
        ));
    }
    if op == "cell_string_op" {
        return Some(parse_error(
            span,
            "SB4039",
            format!(
                "substring / `:s/…` on an array cell (`${{{}[…]:…}}`) isn't \
                 supported yet — copy the cell into a scalar first, then apply \
                 the operator",
                pe.name
            ),
        ));
    }
    if op.starts_with('@') && op.len() == 2 {
        return Some(parse_error(
            span,
            "SB4015",
            format!(
                "`${{var{}}}` isn't part of sash — use the $(toupper …) / \
                 $(tolower …) builtins",
                op
            ),
        ));
    }
    // DD#75 §4: the `[@]`/`[*]` bracket-subscript collection forms are retired
    // — `@a` is the sole splat. Reject each with its canonical replacement.
    if let Some(d) = retired_bracket_operator_diag(pe, op, span) {
        return Some(d);
    }
    // DD#65: the pruned `${var:OP}` family — every `split_operator` op EXCEPT
    // `:-` (assign-default/alt/error, prefix/suffix strip, glob replace, case).
    // Rejects uniformly; migration to the builtins is sash-convert's job (see
    // task `converter-var-modifier-migration`).
    if is_dropped_param_operator(op) {
        return Some(parse_error(
            span,
            "SB4034",
            format!(
                "the `{}` parameter operator (in ${{{}…}}) isn't part of sash — \
                 use the string builtins (toupper/tolower, sub/gsub, basename/dirname) \
                 or the :- / :off:len / :s/ forms",
                user_facing_operator(op),
                pe.name
            ),
        ));
    }
    None
}

/// DD#75 §4: reject a retired collection form (`[@]`/`[*]` bracket subscripts)
/// or the retired `${#…}` length sugar, pointing at its canonical replacement.
/// `@a` is the sole splat; length is `$(len a)`; keys are `$(keys m)` (maps
/// only). Returns `None` for any non-retired operator.
fn retired_bracket_operator_diag(
    pe: &ParameterExpansion,
    op: &str,
    span: Span,
) -> Option<Diagnostic> {
    let name = &pe.name;
    let msg = match op {
        "arrvals_retired" => format!(
            "the `[@]`/`[*]` array splat (`${{{name}[@]}}` / `${name}[@]`) is \
             retired — use `@{name}` for the splat, or `$(join {name})` for a \
             joined string"
        ),
        "len_retired" => format!(
            "the `${{#…}}` length sugar (`${{#{name}}}`) is retired — use \
             `$(len {name})` in value context or `(len {name})` inside `$(( ))`"
        ),
        "arrkeys_retired" => format!(
            "the `[@]`/`[*]` keys form `${{!{name}[@]}}` is retired — use \
             `$(keys {name})` (maps only; an indexed array's keys are 0..len-1)"
        ),
        _ => return None,
    };
    Some(parse_error(span, "SB4038", msg))
}

/// Map an internal dropped-operator string to the form the user wrote, for
/// the SB4034 diagnostic: `glob_replace` → `/` (the `/`-family remap), the
/// per-element array form `arrvals_<op>` → `[@]<op>`, everything else verbatim.
fn user_facing_operator(op: &str) -> String {
    if op == "glob_replace" {
        return "/".to_string();
    }
    if let Some(core) = op.strip_prefix("arrvals_") {
        return format!("[@]{}", core);
    }
    op.to_string()
}

/// DD#65: operators dropped from the `${var:OP}` family — the full
/// `split_operator` set minus the kept `:-`. The `/`-family glob-replace is
/// remapped to `glob_replace` at parse (parse_standard_operator) so it is
/// distinguishable here from the kept `:s/` sed op, which shares the raw
/// `/`//` operator strings. The per-element **array** forms `${arr[@]OP}`
/// (operator `arrvals_<op>`, `parse_subscript_operator`) are dropped too, for
/// consistency with the scalar drop — the replacement is a `for x in @a` loop
/// with `$(sub …)`/`$(gsub …)`. Kept forms (`substring`/`:off:len`, `:s/`,
/// `strlen`, plain `arrvals`/`arrkeys`/`arrjoin` join, `glob`) arrive via other
/// parse paths / operators and are not matched here.
fn is_dropped_param_operator(op: &str) -> bool {
    // Per-element array form: `arrvals_<raw op>` (the array parse path keeps the
    // raw `/`//`, unlike the scalar `glob_replace` remap). Plain `arrvals`
    // (`${a[@]}` join) has no trailing `_op` and is kept.
    if let Some(core) = op.strip_prefix("arrvals_") {
        return matches!(
            core,
            "/" | "//" | "#" | "##" | "%" | "%%" | "^" | "^^" | "," | ",,"
        );
    }
    matches!(
        op,
        ":=" | ":+" | "#" | "##" | "%" | "%%" | "glob_replace" | "^" | "^^" | "," | ",,"
    )
}

/// Walk all expressions in a statement, calling f for each.
fn walk_exprs(stmt: &Statement, f: &mut impl FnMut(&Expr)) {
    crate::ir::walk_all_exprs(stmt, f);
}
