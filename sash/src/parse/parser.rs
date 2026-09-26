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
//!   - Declarations: parse_function, parse_declaration
//!   - Commands: parse_simple_or_assignment, parse_command
//!   - Composition: maybe_wrap_redirects (pipes, list ops, redirects)
//!   - Test expressions: parse_test_expr (recursive descent for [[ ]])
//!   - Postfix rejection: SB4021–4024 on compound commands

use super::arith::{
    collect_arith_content_raw, lower_arith_command, lower_c_style_for, parse_arith_command,
};
use super::error::parse_error;
use super::expr::{
    is_identifier, keyword_to_str, parse_array_literal, parse_assignment_value, parse_expr,
    parse_operand_word, parse_subscript_index, span_from,
};
use super::lexer::{Scanner, Tok};
use crate::diagnostic::Diagnostic;
use crate::ir::*;

/// Parse sash-native source into IR.
/// Returns the program or a list of diagnostics on failure.
/// DD#56: consume one `END { }` at the top level into `end_block`.
///
/// Split out of `parse` for the complexity gate. `end_span` doubles as the
/// seen-flag and as the line number SB4055 quotes back.
fn take_end_block(
    scanner: &mut Scanner,
    end_block: &mut Option<Vec<Statement>>,
    end_span: &mut Option<Span>,
    diags: &mut Vec<Diagnostic>,
) {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    let body = match parse_end_block(scanner) {
        Ok(body) => body,
        Err(d) => {
            diags.push(d);
            recover(scanner);
            return;
        }
    };
    match *end_span {
        Some(prev) => diags.push(parse_error(
            span,
            "SB4055",
            format!(
                "a second `END {{ }}` block — a program has one exit handler, \
                 and the first (line {}) would never run. Merge the two bodies.",
                prev.line
            ),
        )),
        None => {
            *end_span = Some(span);
            *end_block = Some(body);
        }
    }
}

/// The scanner's own fatal diagnostics, in report-first order.
///
/// Each of these makes the rest of the parse meaningless, so exactly one is
/// reported and the deferred `diags` are dropped. The order is the order of
/// consequence, not of severity: an unterminated quote swallows the source
/// after it, so anything else the parse turned up is downstream of that one
/// mistake and must not be reported alongside it.
fn scanner_fatal(scanner: &Scanner) -> Option<Diagnostic> {
    // An unterminated quote / `${` swallows everything after it, so whatever
    // else the parse turned up is downstream of that one mistake. Report it
    // first and alone (SB4057).
    if let Some((open_at, what)) = scanner.unterminated() {
        let (line, col) = scanner.line_col(open_at);
        return Some(parse_error(
            Span::new(line, col),
            "SB4057",
            format!(
                "unterminated {what} — it is never closed before the end of \
                 the source, so everything after it was read as part of it"
            ),
        ));
    }

    // A rejected `$'…'` escape (DD#115: an archaic bash spelling is a
    // loud error, never a silent difference). The escape is resolved in the
    // lexer, so this is the only channel through which it can surface.
    if let Some((at, why)) = scanner.bad_escape() {
        let (line, col) = scanner.line_col(at);
        let why = why.to_string();
        return Some(parse_error(
            Span::new(line, col),
            "SB4063",
            format!("unsupported escape in `$'…'` — {why}"),
        ));
    }

    // A `#` glued to an operator (`echo a;# c`) is a comment in bash and a word
    // here — the rest of the line becomes code, so every later diagnostic is a
    // consequence. Report it first and alone.
    if let Some(at) = scanner.attached_hash() {
        let (line, col) = scanner.line_col(at);
        return Some(parse_error(
            Span::new(line, col),
            "SB4058",
            "a `#` comment must be preceded by a space — this one is glued to \
             the operator before it, so sash reads it as a word, not a comment \
             (bash would comment here; sash requires the space so an interior \
             `#`, as in `a#b` or `16#ff`, can stay part of the word)"
                .to_string(),
        ));
    }

    None
}

pub fn parse(source: &str) -> Result<(Program, Vec<Diagnostic>), Vec<Diagnostic>> {
    let mut scanner = Scanner::new(source);
    let mut stmts = Vec::new();
    let mut diags = Vec::new();
    let mut end_block: Option<Vec<Statement>> = None;
    let mut end_span: Option<Span> = None;

    while !scanner.at_eof() {
        skip_terminators(&mut scanner);
        if scanner.at_eof() {
            break;
        }
        parse_top_level_item(
            &mut scanner,
            &mut stmts,
            &mut end_block,
            &mut end_span,
            &mut diags,
        );
    }

    if let Some(d) = scanner_fatal(&scanner) {
        return Err(vec![d]);
    }

    // A deferred diagnostic is not fatal at parse time; only an Error is.
    if diags
        .iter()
        .any(|d| d.severity == crate::diagnostic::Severity::Error)
    {
        return Err(diags);
    }

    let program = Program {
        statements: stmts,
        end_block,
    };
    assert_no_magic_sugar(&program);
    Ok((program, diags))
}

/// DD#111 invariant: **no magic variable survives parse.** Every `$X`
/// punctuation form is sugar the front end desugars into its intrinsic
/// (`magic_sugar_expansion`), so a `SimpleExpansion` bearing one of those
/// names downstream means a construction site was missed.
///
/// This exists because a missed site is otherwise *silent*: that one context
/// would keep a magic variable, every spec would still pass, and the checks
/// that look for the intrinsic form — SB5012's `$?` state machine above all —
/// would simply stop seeing reads there. An invariant worth stating is worth
/// failing on, so this panics rather than warns. It cannot fire on user input;
/// only on a compiler bug.
fn assert_no_magic_sugar(program: &Program) {
    let check = |stmt: &Statement| {
        crate::ir::walk_all_exprs(stmt, &mut |e| {
            if let ExprKind::SimpleExpansion(n) = &e.kind {
                assert!(
                    super::expr::magic_sugar_expansion(n, e.span).is_none(),
                    "ICE: `${}` survived parse as a SimpleExpansion; a construction site is not routed through magic_sugar_expansion (DD#111)",
                    n
                );
            }
        });
    };
    for stmt in &program.statements {
        check(stmt);
    }
    for stmt in program.end_block.iter().flatten() {
        check(stmt);
    }
}

/// One top-level item: an `END { }` block or a statement. Split out of `parse`
/// for the complexity gate.
fn parse_top_level_item(
    scanner: &mut Scanner,
    stmts: &mut Vec<Statement>,
    end_block: &mut Option<Vec<Statement>>,
    end_span: &mut Option<Span>,
    diags: &mut Vec<Diagnostic>,
) {
    // DD#56: `END { }` is the program's exit handler, not a statement. It is
    // intercepted here rather than in `dispatch_statement` so that it lands in
    // `Program::end_block`, and so that an `END` anywhere else is by
    // construction nested and rejected (SB4054).
    if matches!(scanner.peek().token, Tok::End) {
        take_end_block(scanner, end_block, end_span, diags);
        return;
    }

    match parse_statement(scanner) {
        // Constructs parsed only so they can be reported, not accepted.
        Ok(Some(stmt)) => {
            check_rejected(&stmt, diags);
            stmts.push(stmt);
        }
        Ok(None) => {}
        Err(d) => {
            diags.push(d);
            recover(scanner); // skip to the next line and keep going
        }
    }
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
        Tok::If => parse_if(scanner),
        Tok::While | Tok::Until => parse_while(scanner),
        Tok::For => parse_for(scanner),
        Tok::Case => Err(case_removed_error(span)),
        Tok::Fn | Tok::Function => parse_function(scanner),
        Tok::Str => parse_declaration(scanner, "str"),
        Tok::Int => parse_declaration(scanner, "int"),
        Tok::Const => parse_declaration(scanner, "const"),
        Tok::Array => parse_declaration(scanner, "array"),
        Tok::Map => parse_declaration(scanner, "map"),
        Tok::End => Err(parse_error(
            span,
            "SB4054",
            "an `END { }` block must be at the top level — it is the program's \
             exit handler, not a statement that can be nested or made conditional"
                .to_string(),
        )),
        Tok::LBrace => parse_group(scanner),
        Tok::DblParen => parse_arith_stmt(scanner),
        Tok::Bang => parse_negated(scanner),
        Tok::Dash => parse_suppressed(scanner),
        Tok::Word(ref w) if w == "-" => parse_suppressed(scanner),
        Tok::DblBracketOpen => parse_test_command(scanner),
        Tok::Attest => parse_attest(scanner),
        Tok::Word(ref w) if w == "return" => parse_return(scanner),
        Tok::Word(_) if is_paren_function(scanner) => parse_paren_function(scanner),
        _ => parse_simple_or_assignment(scanner),
    }
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
    let content = collect_arith_content_raw(scanner, span)?;

    // Split on ';' — should give us 3 parts
    let parts: Vec<&str> = content.splitn(3, ';').collect();
    let init = parts.first().map(|s| s.trim()).unwrap_or("");
    let cond = parts.get(1).map(|s| s.trim()).unwrap_or("");
    let update = parts.get(2).map(|s| s.trim()).unwrap_or("");

    skip_terminators(scanner);
    let body = parse_loop_body(scanner, span)?;

    Ok(Some(lower_c_style_for(init, cond, update, body, span)?))
}

// ── function ─────────────────────────────────────────────────

fn parse_function(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'fn' or 'function'

    let name = expect_word(scanner, span, "expected function name")?;

    parse_optional_empty_parens(scanner, span)?;
    Ok(Some(finish_function_def(scanner, name, span)?))
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
    Ok(Some(finish_function_def(scanner, name, span)?))
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

    Ok(Statement {
        kind: StmtKind::Function(Function {
            name,
            return_type,
            body,
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

/// Consume an optional `-> int` return-type annotation. The lexer always emits
/// `>` as its own token, so the arrow arrives **split** as `-` `>` for every
/// spelling (`->int`, `-> int`, `()  ->  int`, `  ->  int`); there is no single
/// `Word("->")` form to accept.
fn parse_optional_return_type(scanner: &mut Scanner) -> VarType {
    let Tok::Word(w) = &scanner.peek().token else {
        return VarType::String;
    };
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
/// `int array` / `int map` stack (DD#35 order: type then structure).
/// const+structure is intentionally left as a compile error (spec 35.2), so it
/// is not stacked here.
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
        // DD#86: `var name` declares a string, meaningful-zero "" (parallel to
        // `int`). Enables declaring a string global at top level without `=""`.
        "str" => (DeclKeyword::Declare, VarType::String, vec![]),
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
) -> Result<Vec<DeclAssignment>, Diagnostic> {
    let mut assignments = Vec::new();

    loop {
        if scanner.at_eof()
            || matches!(
                scanner.peek().token,
                Tok::Newline
                    | Tok::Semi
                    | Tok::Eof
                    // A brace block's closer terminates the statement exactly as a
                    // newline or `;` does, so the declaration list must stop there.
                    // Without this, `fn f { str x="a" }` consumed the `}` as a
                    // candidate name and reported SB4014 "expected variable name",
                    // pointing at a declaration that was already complete. It hit
                    // every brace form -- fn, for, if, while, and DD#56 END -- and
                    // only when the declaration was the LAST statement in the block,
                    // since otherwise a `;` stopped the loop first.
                    | Tok::RBrace
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

/// Parse one `name`, `name=value`, or `name=(array)` within a declaration
/// list. A scalar value uses `parse_assignment_value` (DD#71 value-word
/// absorb); `=(…)` stays an array literal.
///
/// `name+=value` is rejected (SB4065): a declaration creates a binding, so
/// there is nothing to append to. Before 2026-09-22 it parsed, and for a
/// collection it fed a *scalar* into an array slot — `array a+=(x)` became
/// the string `"(x)"` and segfaulted on the first read.
fn parse_one_decl_assignment(
    scanner: &mut Scanner,
    span: Span,
) -> Result<DeclAssignment, Diagnostic> {
    let name = expect_word(scanner, span, "expected variable name in declaration")?;

    if matches!(scanner.peek().token, Tok::Eq) {
        scanner.next(); // consume =
        if matches!(scanner.peek().token, Tok::LParen) {
            scanner.next();
            let arr = parse_array_literal(scanner, span)?;
            return Ok(DeclAssignment {
                name,
                value: Some(arr),
            });
        }
        let value = parse_assignment_value(scanner)?;
        return Ok(DeclAssignment {
            name,
            value: Some(value),
        });
    }

    if matches!(scanner.peek().token, Tok::PlusAssign) {
        return Err(parse_error(
            span,
            "SB4065",
            format!(
                "cannot append with `+=` in a declaration: `{name}` is created here, \
                 so there is no previous value. Declare it first, then append."
            ),
        ));
    }

    // Bare declaration: `int x` — no initializer at all.
    Ok(DeclAssignment { name, value: None })
}

// ── END block ────────────────────────────────────────────────

/// Parse the body of an `END { }` block. The caller is [`parse`], which owns
/// the top-level loop; an `END` reached through [`dispatch_statement`] is by
/// construction *not* at the top level and is rejected there.
fn parse_end_block(scanner: &mut Scanner) -> Result<Vec<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'END'

    skip_terminators(scanner);
    expect(scanner, &Tok::LBrace, span, "expected '{' after END")?;
    let body = parse_body_until(scanner, &[Tok::RBrace])?;
    expect(scanner, &Tok::RBrace, span, "expected '}'")?;
    Ok(body)
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
fn parse_return_producer_head(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    match scanner.peek().token {
        Tok::DblBracketOpen => parse_test_command(scanner),
        Tok::DblParen => parse_arith_stmt(scanner),
        // DD#70/#74: `return ! X` — a negated producer. `parse_negated` folds
        // the `!` onto the terminal unit and rejects a non-terminal `!`
        // (`return ! X | Y` → SB4040), instead of the bare-`!` SB4014.
        Tok::Bang => parse_negated(scanner),
        _ => parse_simple_or_assignment(scanner),
    }
}

/// SB4014 — `return` was given something that is neither a status *producer*
/// (DD#74) nor a status *value*. Shared by the guard in `parse_return` and the
/// `None` arm below so the sentence exists once.
fn return_producer_error(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4014",
        "return: expected a status producer (a command, `[[ … ]]`, or `(( … ))`)".to_string(),
    )
}

/// The non-producer path: `return`, `return 7`, `return $?`, `return "5"` —
/// a status *value*, parsed as a plain `return` command.
///
/// A reserved word, a bare `-`, or `=` is neither a producer nor a value, and
/// this is where they used to be absorbed as an *argument* and dropped:
/// `return if` compiled and yielded 0 where bash reports "numeric argument
/// required" (rc 2), and `return - echo hi` additionally threw the command
/// away without running it. `keyword_to_str` is the canonical reserved-word
/// list (parse/expr.rs), so a keyword added later is covered here without a
/// second list to keep in step.
fn parse_return_value_form(
    scanner: &mut Scanner,
    after: &Tok,
) -> Result<Option<Statement>, Diagnostic> {
    if super::expr::keyword_to_str(after).is_some() || matches!(after, Tok::Dash | Tok::Eq) {
        let bad = scanner.peek_n(1).span;
        return Err(return_producer_error(span_from(scanner, bad.start)));
    }
    parse_simple_or_assignment(scanner)
}

fn parse_return(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let after = scanner.peek_n(1).token.clone();
    let is_producer = matches!(after, Tok::DblBracketOpen | Tok::DblParen | Tok::Bang)
        || matches!(&after, Tok::Word(w) if !w.chars().all(|c| c.is_ascii_digit()));
    if !is_producer {
        return parse_return_value_form(scanner, &after);
    }
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'return'
    let producer = parse_return_producer_head(scanner)?;
    match producer {
        Some(p) => {
            // DD#74: `return` takes a status *producer*, and a producer can be a
            // composite — `return A | B`, `return A && B`, `return A > f`. The
            // per-kind parse above yields only the first command / `[[ ]]` /
            // `(( ))`; fold any trailing pipeline / `&&`/`||` list / redirect
            // into the producer (the same `maybe_wrap_redirects` a normal
            // statement gets) so the whole composite is consumed by the return,
            // not left as an uncaught pipeline stage after a stage-0 `return`.
            let folded = maybe_wrap_redirects(scanner, p)?;
            Ok(Some(Statement {
                kind: StmtKind::ReturnProducer(Box::new(folded)),
                span,
            }))
        }
        None => Err(return_producer_error(span)),
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
    // DD#70: `! - cmd` stays rejected, and NOT as a must-consume rule — `-`
    // has already discarded the result, so `!` has nothing left to invert.
    // That is a composition error, so the addendum that made `- !` legal
    // (`-` discards the negated boolean) does not reach it.
    if matches!(&stmt.kind, StmtKind::Suppressed(_)) {
        return Err(parse_error(
            span,
            "SB4019",
            "`!` cannot negate a `-`-suppressed command — `-` and `!` don't \
             combine on one stage (suppress discards the status; `!` inverts it)"
                .to_string(),
        ));
    }
    // Reviewability (SB4040): `!` on a pipeline (`! a | b`) is rejected. The
    // operand was parsed by `dispatch_statement`, which never folds `|`, so an
    // already-multi-stage pipeline cannot arrive here — the rest of the
    // pipeline is always still ahead as a trailing `|`. Escape hatch: wrap the
    // pipeline in a function and negate the function.
    if matches!(scanner.peek().token, Tok::Pipe) {
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

/// SB4040: `!` may only negate the **terminal** segment of a pipeline (DD#70).
/// A `!` on a non-terminal stage (`! a | b`, `a | ! b | c`) is rejected — its
/// status is plumbing, and negating it then folding it into the pipefail
/// aggregate is illegible. `a | ! b` (negating the final stage, which supplies
/// the pipeline's result) is allowed. Escape hatch: wrap in a predicate.
fn negate_pipeline_error(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4040",
        "`!` may only negate the final stage of a pipeline (`a | ! b`) — a `!` \
         on an earlier stage negates a plumbing status a reviewer can't trace. \
         Wrap the pipeline in a predicate and negate that: \
         `f() { if a | b; then return 0; fi; return 1 }` then `if ! f; then …`"
            .to_string(),
    )
}

// ── suppressed command (- cmd) ───────────────────────────────

fn parse_suppressed(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);

    // Peek ahead: if what follows looks like a command, treat - as suppression.
    // DD#70 addendum: `!` is word-like enough to follow — `- ! cmd` discards
    // the negated boolean.
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
                | Tok::Bang
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
        // DD#70 addendum: `- ! X` discards the NEGATION's result, so the
        // suppression must stay outside. Pushing it into the first segment
        // would leave the `!` unsuppressed and still must-consume. (A negated
        // `Pipeline` here is always a single command — `! a | b` is SB4040.)
        StmtKind::Pipeline(ref p) if p.negated => Statement {
            kind: StmtKind::Suppressed(Box::new(Statement {
                kind: stmt.kind,
                span: stmt_span,
            })),
            span,
        },
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
/// DD#94: inside `[[ … ]]` a newline is trivia, not a terminator — the
/// construct is delimited by `]]`, so an arm list or a spanned test can be laid
/// out over several lines. Called at the parser's structural points rather than
/// in the scanner, because an arm BODY is an ordinary statement list where
/// newlines still separate statements.
fn skip_test_newlines(scanner: &mut Scanner) {
    while matches!(scanner.peek().token, Tok::Newline) {
        scanner.next();
    }
}

/// The remaining arms of a dispatch, after the first. Stops at `]]`, or at the
/// `|| { … }` default arm (which must be last). DD#94: newlines between arms
/// are trivia.
fn parse_dispatch_arms(
    scanner: &mut Scanner,
    span: Span,
    op: &str,
    items: &mut Vec<CaseItem>,
) -> Result<(), Diagnostic> {
    loop {
        skip_test_newlines(scanner);
        match scanner.peek().token {
            Tok::OrOr => {
                // `|| { group }` — the default arm (must be last). No synthetic
                // `*` pattern: DD#71 makes `*` an ordinary literal, so the
                // default is flagged instead.
                scanner.next();
                skip_test_newlines(scanner);
                items.push(CaseItem {
                    patterns: vec![],
                    body: parse_brace_block(scanner, span)?,
                    is_default: true,
                });
                return Ok(());
            }
            Tok::DblBracketClose => return Ok(()),
            _ => {
                let first = parse_operand_word(scanner)?;
                let pats = collect_arm_patterns(scanner, vec![first], op)?;
                skip_test_newlines(scanner);
                items.push(CaseItem {
                    patterns: pats,
                    body: parse_brace_block(scanner, span)?,
                    is_default: false,
                });
            }
        }
    }
}

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
    skip_test_newlines(scanner);
    let mut items = vec![CaseItem {
        patterns: first_pats,
        body: parse_brace_block(scanner, span)?,
        is_default: false,
    }];

    parse_dispatch_arms(scanner, span, &op, &mut items)?;

    expect(scanner, &Tok::DblBracketClose, span, "expected ']]'")?;

    Ok(Some(Statement {
        kind: StmtKind::Case(Case { value, items, op }),
        span,
    }))
}

/// Parse a test expression inside [[ ]].
fn parse_test_expr(scanner: &mut Scanner, span: Span) -> Result<TestExpr, Diagnostic> {
    skip_test_newlines(scanner);
    parse_test_or(scanner, span)
}

fn parse_test_or(scanner: &mut Scanner, span: Span) -> Result<TestExpr, Diagnostic> {
    let (mut left, mut any_and) = parse_test_and_tracked(scanner, span)?;
    skip_test_newlines(scanner);
    let mut any_or = false;
    while matches!(scanner.peek().token, Tok::OrOr) {
        any_or = true;
        scanner.next();
        skip_test_newlines(scanner);
        let (right, right_and) = parse_test_and_tracked(scanner, span)?;
        any_and |= right_and;
        skip_test_newlines(scanner);
        left = TestExpr::Or(Box::new(left), Box::new(right));
    }
    // DD#95: `&&` and `||` in one test group differently inside and outside
    // `[[ ]]`, so sash refuses to choose — say which you meant with `( )`.
    // A parenthesised operand is a *primary* here, so its own `&&`/`||` are
    // checked independently and never count as mixing at this level.
    if any_or && any_and {
        return Err(parse_error(
            span,
            "SB4053",
            "`&&` and `||` in one `[[ ]]` test are ambiguous — sash does not pick \
             a grouping. Use parentheses to say which you mean: \
             `[[ ( a || b ) && c ]]` or `[[ a || ( b && c ) ]]`"
                .to_string(),
        ));
    }
    Ok(left)
}

/// A `&&` chain, plus whether it actually combined one at this level (DD#95:
/// `false` for a lone primary, including a parenthesised group).
fn parse_test_and_tracked(
    scanner: &mut Scanner,
    span: Span,
) -> Result<(TestExpr, bool), Diagnostic> {
    let mut left = parse_test_not(scanner, span)?;
    skip_test_newlines(scanner);
    let mut had_and = false;
    while matches!(scanner.peek().token, Tok::AndAnd) {
        had_and = true;
        scanner.next();
        skip_test_newlines(scanner);
        let right = parse_test_not(scanner, span)?;
        skip_test_newlines(scanner);
        left = TestExpr::And(Box::new(left), Box::new(right));
    }
    Ok((left, had_and))
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
        // Either a `-X` file/emptiness test, or a DD#93 content-shape test
        // (`i?`/`p?`/`g?`) — those carry a `?` suffix instead of a `-` prefix,
        // so the shape check has to admit both. `len() >= 2` keeps the bare
        // `?` (implicit `-n`) out of this path.
        let looks_like_op = w.len() >= 2 && (w.starts_with('-') || w.ends_with('?'));
        if looks_like_op && is_unary_test_op(w) {
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

/// The outcome of looking for a glued (`$x==a`) operator in a test operand.
pub(crate) enum CompactOp {
    /// A glued operator on a `$`-initial word: a real comparison.
    Found(Expr, String, Expr),
    /// A glued operator on a word that starts with static text (`ab==$x`).
    /// DD#118 reserves the glued spelling for the `$`-initial template, so
    /// this is an error rather than a different answer than bash gives.
    LiteralInitial(String),
    /// No glued operator here.
    None,
}

/// Try to extract a compact `==` or `=~` operator from an Expr. The `[[ ]]`
/// lexer always splits `==`/`=~` into its own token, so a compact dispatch
/// subject (`$x==a`) always arrives as a `Concat` whose operator is a
/// *middle* part (`Concat([$x, "==", a])`) — never a single glued
/// `Literal("a==b")` and never an operator glued to the subject/pattern. So
/// only the `Concat` middle-part case is reachable; every other shape is a
/// non-compact operand (`_ => None`).
fn extract_compact_operator(expr: &Expr) -> CompactOp {
    match &expr.kind {
        ExprKind::Concat(parts) => extract_from_concat(parts, expr.span),
        _ => CompactOp::None,
    }
}

/// Extract a compact `==`/`=~` operator that appears as a *middle* part of a
/// `Concat` (`Concat([subject, "==", pattern])`) — the only shape the lexer can
/// produce for a compact dispatch operator (it splits `==`/`=~` into their own
/// token). A subject/pattern spanning several parts is rejoined via
/// `build_expr_from_parts`.
///
/// DD#118: the glued spelling is recognised only when the word **starts with
/// an expansion**. A word beginning with static text reads as data, and
/// accepting a glued operator there is what let `[[ ab==$x ]]` answer
/// differently from bash without saying so.
fn extract_from_concat(parts: &[Expr], span: Span) -> CompactOp {
    let found = (1..parts.len().saturating_sub(1)).find_map(|i| {
        let ExprKind::Literal(s) = &parts[i].kind else {
            return None;
        };
        let op = match_operator(s)?;
        Some((i, op))
    });
    let Some((i, op)) = found else {
        return CompactOp::None;
    };
    if !starts_with_expansion(parts) {
        return CompactOp::LiteralInitial(op);
    }
    let left = build_expr_from_parts(&parts[..i], span);
    let right = build_expr_from_parts(&parts[i + 1..], span);
    CompactOp::Found(left, op, right)
}

/// Whether the word carries the `$` sigil that marks the glued template.
/// `Literal`, `SingleQuoted` and `DoubleQuoted` are the static-text kinds;
/// everything else is an expansion and spells a leading `$`.
fn starts_with_expansion(parts: &[Expr]) -> bool {
    !matches!(
        parts[0].kind,
        ExprKind::Literal(_) | ExprKind::SingleQuoted(_) | ExprKind::DoubleQuoted(_)
    )
}

/// The fix-it text for a glued operator on a static-initial word. `==`
/// commutes, so swapping the operands is a real second option; `=~` does not
/// — its subject must stay on the left — so it is only offered the spacing.
fn glued_operator_hint(op: &str) -> String {
    if op == "==" {
        "a glued `==` needs the left operand to start with `$`; either space the \
         operator (`WORD == $var`) or swap the operands (`$var==WORD`)"
            .to_string()
    } else {
        "a glued `=~` needs the left operand to start with `$`; space the operator \
         (`WORD =~ $var`). The operands cannot be swapped — `=~` takes the subject \
         on the left."
            .to_string()
    }
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

/// Rejoin one side of a compact operator from a slice of `Concat` parts. A
/// single part is returned as-is; several parts (`$x$y==ab`, `ab==a$x`) are
/// re-wrapped in a `Concat`.
///
/// The empty slice cannot occur: both call sites slice a `Concat` around a
/// *middle* index `i` drawn from `1..len-1`, so `parts[..i]` holds at least
/// `parts[0]` and `parts[i+1..]` at least `parts[len-1]`. Returning
/// `Literal("")` here would make an empty subject or pattern out of a program
/// that has neither — a plausible wrong answer rather than a loud one.
fn build_expr_from_parts(parts: &[Expr], span: Span) -> Expr {
    match parts {
        [] => unreachable!("compact operator is a middle part, so neither side is empty"),
        [single] => single.clone(),
        _ => Expr {
            kind: ExprKind::Concat(parts.to_vec()),
            span,
        },
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
            let at = scanner.peek().span.start;
            Err(parse_error(
                span_from(scanner, at),
                "SB4067",
                "`=` is not a comparison operator; write `==` for string equality \
                 (a single `=` always assigns)"
                    .to_string(),
            ))
        }
        Tok::Bang => {
            let at = scanner.peek().span.start;
            Err(parse_error(
                span_from(scanner, at),
                "SB4067",
                "`!` is not valid after a test operand; write `!=` for string \
                 inequality, or `!` before the whole test to negate it"
                    .to_string(),
            ))
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
            match extract_compact_operator(&left) {
                CompactOp::Found(left_expr, op, right_expr) => {
                    return Ok(TestExpr::Binary {
                        left: Box::new(left_expr),
                        op,
                        right: Box::new(right_expr),
                    });
                }
                CompactOp::LiteralInitial(op) => {
                    return Err(parse_error(left.span, "SB4069", glued_operator_hint(&op)));
                }
                CompactOp::None => {}
            }
            // A word with no operator after it is a "bare word test" only when
            // the test is otherwise complete. If some other token interrupted
            // it, that token is the real fault and "expected `]]`" is the
            // better diagnosis (e.g. `[[ a&b == x ]]`, where `&` ends the word).
            if !at_test_end(scanner) {
                return Err(parse_error(span, "SB4014", "expected ']]'".to_string()));
            }
            Err(parse_error(
                left.span,
                "SB4068",
                "a bare word is not a test; write `-n WORD` to test that it is \
                 non-empty, or `-z WORD` that it is empty"
                    .to_string(),
            ))
        }
    }
}

/// The tokens that can legally follow a complete test operand.
fn at_test_end(scanner: &mut Scanner) -> bool {
    matches!(
        scanner.peek().token,
        Tok::DblBracketClose
            | Tok::AndAnd
            | Tok::OrOr
            | Tok::RParen
            | Tok::LBrace
            | Tok::Semi
            | Tok::Newline
    )
}

fn parse_binary_rhs(
    left: Expr,
    op: String,
    scanner: &mut Scanner,
    span: Span,
) -> Result<TestExpr, Diagnostic> {
    // DD#94: the operator has been consumed and an operand is owed, so a
    // newline here continues the test rather than ending it — the same
    // pending-operator rule the arithmetic separators use.
    skip_test_newlines(scanner);
    let right = if op == "=~" {
        // DD#71: the `=~` RHS is an operand word (metachars literal, quote/
        // $-aware, absorbed to the next whitespace/`]]`). No bespoke regex
        // collector.
        let word = parse_operand_word(scanner)?;
        // An unquoted pattern that contains `((` ends the word there, because
        // the lexer has already taken the two parens as the arithmetic
        // opener. Left alone this surfaces as "expected `]]`" from
        // `parse_test_command`, which names a delimiter the input does not
        // lack — say what actually happened instead.
        if matches!(scanner.peek().token, Tok::DblParen) {
            return Err(parse_error(
                span,
                "SB4014",
                "`((` in an unquoted regex is read as the arithmetic opener \
                 `$(( … ))`, so the pattern ends before it — quote the pattern \
                 (`=~ '^((a))$'`) or separate the parens (`^(a(b))$`)"
                    .to_string(),
            ));
        }
        word
    } else {
        parse_test_operand(scanner, span)?
    };
    Ok(TestExpr::Binary {
        left: Box::new(left),
        op,
        right: Box::new(right),
    })
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
            // DD#93 Part C: content-shape tests. The `?` suffix keeps them out
            // of bash's saturated `-X` namespace and reserves room for the
            // remaining POSIX classes (`a?`, `d?`, `l?`, `s?`, …) later.
            | "i?" // strict, i64-representable integer (renamed from `-i`)
            | "p?" // non-empty, all bytes ASCII printable (0x20–0x7E)
            | "g?" // non-empty, all bytes ASCII graph (0x21–0x7E)
            | "-b"
            | "-c"
            | "-p"
            // stat-mode bits. `file_test_op_code` has implemented these as
            // codes 10/12/13 all along; they were simply never listed here,
            // so `[[ -k f ]]` parsed as two words and reported SB4014.
            // (`-g` is the mode bit; the content-shape test is `g?` above.)
            | "-k"
            | "-g"
            | "-u"
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

// ── attest ────────────────────────────────────────────────────

fn parse_attest(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'attest'

    // First identifier: a scalar variable, or the first attest-all binder.
    let first = expect_word(scanner, span, "expected variable name after 'attest'")?;

    let lit = |s: String| Expr {
        kind: ExprKind::Literal(s),
        span,
    };

    // Scalar form `attest VAR [[…]]` is just the var; attest-all forms
    // (`attest BINDER… in ARRAY [[…]]`) collect binders then the array.
    // A bare unary operator (the DD#93 Part B shorthand, `attest VAR OP`)
    // also ends the lead — without this the operator would be swallowed as
    // another binder and fail looking for `in`. So does `((` (DD#93 Part D,
    // `attest SOURCE (( TARGET = SOURCE ))`), which is scalar-only.
    let mut args: Vec<Expr> = if matches!(scanner.peek().token, Tok::DblBracketOpen | Tok::DblParen)
        || peek_unary_test_op(scanner).is_some()
    {
        vec![lit(first)]
    } else {
        parse_attest_all_lead(scanner, first, span)?
    };

    // Expect [[ test_expr ]]
    if matches!(scanner.peek().token, Tok::DblBracketOpen) {
        scanner.next();
        let test_expr = parse_test_expr(scanner, span)?;
        expect(
            scanner,
            &Tok::DblBracketClose,
            span,
            "expected ']]' in attest",
        )?;
        args.push(Expr {
            kind: ExprKind::Test(test_expr),
            span,
        });
    } else if matches!(scanner.peek().token, Tok::DblParen) {
        // DD#93 Part D: the int-assignment is the predicate. Parsed as an
        // attest predicate, never as a free arithmetic statement — which is
        // why the bare source needs no DD#77 disposition here.
        scanner.next();
        let arith = parse_arith_command(scanner, span)?;
        args.push(Expr {
            kind: ExprKind::Arithmetic(Box::new(arith)),
            span,
        });
    } else if let Some(op) = peek_unary_test_op(scanner) {
        // DD#93 Part B: `attest VAR OP` ≡ `attest VAR [[ OP $VAR ]]`.
        scanner.next();
        args.push(Expr {
            kind: ExprKind::Test(attest_shorthand_test(&op, &args, span)),
            span,
        });
    }

    Ok(Some(Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("attest".to_string()),
                span,
            },
            args,
        }),
        span,
    }))
}

/// A bare word that is a unary test operator (`-n`, `-f`, `i?`, …). Same
/// shape check as `try_parse_leading_unary`: `-X`, or a DD#93 `?`-suffixed
/// content-shape test, with `len() >= 2` excluding the bare `?`.
fn peek_unary_test_op(scanner: &mut Scanner) -> Option<String> {
    let Tok::Word(ref w) = scanner.peek().token else {
        return None;
    };
    let looks_like_op = w.len() >= 2 && (w.starts_with('-') || w.ends_with('?'));
    (looks_like_op && is_unary_test_op(w)).then(|| w.clone())
}

/// DD#93 Part B: build the predicate the shorthand stands for, applying `op`
/// to the name(s) the attest binds. The operator binds them by construction,
/// so a wrong-var predicate (SB5020) cannot be written in this form.
///
/// `lead` is the args collected so far — the same shapes `classify_attest`
/// decodes: `[var]`, `[binder, array]`, `[key, val, map]`.
fn attest_shorthand_test(op: &str, lead: &[Expr], span: Span) -> TestExpr {
    let unary = |name: &str| TestExpr::Unary {
        op: op.to_string(),
        operand: Box::new(Expr {
            kind: ExprKind::SimpleExpansion(name.to_string()),
            span,
        }),
    };
    let name_at = |i: usize| match lead.get(i).map(|e| &e.kind) {
        Some(ExprKind::Literal(s)) => s.clone(),
        // Unreachable via parse_attest (the lead is always literals); a
        // wrong name here would be caught by taint_attest's SB5020.
        _ => String::new(),
    };
    match lead.len() {
        // `attest k v in m OP` — the map form's predicate must validate BOTH
        // binders (SB5020), so the operator applies to each (DG 2026-09-07).
        3 => TestExpr::And(Box::new(unary(&name_at(0))), Box::new(unary(&name_at(1)))),
        // `attest x OP` (scalar) and `attest e in a OP` (attest-all) both
        // bind the first name — the variable, or the element binder.
        _ => unary(&name_at(0)),
    }
}

/// DD#61 attest-all lead: `BINDER… in ARRAY`. Returns the binder literals
/// followed by the array literal (the trailing `[[ test ]]` is parsed by
/// the caller).
fn parse_attest_all_lead(
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
                "expected 'in' in attest-all (attest x in a [[ … ]])".to_string(),
            ));
        };
        binders.push(expect_word(
            scanner,
            span,
            "expected binder name in attest",
        )?);
    }
    scanner.next(); // consume 'in'
    let array = expect_word(scanner, span, "expected array name after 'in' in attest")?;
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

    if looks_like_assignment(scanner) {
        return parse_assignment(scanner, span);
    }
    if let Some(name) = looks_like_regex_mutation(scanner) {
        return parse_regex_mutation(scanner, span, &name);
    }
    parse_command(scanner, span)
}

/// `name = value` / `name += value` / `name[idx] = value` shape probe.
/// Doesn't consume any tokens.
fn looks_like_assignment(scanner: &mut Scanner) -> bool {
    let head = scanner.peek();
    let Tok::Word(w) = &head.token else {
        return false;
    };
    let w = w.clone();
    let w_end = head.span.end;
    let next = scanner.peek_n(1);
    if matches!(next.token, Tok::Eq | Tok::PlusAssign) && is_identifier_or_subscript(&w) {
        let eq = next.span;
        if eq.start == w_end {
            return true;
        }
        return scanner.peek_n(2).span.start != eq.end;
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
    // The index is no longer a single token (it may be `$(( … ))`, a
    // disposition, or a quoted key), so look ahead for the `]` that closes it
    // and require an assignment operator immediately after. The `name[` word is
    // still unconsumed here, so the index starts one token later than it does
    // in `parse_split_subscript_index`.
    (1..=MAX_LHS_INDEX_TOKENS).any(|i| {
        matches!(&scanner.peek_n(i).token, Tok::Word(rb) if rb == "]")
            && matches!(scanner.peek_n(i + 1).token, Tok::Eq | Tok::PlusAssign)
    })
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

/// Consume a dynamic subscript index that the lexer split into its own tokens,
/// followed by the closing `]` word. Used for `name[IDX]=value` assignments
/// (DD#33).
///
/// This used to accept only `$var`/`${var}`, which made the assignment LHS a
/// strictly narrower surface than the read side: `${a[$(( … ))]}` read fine but
/// `a[$(( … ))]=` was SB4014, even though `$(( … ))` is the conversion DD#80
/// itself names. The two surfaces must accept the same index forms, so the raw
/// index text is recovered from the token spans and handed to the *same*
/// [`parse_subscript_index`] the read path uses — uniformity is then structural
/// rather than two lists kept in step by hand.
///
/// Re-parsing the text, rather than running `parse_expr` on the live scanner,
/// is what keeps the closing `]` out of the index: with no space before it the
/// word parser glues it onto the last operand, so `a[$i]=` yielded `1]`.
fn parse_split_subscript_index(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    let Some(close) = lhs_index_close(scanner) else {
        return Err(parse_error(
            span,
            "SB4014",
            "unsupported dynamic subscript index in assignment".to_string(),
        ));
    };
    let start = scanner.peek().span.start;
    let end = scanner.peek_n(close).span.start;
    let text = scanner.source_slice(start, end).to_string();
    for _ in 0..=close {
        scanner.next();
    }
    Ok(parse_subscript_index(&text, span))
}

/// Offset of the `]` token that closes an assignment-LHS subscript, or `None`
/// if there is none within [`MAX_LHS_INDEX_TOKENS`].
fn lhs_index_close(scanner: &mut Scanner) -> Option<usize> {
    (0..MAX_LHS_INDEX_TOKENS)
        .find(|&i| matches!(&scanner.peek_n(i).token, Tok::Word(w) if w == "]"))
}

/// How far the assignment-LHS subscript parser looks ahead for the `]` that
/// closes the index. Bounded so a bare `name[` cannot walk the rest of the file.
const MAX_LHS_INDEX_TOKENS: usize = 32;

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
    let raw = scanner
        .take_sed_stmt_payload()
        .ok_or_else(|| sed_malformed_err(span))?;
    let body = strip_sed_prefix(&raw);
    validate_static_sed_re(body, span)?;
    let subject = quoted_var_subject(var_name, span);
    let value =
        super::expr::build_sed_sub(body, subject, span)?.ok_or_else(|| sed_malformed_err(span))?;
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

fn sed_malformed_err(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4032",
        "malformed `=~ s/…` substitution — expected s/RE/SUB/[flags] with three `/` delimiters"
            .to_string(),
    )
}

/// Strip the `s/` the *scanner* has already required. `take_sed_stmt_payload`
/// returns `None` unless the payload starts with `s/` — `scan_one_sed_edit`
/// compares those two bytes literally before scanning anything else — so every
/// malformed `=~` payload is rejected there, with SB4032, before the parser
/// sees it. A payload arriving here without the prefix means the scanner and
/// the parser have drifted apart, which is a compiler bug and not a program
/// one. Unreachable by construction, so it is a loud ICE rather than a
/// user-facing diagnostic (DG, 2026-09-19).
pub(crate) fn strip_sed_prefix(raw: &str) -> &str {
    raw.strip_prefix("s/").unwrap_or_else(|| {
        panic!("ICE: sed payload reached the parser without the scanner-required `s/`: {raw:?}")
    })
}

/// Preserve the DD#39 compile-time regex check on the `=~` mutation form: a
/// STATIC (non-interpolating) pattern is validated now; an interpolating one
/// (`$x` in the pattern) is a runtime regcomp check.
///
/// The `None` arm is the same should-never-run ICE as [`strip_sed_prefix`]:
/// `scan_one_sed_edit` scans the two remaining `/` delimiters before the
/// payload is handed over, so `split_sed_fields` cannot fail on a payload that
/// got this far.
pub(crate) fn validate_static_sed_re(body: &str, span: Span) -> Result<(), Diagnostic> {
    let Some((re, _, _)) = super::expr::split_sed_fields(body) else {
        panic!("ICE: sed payload reached the parser without the scanner-required `/` delimiters: {body:?}")
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
    let inner = super::expr::magic_sugar_expansion(var_name, span).unwrap_or_else(|| Expr {
        kind: ExprKind::SimpleExpansion(var_name.to_string()),
        span,
    });
    Expr {
        kind: ExprKind::DoubleQuoted(vec![inner]),
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

/// Does a file-descriptor word bind to the redirect operator after it?
///
/// bash binds the prefix only when the digits are **adjacent** to the
/// operator. `2>f` redirects fd 2; `2 >f` passes `2` as an argument and
/// redirects stdout. Testing only the operator's token kind — which both
/// call sites did — ate the argument: `echo hi 2 >f` wrote `hi` to stdout
/// and left the file empty, where bash writes `hi 2` to the file. The
/// adjacency test is the same one `looks_like_assignment` uses to tell
/// `x=1` from `x =1`.
fn fd_prefix_binds(scanner: &mut Scanner) -> bool {
    let fd_end = scanner.peek().span.end;
    let next = scanner.peek_n(1);
    next.span.start == fd_end
        && matches!(
            next.token,
            Tok::Gt | Tok::DblGt | Tok::GtAmp | Tok::LtAmp | Tok::Lt
        )
}

/// Consume an fd-tagged redirect whose fd digits are the current token.
/// Only valid when `fd_prefix_binds` just returned true.
fn parse_fd_redirect(scanner: &mut Scanner, w: &str, span: Span) -> Result<Redirect, Diagnostic> {
    // `is_fd_number` guarantees 1-2 ASCII digits at both call sites, so this
    // parse cannot fail; a silent 0 would mean redirecting stdin by accident.
    let fd: u32 = w
        .parse()
        .expect("is_fd_number guarantees a 1-2 digit decimal");
    scanner.next(); // consume FD number
    let mut redir = parse_redirect(scanner, span)?;
    redir.fd = Some(fd);
    Ok(redir)
}

/// Handle a leading file-descriptor word (`2` in `2>err`): if an adjacent
/// redirect operator follows, parse an fd-tagged redirect; otherwise it is
/// a plain argument.
fn parse_fd_prefixed(
    scanner: &mut Scanner,
    w: &str,
    span: Span,
    args: &mut Vec<Expr>,
    redirects: &mut Vec<Redirect>,
) -> Result<(), Diagnostic> {
    if fd_prefix_binds(scanner) {
        redirects.push(parse_fd_redirect(scanner, w, span)?);
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
    while let Some(redir) = next_trailing_redirect(scanner, span)? {
        redirects.push(redir);
    }
    Ok(redirects)
}

/// A bare redirect operator, with no file-descriptor prefix.
fn is_redirect_op(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Gt
            | Tok::DblGt
            | Tok::Lt
            | Tok::GtAmp
            | Tok::LtAmp
            | Tok::LtGt
            | Tok::AmpGt
            | Tok::HereString
    )
}

/// One step of the trailing-redirect loop: the next redirect, or `None`
/// once the token stream has moved past them. A digit word that does *not*
/// bind (`2 >f`) ends the run rather than being consumed — it is an
/// argument, and belongs to whatever parses next.
fn next_trailing_redirect(
    scanner: &mut Scanner,
    span: Span,
) -> Result<Option<Redirect>, Diagnostic> {
    let tok = scanner.peek().token.clone();
    if is_redirect_op(&tok) {
        return parse_redirect(scanner, span).map(Some);
    }
    let Tok::Word(ref w) = tok else {
        return Ok(None);
    };
    if !is_fd_number(w) || !fd_prefix_binds(scanner) {
        return Ok(None);
    }
    parse_fd_redirect(scanner, w, span).map(Some)
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
    // One iteration by construction: `parse_statement` consumes the whole rest
    // of the pipeline and `append_pipeline_stage` merges it, so there is never
    // a second `|` left to see. It was a `while` only because the old code
    // could fall through without consuming anything — the path that dropped a
    // trailing `|` on the floor (SB4056).
    if matches!(scanner.peek().token, Tok::Pipe) {
        let pipe_span = scanner.peek().span;
        scanner.next();
        skip_terminators(scanner);
        // Same hole as the list operators: with no stage after the `|` the loop
        // simply fell out and the pipeline was returned as its left side alone,
        // so `echo a |` compiled and printed `a`. bash rejects it.
        let Some(next_stmt) = parse_statement(scanner)? else {
            return Err(parse_error(
                span_from(scanner, pipe_span.start),
                "SB4056",
                "`|` has no right-hand side — a pipeline needs a command on both sides".to_string(),
            ));
        };
        append_pipeline_stage(&mut commands, next_stmt);
    }
    // DD#55 / SB4023: `pipeline && C` / `pipeline || C` is rejected — see
    // reject_pipeline_and_or_postfix.
    reject_pipeline_and_or_postfix(&commands, span)?;
    if commands.len() > 1 {
        reject_statement_form_pipeline_stage(&commands)?;
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

/// Append a parsed `|`-stage, flattening a nested pipeline (right-associative
/// `a | b | c` → one flat stage list). A negated single-command stage
/// (`a | ! b`) arrives from `parse_negated` as `Pipeline{[b], negated:true}`;
/// preserve the negation as a per-segment `Negated(b)` marker instead of
/// flattening it away (DD#70 terminal-segment negation). A non-terminal `!`
/// (`a | ! b | c`) never reaches here — `parse_negated` rejects it (SB4040)
/// because the negated unit has a trailing `|`.
fn append_pipeline_stage(commands: &mut Vec<Statement>, next_stmt: Statement) {
    match next_stmt.kind {
        StmtKind::Pipeline(p) if p.negated && p.commands.len() == 1 => {
            let span = next_stmt.span;
            let inner = p.commands.into_iter().next().unwrap();
            commands.push(Statement {
                kind: StmtKind::Negated(Box::new(inner)),
                span,
            });
        }
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

/// Peel the wrappers a pipeline stage may carry (`- stmt`, `! stmt`,
/// `stmt > f`) to reach the statement that actually runs, so `- [[ … ]] | cat`
/// cannot smuggle a test past `reject_statement_form_pipeline_stage`.
fn unwrap_stage_wrappers(stmt: &Statement) -> &Statement {
    let mut cur = stmt;
    loop {
        match &cur.kind {
            StmtKind::Suppressed(inner) | StmtKind::Negated(inner) => cur = inner,
            StmtKind::Redirected(r) => cur = &r.body,
            _ => return cur,
        }
    }
}

/// Name the statement form a stage is, or `None` if it is an ordinary command.
/// `[[ … ]]` and `attest` parse to a `Command` whose *name* is the literal
/// `"[["` / `"attest"`, which is exactly why the pipeline path mistook them
/// for externals.
fn statement_form_label(stmt: &Statement) -> Option<&'static str> {
    match &stmt.kind {
        StmtKind::Command(c) => match &c.name.kind {
            ExprKind::Literal(n) if n == "[[" => Some("`[[ … ]]`"),
            ExprKind::Literal(n) if n == "attest" => Some("`attest`"),
            _ => None,
        },
        StmtKind::Assignment(_) | StmtKind::ArithTest(_) => Some("`(( … ))`"),
        _ => None,
    }
}

/// SB4022: `[[ … ]]`, `attest …` and `(( … ))` are statement forms, not
/// commands, so they may not be pipeline stages. A stage runs in a forked
/// child: a test's status is discarded and an arithmetic assignment is lost,
/// which made `(( n = 2 )) | cat` silently do nothing. Worse, `[[` and
/// `attest` reached codegen as an ordinary command *named* `[[` — the runtime
/// aborted with its own "bare command passed to sash_exec" assertion, i.e. a
/// clean compile that could only fail at run time.
///
/// Compound commands (`if`, `for`, `match`) are already rejected here by
/// `compound_postfix_rejection`; these three were the statement forms it did
/// not classify as compound. `&&`/`||` on a test stays legal — only the pipe
/// discards the result.
fn reject_statement_form_pipeline_stage(commands: &[Statement]) -> Result<(), Diagnostic> {
    for stmt in commands {
        let stage = unwrap_stage_wrappers(stmt);
        if let Some(what) = statement_form_label(stage) {
            return Err(parse_error(
                stage.span,
                "SB4022",
                format!(
                    "{what} isn't allowed as a pipeline stage — a stage runs in a forked \
                     child, so its result is discarded; wrap it in a function"
                ),
            ));
        }
    }
    Ok(())
}

/// SB4053 (DD#95): `&&` and `||` mixed in one command list. The advice differs
/// from the `[[ ]]` case: there is no grouping construct out here, and none is
/// wanted — a command list that needs a grouping is asking to be an `if` or a
/// dispatch.
fn mixed_list_error(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4053",
        "`&&` and `||` in one list are ambiguous — sash does not pick a grouping. \
         `a && b || c` could mean `(a && b) || c` or `a && (b || c)`. Use \
         `if a && b { … } else { … }`, or the `[[ … { … } || { … } ]]` dispatch \
         form"
            .to_string(),
    )
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
    //
    // DD#95: a chain must not MIX the two. Inside `[[ ]]` the same text groups
    // as `A || (B && C)` (`&&` binds tighter, as in C/Go/Rust/Python); out here
    // it is `(A || B) && C` (equal precedence, as in POSIX shell). Rather than
    // let one text mean two things, sash rejects the mixture — and out here
    // there is nothing to group with, because the answer is `if` or dispatch.
    let mut chain_op: Option<ListOp> = None;
    loop {
        let op = match scanner.peek().token {
            Tok::AndAnd => ListOp::And,
            Tok::OrOr => ListOp::Or,
            Tok::Amp => return Ok(background_list_op(scanner, left)),
            _ => return Ok(left),
        };
        match &chain_op {
            Some(prev) if *prev != op => return Err(mixed_list_error(left.span)),
            Some(_) => {}
            None => chain_op = Some(op.clone()),
        }
        let span = left.span;
        let op_start = scanner.peek().span.start;
        scanner.next(); // consume && / ||
        skip_terminators(scanner);
        // A missing right operand is a syntax error (SB4056), not an implicit
        // `true`. The old `.unwrap_or(true_cmd)` silently turned `echo a &&`
        // into `echo a && true`, which compiled and ran while bash rejects the
        // text outright. (`background_list_op` still synthesises a `true` — a
        // trailing `&` genuinely has no right operand.)
        let Some(right) = parse_list_operand(scanner)? else {
            return Err(parse_error(
                span_from(scanner, op_start),
                "SB4056",
                format!(
                    "`{}` has no right-hand side — a list operator needs a command \
                     on both sides",
                    match op {
                        ListOp::And => "&&",
                        ListOp::Or => "||",
                        ListOp::Background => "&",
                    }
                ),
            ));
        };
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
        let what = redirect_reject_noun(&stmt.kind);
        return Err(parse_error(
            stmt.span,
            "SB4046",
            format!(
                "a redirect cannot attach to {what} — redirects attach to a command that produces output"
            ),
        ));
    }
    parse_trailing_pipeline(scanner, stmt)
}

/// The SB4046 noun for a statement that has a trailing redirect still pending
/// here — or a loud ICE, because there is no such thing as a legal one.
///
/// This used to end in `wrap_with_redirects`, which built a `Redirected` node
/// when `redirects` was non-empty. Branch coverage showed that arm had never
/// run: of 18594 calls, 18574 arrived with no redirects at all and the other
/// 20 all returned SB4046. The reason is that a command absorbs its own
/// trailing redirects in `parse_simple_command` (325 times in the same sweep),
/// so the only kinds that can still be holding one here are precisely the
/// kinds `noncommand_redirect_target` names.
///
/// Deleting the dead arm silently would have been the wrong repair: if some
/// statement kind later gains the ability to carry a redirect past
/// `parse_simple_command`, the `None` case is reached and the redirect would
/// simply vanish from the AST — output going to a file the program never
/// opens, with no diagnostic. Panicking instead turns that regression into a
/// caught ICE. Exercised by `redirect_reject_noun_is_loud_ice`, since no
/// source text can reach it.
pub(super) fn redirect_reject_noun(kind: &StmtKind) -> &'static str {
    match noncommand_redirect_target(kind) {
        Some(what) => what,
        None => panic!(
            "ICE: a trailing redirect survived to wrap_pipeline_part on a \
             statement kind that should have absorbed it in parse_simple_command"
        ),
    }
}

/// A redirect may only attach to something that produces output. If `kind` is
/// a function definition, a variable declaration, a variable assignment, an
/// arithmetic test, or a `[[ … ]]` test, a trailing redirect is illegal
/// (SB4046) — returns the noun for the diagnostic; `None` means a redirect is
/// allowed on this statement kind. (Compound commands are handled separately
/// by `reject_compound_postfix`, SB4021.)
///
/// The two test forms are the later additions. Both produce no output, so
/// bash's behaviour — create and truncate the file, write nothing into it —
/// is a wart rather than a feature. sash parsed the redirect correctly and
/// then discarded it in the transform, which diverged from bash *silently*;
/// DG's call was that a meaningless redirect is an error, not a no-op.
fn noncommand_redirect_target(kind: &StmtKind) -> Option<&'static str> {
    match kind {
        StmtKind::Function(_) => Some("a function definition"),
        StmtKind::Declaration(_) => Some("a variable declaration"),
        StmtKind::Assignment(_) => Some("a variable assignment"),
        StmtKind::ArithTest(_) => Some("an arithmetic test"),
        StmtKind::Command(c) if is_bracket_test(c) => Some("a `[[ … ]]` test"),
        _ => None,
    }
}

/// The `[[ … ]]` form, which `parse_test_command` builds as a command whose
/// name is the literal `[[`.
fn is_bracket_test(c: &Command) -> bool {
    matches!(&c.name.kind, ExprKind::Literal(n) if n == "[[")
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

/// The plain `(fd, op)` for a redirect operator token. `&>` and `<<<` are
/// handled separately (they own their target parsing / are rejected), and any
/// other token returns `None` (→ SB4014). A pure dispatcher.
fn redirect_op_for_token(tok: &Tok) -> Option<(Option<u32>, RedirectOp)> {
    Some(match tok {
        Tok::Gt => (None, RedirectOp::Write),
        Tok::DblGt => (None, RedirectOp::Append),
        Tok::Lt => (None, RedirectOp::Read),
        Tok::GtAmp => (None, RedirectOp::DupOutput),
        Tok::LtAmp => (None, RedirectOp::DupInput),
        Tok::LtGt => (None, RedirectOp::ReadWrite),
        _ => return None,
    })
}

/// `&>file` → `>file 2>&1` (the `2>&1` half is handled at the IR level); the
/// target must be a scalar filename like any other write redirect.
fn parse_ampgt_redirect(scanner: &mut Scanner, span: Span) -> Result<Redirect, Diagnostic> {
    let target = parse_expr(scanner)?;
    reject_nonscalar_redirect_target(&target, span)?;
    Ok(Redirect {
        fd: Some(1),
        op: RedirectOp::Write,
        target,
    })
}

/// DD#83: sash has no here-strings. `consumer <<< X` is exactly
/// `producer | consumer` in sash (no subshell → the reader persists; taint is
/// provenance-based, not channel-based; sash doesn't word-split) — pure sugar
/// over a pipe, with no semantics a pipe lacks. Dropped rather than maintained
/// (also retires the silent-empty / missing-newline here-string bugs).
fn here_string_diag(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4050",
        "sash has no here-strings (`<<<`) — they are exactly a pipe in sash \
         (no subshell, provenance-based taint, no word-splitting). Feed the \
         data with `producer | consumer`: `cat <<< $x` → `echo $x | cat`; \
         `read v <<< $x` → `echo $x | read v`."
            .to_string(),
    )
}

fn parse_redirect(scanner: &mut Scanner, span: Span) -> Result<Redirect, Diagnostic> {
    let tok = scanner.next();
    // `&>file` and `<<<` have their own target handling; everything else maps
    // to a plain (fd, op) via redirect_op_for_token.
    match tok.token {
        Tok::AmpGt => return parse_ampgt_redirect(scanner, span),
        Tok::HereString => return Err(here_string_diag(span)),
        _ => {}
    }
    let (fd, op) = redirect_op_for_token(&tok.token)
        .ok_or_else(|| parse_error(span, "SB4014", "expected redirect operator".to_string()))?;
    let target = parse_expr(scanner)?;
    if matches!(op, RedirectOp::DupOutput | RedirectOp::DupInput) {
        reject_nonfd_dup_target(&target, op, span)?;
    } else {
        reject_nonscalar_redirect_target(&target, span)?;
    }
    Ok(Redirect { fd, op, target })
}

/// A `>&`/`<&` fd-dup target must be a plain non-negative fd number. Reject
/// anything else — a non-numeric target (`>&foo`), trailing garbage (`2>&1x`),
/// the bash close form (`>&-`), or a dynamic word (`>&$n`) — rather than the
/// old silent dup-to-fd-2 fallback in `emit_dup_output`
/// (bug-redirect-dup-target-unvalidated). DG (2026-08-29): reject-only for now;
/// exposing dup/close as builtins is a separate follow-up.
fn reject_nonfd_dup_target(target: &Expr, op: RedirectOp, span: Span) -> Result<(), Diagnostic> {
    let is_fd = matches!(&target.kind,
        ExprKind::Literal(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
    if is_fd {
        return Ok(());
    }
    let arrow = if op == RedirectOp::DupInput {
        "<&"
    } else {
        ">&"
    };
    Err(parse_error(
        span,
        "SB4048",
        format!(
            "`{arrow}` needs a file-descriptor number (e.g. `{arrow}2`) — sash \
             doesn't support `{arrow}` with a non-numeric target, trailing text, \
             or the `-` close form. Redirect to a file with `>`/`<`, dup a \
             numbered fd, or close one with `close N`."
        ),
    ))
}

/// A file-redirect target (`>` / `>>` / `<` / `<>` / `&>`) must be a *scalar*
/// read the compiler can evaluate into a filename: a literal, a quoted path,
/// `$x`, `${x:-default}`, `$a[i]`, or a concatenation of those. Reject a
/// command substitution `$(…)`, arithmetic `$((…))`, or any other non-scalar
/// form (array literal, `[[ … ]]`): the compiler can't know what filename a
/// command would produce, and the old fallthrough silently redirected to an
/// *empty* target (bug-redirect-nonsimple-target-empty). DG (2026-08-30):
/// support scalars, reject complex — assign the value to a variable first,
/// then redirect to `$v`. Here-strings (`<<<`) feed stdin *data*, not a
/// filename, and are intentionally not restricted here.
fn redirect_target_is_scalar(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_)
        | ExprKind::SimpleExpansion(_)
        | ExprKind::ParameterExpansion(_)
        | ExprKind::Subscript { .. } => true,
        ExprKind::Concat(parts) | ExprKind::DoubleQuoted(parts) => {
            parts.iter().all(|p| redirect_target_is_scalar(&p.kind))
        }
        _ => false,
    }
}

fn reject_nonscalar_redirect_target(target: &Expr, span: Span) -> Result<(), Diagnostic> {
    if redirect_target_is_scalar(&target.kind) {
        return Ok(());
    }
    Err(parse_error(
        span,
        "SB4049",
        "a redirect target must be a scalar filename — a literal/quoted path, \
         `$x`, `${x:-default}`, or `$a[i]`. sash doesn't support a command \
         substitution `$(…)` or arithmetic `$((…))` as a redirect target: the \
         compiler can't know the resulting filename. Assign it to a variable \
         first, then redirect to `$v`."
            .to_string(),
    ))
}

// ── Helpers ──────────────────────────────────────────────────

/// A word that is an identifier or a subscripted one: `name` or `name[idx]`.
///
/// This used to be a flat character-set test (alphanumeric plus `_ [ ]`), which
/// accepted `9x`, `123`, `][`, and `a]b[` as assignment targets — so `9x=1`
/// parsed as an assignment to a name bash will not accept. It now checks the
/// two parts separately: [`is_identifier`] for the name, and a single
/// well-formed `[…]` for the subscript.
fn is_identifier_or_subscript(s: &str) -> bool {
    let Some(open) = s.find('[') else {
        return is_identifier(s);
    };
    let Some(rest) = s.strip_suffix(']') else {
        return false;
    };
    let subscript = &rest[open + 1..];
    is_identifier(&s[..open]) && !subscript.is_empty() && !subscript.contains(['[', ']'])
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
        // Some keywords can also be used as identifiers in certain contexts.
        // The spelling comes from `keyword_to_str`, the single reserved-word
        // list — never from the variant's `Debug` name. Deriving it as
        // `format!("{:?}", tok).to_lowercase()` is a second, silent answer to
        // "how is this keyword spelled?" that happens to agree only because
        // every variant here is a single lowercase word; it would have
        // produced `dblbracketopen` for a variant whose name is not its text.
        Tok::In | Tok::Do | Tok::Done | Tok::Then | Tok::Fi | Tok::Else | Tok::Elif | Tok::Esac => {
            let name = keyword_to_str(&scanner.peek().token)
                .expect("every variant in this arm is in keyword_to_str's domain")
                .to_string();
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
                "SB5076",
                "sash has no backtick command substitution — use $(...)".to_string(),
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

/// DG (2026-08-29): trailing content the compiler doesn't recognise (`${x&y}`,
/// `${x:foo}`, …), carried as the raw unparsed remainder on a sentinel
/// `unrecognized` operator by `parse_parameter_expansion`. Reject rather than
/// silently drop it — the compiler must never emit code for input it doesn't
/// fully understand.
fn unrecognized_operator_diag(pe: &ParameterExpansion, span: Span) -> Option<Diagnostic> {
    let trailing = match pe.operand.as_deref() {
        Some(Expr {
            kind: ExprKind::Literal(s),
            ..
        }) => s.as_str(),
        _ => "",
    };
    Some(parse_error(
        span,
        "SB4047",
        format!(
            "unrecognized content `{trailing}` after `{}` in `${{…}}` — the \
             compiler does not recognise this parameter operator and will not \
             silently ignore it. sash's parameter operators are `:-` / `:?` / \
             `:off:len` / `:s/…/…/` (plus the string builtins toupper/tolower, \
             sub/gsub, basename/dirname)",
            pe.name
        ),
    ))
}

/// The single-operator rejects that are a plain equality (or the two-char
/// `@X` transform): indirect `${!x}` (SB4003), a substring/`:s/` on an array
/// cell (SB4039), and the `@`-transform family (SB4015). `None` if `op` is
/// none of these.
fn simple_rejected_operator_diag(
    op: &str,
    pe: &ParameterExpansion,
    span: Span,
) -> Option<Diagnostic> {
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
    None
}

/// DD#84: `:~` (elide) never takes a word — `${x:~foo}` is a syntax error
/// (SB4051). For a value/default, use `:-`. (`:!` is a modifier, not a
/// disposition, so a malformed `${x:!foo}` is caught by `flag_malformed_trust`
/// instead.)
fn elide_takes_no_word_diag(op: &str, pe: &ParameterExpansion, span: Span) -> Option<Diagnostic> {
    if op != ":~" || pe.operand.is_none() {
        return None;
    }
    Some(parse_error(
        span,
        "SB4051",
        format!(
            "`${{{}{}}}` takes no word — `{}` only removes the token when its \
             value is empty. For a default value use `${{{}:-WORD}}`.",
            pe.name, op, op, pe.name
        ),
    ))
}

/// The reject diagnostic for a rejected `${var:OP}` operator, or `None`. The
/// operator is a single value, so at most one applies: indirect `${!x}`
/// (SB4003), `@`-transforms (SB4015), and the DD#65 pruned modifier family
/// (SB4034).
fn rejected_param_operator_diag(pe: &ParameterExpansion, span: Span) -> Option<Diagnostic> {
    let op = pe.operator.as_deref()?;
    if op == "unrecognized" {
        return unrecognized_operator_diag(pe, span);
    }
    if let Some(d) = elide_takes_no_word_diag(op, pe, span) {
        return Some(d);
    }
    if let Some(d) = simple_rejected_operator_diag(op, pe, span) {
        return Some(d);
    }
    // DD#75 §4: the `[@]`/`[*]` bracket-subscript collection forms are retired
    // — `@a` is the sole splat. Reject each with its canonical replacement.
    if let Some(d) = retired_bracket_operator_diag(pe, op, span) {
        return Some(d);
    }
    // DD#65: the pruned `${var:OP}` family — every `split_operator` op EXCEPT
    // the kept `:-` / `:?` (default / require; `:?` re-added by DD#77). Includes
    // the non-colon forms `-` `+` `=` `?` (DG 2026-08-29: no compiler coercion —
    // one spelling, the colon forms; conversion `-`→`:-` is sash-convert's job).
    // Rejects uniformly; migration to the builtins is sash-convert's job (see
    // task `converter-var-modifier-migration`).
    if is_dropped_param_operator(op) {
        return Some(parse_error(
            span,
            "SB4034",
            format!(
                "the `{}` parameter operator (in ${{{}…}}) isn't part of sash — \
                 use the string builtins (toupper/tolower, sub/gsub, basename/dirname) \
                 or the :- / :? / :off:len / :s/ forms",
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
/// `split_operator` set minus the kept `:-` / `:?` (DD#77). Includes the
/// non-colon default family `-` `+` `=` `?` (DG 2026-08-29 — see
/// `rejected_param_operator_diag`): these are recognised by `split_operator` so
/// they reject explicitly here instead of silently dropping (`${x-4}` → `${x}`).
/// The `/`-family glob-replace is remapped to `glob_replace` at parse
/// (parse_standard_operator) so it is distinguishable here from the kept `:s/`
/// sed op, which shares the raw `/`//` operator strings. The per-element
/// **array** forms `${arr[@]OP}` (operator `arrvals_<op>`,
/// `parse_subscript_operator`) are dropped too, for consistency with the scalar
/// drop — the replacement is a `for x in @a` loop with `$(sub …)`/`$(gsub …)`.
/// Kept forms (`substring`/`:off:len`, `:s/`, `strlen`, plain
/// `arrvals`/`arrjoin` join, `glob`) arrive via other parse paths /
/// operators and are not matched here.
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
        ":=" | ":+"
            | "#"
            | "##"
            | "%"
            | "%%"
            | "glob_replace"
            | "^"
            | "^^"
            | ","
            | ",,"
            | "-"
            | "+"
            | "="
            | "?"
    )
}

/// Walk all expressions in a statement, calling f for each.
fn walk_exprs(stmt: &Statement, f: &mut impl FnMut(&Expr)) {
    crate::ir::walk_all_exprs(stmt, f);
}
