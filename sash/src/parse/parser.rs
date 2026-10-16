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
use super::lexer::{has_bare_spelling, Scanner, Tok};
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

    // Every diagnostic is a hard error (see `diagnostic::Diagnostic`), so any
    // at all ends the parse.
    if !diags.is_empty() {
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
/// (`magic_sugar_expansion` / `magic_sugar_intrinsic`), so one of those names
/// surviving downstream means a construction site was missed.
///
/// This exists because a missed site is otherwise *silent*: that one context
/// would keep a magic variable, every spec would still pass, and the checks
/// that look for the intrinsic form — SB5012's `$?` state machine above all —
/// would simply stop seeing reads there. An invariant worth stating is worth
/// failing on, so this panics rather than warns. It cannot fire on user input;
/// only on a compiler bug.
///
/// **Both node types are checked, and that is the point.** The guard once
/// walked `ExprKind` alone, so it proved the property for the word path while
/// the arithmetic lexer kept its own `$$`/`$?`/`$!` scan that produced
/// `ArithExpr::DollarVar` — a different type, structurally invisible here.
/// The miss was exactly the one the paragraph above predicts: `$(( $? ))` read
/// a status SB5012 never saw.
fn assert_no_magic_sugar(program: &Program) {
    let check = |stmt: &Statement| crate::ir::walk_all_exprs(stmt, &mut assert_expr_no_magic_sugar);
    for stmt in &program.statements {
        check(stmt);
    }
    for stmt in program.end_block.iter().flatten() {
        check(stmt);
    }
}

/// The per-expression half of [`assert_no_magic_sugar`] — one arm per node
/// type that can carry a magic name.
fn assert_expr_no_magic_sugar(e: &Expr) {
    match &e.kind {
        ExprKind::SimpleExpansion(n) => assert!(
            super::expr::magic_sugar_expansion(n, e.span).is_none(),
            "ICE: `${}` survived parse as a SimpleExpansion; a construction site is not routed through magic_sugar_expansion (DD#111)",
            n
        ),
        ExprKind::Arithmetic(a) => crate::ir::walk_arith_var_reads(a, &mut |n| {
            assert!(
                super::expr::magic_sugar_intrinsic(n).is_none(),
                "ICE: `${}` survived parse as an ArithExpr::DollarVar; a construction site is not routed through magic_sugar_intrinsic (DD#111)",
                n
            );
        }),
        _ => {}
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

    match parse_statement_here(scanner) {
        // Constructs parsed only so they can be reported, not accepted.
        Ok(stmt) => {
            check_rejected(&stmt, diags);
            stmts.push(stmt);
        }
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
///
/// `None` means "there is no statement here", and EOF is the only thing that
/// produces it. Callers that have already established `!at_eof()` should call
/// `parse_statement_here` instead — handling an `Option` they cannot receive
/// is what left `parse_top_level_item` and `parse_body_until` with a cold arm
/// each.
pub fn parse_statement(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    skip_terminators(scanner);
    if scanner.at_eof() {
        return Ok(None);
    }
    parse_statement_here(scanner).map(Some)
}

/// Parse a statement the caller has already established is there.
pub(super) fn parse_statement_here(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
    let start = scanner.peek().span.start;
    let tok = scanner.peek().token.clone();
    let span = span_from(scanner, start);

    let stmt = dispatch_statement(scanner, tok, span)?;

    if is_compound(&stmt.kind) {
        reject_compound_postfix(stmt, scanner)
    } else {
        maybe_wrap_redirects(scanner, stmt)
    }
}

/// Dispatch on the leading token of a statement. Pure routing — each
/// arm calls the corresponding `parse_*` subparser, which is responsible
/// for advancing the scanner.
fn dispatch_statement(
    scanner: &mut Scanner,
    tok: Tok,
    span: Span,
) -> Result<Statement, Diagnostic> {
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
        // A `}` only reaches here when no enclosing block's stop set claimed
        // it (a block body breaks on `}` before parsing a statement), so it
        // closes nothing. bash would have kept it literal — say so, rather
        // than falling through to a bare "expected expression".
        Tok::RBrace => Err(parse_error(
            span,
            "SB4072",
            "a `}` here closes nothing — no block is open. bash would keep it \
             literal; quote it for a literal brace (`'a}b'`)"
                .to_string(),
        )),
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
) -> Result<Statement, Diagnostic> {
    let next = scanner.peek().token.clone();
    if let Some((code, msg)) = compound_postfix_rejection(&next) {
        return Err(parse_error(stmt.span, code, msg));
    }
    Ok(stmt)
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
        stmts.push(parse_statement_here(scanner)?);
    }
    Ok(stmts)
}

fn tok_eq(a: &Tok, b: &Tok) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Parse a compound list (condition) — one or more statements.
///
/// Every caller is a control-flow *header* (`if`/`elif`/`while`/`until`),
/// so this is also where the header region is marked: inside it a `{`
/// terminates a command's word list unconditionally, because it opens the
/// block body (DD#68). See [`Scanner::enter_header`].
fn parse_compound_list(scanner: &mut Scanner, stop: &[Tok]) -> Result<Vec<Statement>, Diagnostic> {
    scanner.enter_header();
    let result = parse_body_until(scanner, stop);
    scanner.exit_header();
    result
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

fn parse_if(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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

/// Build the `If` statement both spellings of `if` share, applying the
/// DD#121 negated-attest normalisation on the way through.
///
/// Both `parse_if_keyword` and `parse_if_brace` must go through here: the
/// normalisation is what makes a negated caught attest *unrepresentable*, and
/// a form that is unrepresentable in one spelling but not the other is not
/// normalised at all.
fn make_if(
    span: Span,
    condition: Vec<Statement>,
    then_body: Vec<Statement>,
    elif_clauses: Vec<ElifClause>,
    else_body: Option<Vec<Statement>>,
) -> Statement {
    let mut inner = If {
        condition,
        then_body,
        elif_clauses,
        else_body,
    };
    flip_negated_attest(&mut inner, span);
    Statement {
        kind: StmtKind::If(inner),
        span,
    }
}

/// Test a condition for `if ! attest …` and, when it matches, replace the
/// negated single-stage pipeline with the bare `attest` command. Returns
/// whether it matched.
///
/// A standalone `! cmd` is a `Pipeline` carrying `negated: true`, *not* a
/// `StmtKind::Negated`, which only ever appears as a stage inside a
/// multi-stage pipeline. Requiring exactly one command is what keeps
/// `! attest … | cmd` (not a caught attest, and SB4022 anyway) out.
///
/// Test and strip are one function on purpose. They used to be two — a `&`
/// predicate and a `&mut` rewriter — each destructuring this same shape, and
/// the rewriter's three "already established" early returns were cold because
/// the predicate had proved them. Two destructurings of one shape can drift,
/// and the drift would have been silent in the worst direction: returning
/// early there leaves the negation in place, so `if ! attest … { A } else { B }`
/// keeps its branches unswapped and the program means the opposite of what it
/// says. Having one destructuring is better than having a second one check the
/// first.
fn take_negated_attest(condition: &mut [Statement]) -> bool {
    let [stmt] = condition else {
        return false;
    };
    let StmtKind::Pipeline(p) = &mut stmt.kind else {
        return false;
    };
    if !p.negated {
        return false;
    }
    let is_attest = matches!(p.commands.as_slice(),
        [stage] if matches!(&stage.kind,
            StmtKind::Command(c) if matches!(&c.name.kind, ExprKind::Literal(n) if n == "attest")));
    if !is_attest {
        return false;
    }
    // `is_attest` matched a one-element slice, so index 0 exists.
    *stmt = p.commands.remove(0);
    true
}

/// DD#121: rewrite `if ! attest … { A } else { B }` into the equivalent
/// `if attest … { B } else { A }`.
///
/// DD#121 names three caught forms — `if attest`, `attest && CMD`,
/// `attest || CMD` — and the negated spelling is none of them. Rather than
/// teach the R2 resolution check and the SSA merge to carry a polarity (two
/// places whose "resolve" and "divert" arms would have to invert in step, and
/// silently disagree if they ever drifted), the polarity is removed here and
/// no negated caught attest ever reaches the analysis.
///
/// The swap is only valid once the elif chain is accounted for: the chain
/// hangs off the *unnegated* condition's false arm, so
/// `if !C {A} elif C2 {B} else {D}` is `if C { if C2 {B} else {D} } else {A}`,
/// a nesting rather than a swap. With no `elif` and no `else`, the then-body
/// becomes empty — the attest passing is then simply a no-op, which is what
/// the source says.
fn flip_negated_attest(inner: &mut If, span: Span) {
    if !take_negated_attest(&mut inner.condition) {
        return;
    }
    let fall_through = negated_fall_through(inner, span);
    inner.else_body = Some(std::mem::replace(&mut inner.then_body, fall_through));
}

/// The body the rewritten `if` falls through to when the attest *passes* —
/// the source's `elif`/`else` chain.
///
/// With no `elif`, that is just the `else` body (empty when there was none).
/// With one, the chain has to *nest* rather than move across: it hangs off
/// the unnegated condition being false, so `if !C {A} elif C2 {B} else {D}`
/// is `if C { if C2 {B} else {D} } else {A}`.
fn negated_fall_through(inner: &mut If, span: Span) -> Vec<Statement> {
    let mut rest = std::mem::take(&mut inner.elif_clauses).into_iter();
    let Some(first) = rest.next() else {
        return inner.else_body.take().unwrap_or_default();
    };
    vec![Statement {
        kind: StmtKind::If(If {
            condition: first.condition,
            then_body: first.body,
            elif_clauses: rest.collect(),
            else_body: inner.else_body.take(),
        }),
        span,
    }]
}

/// The `then … [elif …] [else …] fi` sugar form of `if`.
fn parse_if_keyword(
    scanner: &mut Scanner,
    span: Span,
    condition: Vec<Statement>,
) -> Result<Statement, Diagnostic> {
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

    Ok(make_if(span, condition, then_body, elif_clauses, else_body))
}

/// DD#68: the brace form of `if` — `if COND { … } [elif COND { … }]…
/// [else if COND { … }] [else { … }]`. Builds the same `If` IR as the
/// `then/fi` sugar. `else if` desugars to `else` + a nested `if`.
fn parse_if_brace(
    scanner: &mut Scanner,
    span: Span,
    condition: Vec<Statement>,
) -> Result<Statement, Diagnostic> {
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
    Ok(make_if(span, condition, then_body, elif_clauses, else_body))
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
        // `parse_if` cannot answer "not an `if`" — every arm of its trailing
        // match is an `Err` or ends in a statement — so there is nothing to
        // handle here. This was an `SB4027 "expected \`if\`"` diagnostic for a
        // shape no source can produce.
        let nested = parse_if(scanner)?;
        Ok(vec![nested])
    } else {
        parse_brace_block(scanner, span)
    }
}

// ── while/until ──────────────────────────────────────────────

fn parse_while(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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

    Ok(Statement {
        kind: StmtKind::While(While {
            condition,
            body,
            until,
        }),
        span,
    })
}

// ── for ──────────────────────────────────────────────────────

fn parse_for(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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

    Ok(Statement {
        kind: StmtKind::For(For {
            variable,
            words,
            body,
        }),
        span,
    })
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

fn parse_c_style_for(scanner: &mut Scanner, span: Span) -> Result<Statement, Diagnostic> {
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

    lower_c_style_for(init, cond, update, body, span)
}

// ── function ─────────────────────────────────────────────────

fn parse_function(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume 'fn' or 'function'

    let name = expect_word(scanner, span, "expected function name")?;

    parse_optional_empty_parens(scanner, span)?;
    finish_function_def(scanner, name, span)
}

/// DD#67: bash `name() { ... }` function-definition sugar (no `fn`/
/// `function` keyword). Produces the same `Function` IR as `fn name { }`.
/// The leading `name ( )` shape is recognised by [`is_paren_function`].
fn parse_paren_function(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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
    finish_function_def(scanner, name, span)
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
    let return_type = parse_optional_return_type(scanner)?;

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
fn parse_optional_return_type(scanner: &mut Scanner) -> Result<VarType, Diagnostic> {
    let Tok::Word(w) = &scanner.peek().token else {
        return Ok(VarType::String);
    };
    if w == "-" && matches!(scanner.peek_n(1).token, Tok::Gt) {
        let start = scanner.peek().span.start;
        scanner.next(); // consume -
        scanner.next(); // consume >
        if !matches!(scanner.peek().token, Tok::Int) {
            return Err(parse_error(
                span_from(scanner, start),
                "SB4014",
                "expected 'int' after '->' in function return type".to_string(),
            ));
        }
        scanner.next();
        return Ok(VarType::Int);
    }
    Ok(VarType::String)
}

// ── declarations (int, const, array, map) ────────────────────

fn parse_declaration(scanner: &mut Scanner, keyword: &str) -> Result<Statement, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume keyword

    let (decl_keyword, var_type, flags) = declaration_axes(scanner, keyword);
    let assignments = parse_declaration_assignments(scanner, span)?;
    Ok(Statement {
        kind: StmtKind::Declaration(Declaration {
            keyword: decl_keyword,
            var_type,
            flags,
            assignments,
        }),
        span,
    })
}

/// Resolve a declaration's (keyword, type, structure-flags) axes, consuming any
/// stacked modifier keyword. Handles `const int` and — DD#76 — the top-level
/// `int array` / `int map` stack (DD#35 order: type then structure).
/// const+structure is intentionally left as a compile error (spec 35.2), so it
/// is not stacked here.
fn declaration_axes(scanner: &mut Scanner, keyword: &str) -> (DeclKeyword, VarType, Vec<String>) {
    match (keyword, &scanner.peek().token) {
        ("const", Tok::Int) => {
            scanner.next();
            return (DeclKeyword::Readonly, VarType::Int, vec!["-r".to_string()]);
        }
        ("int", Tok::Array) => {
            scanner.next();
            return (DeclKeyword::Declare, VarType::Int, vec!["-a".to_string()]);
        }
        ("int", Tok::Map) => {
            scanner.next();
            return (DeclKeyword::Declare, VarType::Int, vec!["-A".to_string()]);
        }
        _ => {}
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
        _ => unreachable!("dispatch_statement passes only declaration keywords"),
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
                    // A `$( )` closer ends the statement for the same reason
                    // `}` does. Without this `$( int x=1 )` took the `)` as
                    // the next declarator name and reported SB4014 "expected
                    // variable name" on a declaration that was complete.
                    | Tok::RParen
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

fn parse_group(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume '{'

    let body = parse_body_until(scanner, &[Tok::RBrace])?;
    expect(scanner, &Tok::RBrace, span, "expected '}'")?;

    Ok(Statement {
        kind: StmtKind::Group(Group { body }),
        span,
    })
}

// ── arithmetic (( )) ─────────────────────────────────────────

fn parse_arith_stmt(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
    let sp = scanner.peek().span;
    let span = span_from(scanner, sp.start);
    scanner.next(); // consume ((

    let expr = parse_arith_command(scanner, span)?;
    let stmt = lower_arith_command(expr, span);
    Ok(stmt)
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
fn parse_return_producer_head(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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
fn parse_return_value_form(scanner: &mut Scanner, after: &Tok) -> Result<Statement, Diagnostic> {
    if super::expr::keyword_to_str(after).is_some() || matches!(after, Tok::Dash | Tok::Eq) {
        let bad = scanner.peek_n(1).span;
        return Err(return_producer_error(span_from(scanner, bad.start)));
    }
    parse_simple_or_assignment(scanner)
}

fn parse_return(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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
    // DD#74: `return` takes a status *producer*, and a producer can be a
    // composite — `return A | B`, `return A && B`, `return A > f`. The
    // per-kind parse above yields only the first command / `[[ ]]` /
    // `(( ))`; fold any trailing pipeline / `&&`/`||` list / redirect
    // into the producer (the same `maybe_wrap_redirects` a normal
    // statement gets) so the whole composite is consumed by the return,
    // not left as an uncaught pipeline stage after a stage-0 `return`.
    //
    // `parse_return_producer_head` always answers — its catch-all arm is
    // `parse_simple_or_assignment`. The `None => Err(return_producer_error(..))`
    // that stood here was a second, unreachable site for the very message
    // `parse_return_value_form` raises live, so probing the message found
    // the other one and read as proof this arm worked.
    let folded = maybe_wrap_redirects(scanner, producer)?;
    Ok(Statement {
        kind: StmtKind::ReturnProducer(Box::new(folded)),
        span,
    })
}

fn parse_negated(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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
    let stmt = dispatch_statement(scanner, tok, unit_span)?;
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
    Ok(Statement {
        kind: StmtKind::Pipeline(Pipeline {
            commands: vec![stmt],
            negated: true,
        }),
        span,
    })
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

fn parse_suppressed(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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
                        // The peek above established that a word-like token or a compound
                        // keyword follows the `-`, so a statement is here by construction and
                        // `parse_statement`'s "nothing here" answer was unreachable.
        let stmt = parse_statement_here(scanner)?;
        Ok(push_suppression_into_pipeline(stmt, span))
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

fn parse_test_command(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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

    Ok(Statement {
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
    })
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
) -> Result<Statement, Diagnostic> {
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

    Ok(Statement {
        kind: StmtKind::Case(Case { value, items, op }),
        span,
    })
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

fn parse_attest(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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

    // Expect the predicate: `[[ … ]]`, the DD#93 Part D `(( … ))`, or the
    // Part B bare-unary shorthand. Mandatory — see `parse_attest_predicate`.
    args.push(parse_attest_predicate(scanner, &args, span)?);

    Ok(Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("attest".to_string()),
                span,
            },
            args,
        }),
        span,
    })
}

/// The attest predicate, in its three spellings — and it is **mandatory**.
///
/// Every form DD#93 documents carries one, and `classify_attest` answers
/// `None` without it, which `handle_attest_stmt` treats as impossible: its
/// ICE says "`attest` shape is fixed by `parse_attest` before lowering
/// (SB4014)". That was false. The scalar path is guarded — this is only
/// entered after a `[[`, `((` or unary-operator peek — but the attest-all
/// path is not, so `attest x in a` with nothing after it parsed clean,
/// slipped `check_attest_form`'s `let … else { return }`, and crashed the
/// compiler in the transform. Raising here is what makes the ICE's claim
/// true.
fn parse_attest_predicate(
    scanner: &mut Scanner,
    args: &[Expr],
    span: Span,
) -> Result<Expr, Diagnostic> {
    if matches!(scanner.peek().token, Tok::DblBracketOpen) {
        scanner.next();
        let test_expr = parse_test_expr(scanner, span)?;
        expect(
            scanner,
            &Tok::DblBracketClose,
            span,
            "expected ']]' in attest",
        )?;
        Ok(Expr {
            kind: ExprKind::Test(test_expr),
            span,
        })
    } else if matches!(scanner.peek().token, Tok::DblParen) {
        // DD#93 Part D: the int-assignment is the predicate. Parsed as an
        // attest predicate, never as a free arithmetic statement — which is
        // why the bare source needs no DD#77 disposition here.
        scanner.next();
        let arith = parse_arith_command(scanner, span)?;
        Ok(Expr {
            kind: ExprKind::Arithmetic(Box::new(arith)),
            span,
        })
    } else if let Some(op) = peek_unary_test_op(scanner) {
        // DD#93 Part B: `attest VAR OP` ≡ `attest VAR [[ OP $VAR ]]`.
        scanner.next();
        Ok(Expr {
            kind: ExprKind::Test(attest_shorthand_test(&op, args, span)),
            span,
        })
    } else {
        // Reachable only from the attest-all lead (DD#113 keeps `(( … ))`
        // scalar-only, so `[[ … ]]` or a Part B operator is what can follow).
        Err(parse_error(
            span,
            "SB4014",
            "expected '[[ … ]]' or a test operator in attest-all (attest x in a [[ … ]])"
                .to_string(),
        ))
    }
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
        _ => unreachable!("parse_attest builds each binder as a literal name"),
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
    // DD#113 reserves arithmetic attest-all shape errors for validate's SB5054.
    if binders.len() > 2 && !matches!(scanner.peek().token, Tok::DblParen) {
        return Err(parse_error(
            span,
            "SB4014",
            "attest-all takes one array binder or two map binders before 'in'".to_string(),
        ));
    }
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

fn parse_simple_or_assignment(scanner: &mut Scanner) -> Result<Statement, Diagnostic> {
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
    // in `parse_split_subscript_index` — hence the `base` of 1.
    lhs_index_close_from(scanner, 1).is_some()
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

fn parse_assignment(scanner: &mut Scanner, span: Span) -> Result<Statement, Diagnostic> {
    let name_tok = scanner.next();
    let Tok::Word(raw_name) = name_tok.token else {
        // `parse_simple_or_assignment` reaches here only when
        // `looks_like_assignment` matched, and that returns false unless
        // `scanner.peek()` is a `Tok::Word` — the very token consumed above.
        unreachable!(
            "looks_like_assignment guarantees a Tok::Word head, got {:?}",
            name_tok.token
        )
    };

    let (name, index) = parse_assignment_lhs(scanner, raw_name, span)?;

    let op = parse_assign_op(scanner);

    // Check for array literal: =(...)
    if matches!(scanner.peek().token, Tok::LParen) {
        scanner.next();
        let arr = parse_array_literal(scanner, span)?;
        return Ok(Statement {
            kind: StmtKind::Assignment(Assignment {
                name,
                index,
                op,
                value: arr,
            }),
            span,
        });
    }

    // Check for empty assignment (newline/semi/eof immediately)
    if scanner.at_eof() || matches!(scanner.peek().token, Tok::Newline | Tok::Semi) {
        return Ok(Statement {
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
        });
    }

    let value = parse_assignment_value(scanner)?;

    Ok(Statement {
        kind: StmtKind::Assignment(Assignment {
            name,
            index,
            op,
            value,
        }),
        span,
    })
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
        let idx = parse_split_subscript_index(scanner, span);
        return Ok((base.to_string(), Some(idx)));
    }
    let Some(bracket_pos) = raw_name.find('[') else {
        return Ok((raw_name, None));
    };
    let name = raw_name[..bracket_pos].to_string();
    let rest = &raw_name[bracket_pos + 1..];
    // The lexer's `braceless_subscript` returns `None` on unbalanced brackets,
    // so `a[0=5` never becomes an assignment LHS at all — it stays a word and
    // SB5011 rejects it. A `[` here therefore always has its `]`.
    let close = rest.find(']').unwrap_or_else(|| {
        unreachable!(
            "an assignment LHS reaches here only through the lexer's balanced \
             subscript scan, so `{raw_name}` must close its `[`"
        )
    });
    let idx = rest[..close].to_string();
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
///
/// DD#116: unreachable by construction, so it ICEs rather than returning a
/// diagnostic. This is reached only from `parse_assignment_lhs`'s
/// `strip_suffix('[')` branch, and `looks_like_assignment` commits to that
/// branch only via [`looks_like_dynamic_subscript_assign`], which has already
/// run [`lhs_index_close_from`] over the same absolute token window and found
/// a `]`. When the probe's window comes up empty the statement is never an
/// assignment at all — a 40-token index is SB5011 as a command name, not
/// SB4014 — so an error here could only describe a *supported* index as
/// unsupported, pointing the user at source that is fine.
fn parse_split_subscript_index(scanner: &mut Scanner, span: Span) -> Expr {
    let Some(close) = lhs_index_close_from(scanner, 0) else {
        unreachable!(
            "looks_like_dynamic_subscript_assign already found this `]` with \
             lhs_index_close_from over the same window"
        )
    };
    let start = scanner.peek().span.start;
    let end = scanner.peek_n(close).span.start;
    let text = scanner.source_slice(start, end).to_string();
    for _ in 0..=close {
        scanner.next();
    }
    parse_subscript_index(&text, span)
}

/// Consume the assignment operator, which is the token immediately after the
/// LHS.
///
/// Checked rather than assumed. This used to be a bare `scanner.next();
/// // consume =` on the non-`+=` path — it did not consume the `=`, it
/// consumed whatever stood there and called it the `=`. The comment stated an
/// invariant the code never tested, and when the shape probe and
/// `parse_split_subscript_index` disagreed about which `]` closed the index,
/// `m[$s ] x ]=9` swallowed the ` x ` as the operator and assigned the literal
/// `]=9`.
///
/// The error arm is unreachable while the probe and the LHS parser agree,
/// which [`lhs_index_close_from`] now makes structural rather than a
/// coincidence of two hand-matched lookahead windows.
///
/// DD#116: unreachable by construction, so it ICEs. Both of
/// [`looks_like_assignment`]'s branches guarantee the operator token.
/// `is_identifier_or_subscript` requires a `]` suffix, so a bare `name[` can
/// never take the first branch — which means the plain and `name[lit]` shapes
/// consume nothing past the name and land on the very token the probe checked
/// at `peek_n(1)`; and the dynamic-subscript branch consumes through the `]`
/// that [`lhs_index_close_from`] found *because it was followed by* `=`/`+=`.
/// A diagnostic here could only tell a user their assignment lacks the `=` it
/// visibly has.
fn parse_assign_op(scanner: &mut Scanner) -> AssignOp {
    match scanner.peek().token {
        Tok::PlusAssign => {
            scanner.next();
            AssignOp::Append
        }
        Tok::Eq => {
            scanner.next();
            AssignOp::Set
        }
        ref other => unreachable!(
            "looks_like_assignment commits only on a following `=`/`+=`, got {other:?}"
        ),
    }
}

/// Offset of the `]` token that closes an assignment-LHS subscript, or `None`
/// if there is none within [`MAX_LHS_INDEX_TOKENS`].
///
/// `base` is the offset of the first index token, which differs by one between
/// the two callers: the shape probe runs with the `name[` word still
/// unconsumed, the parser after it. **Both must call this**, because the `]`
/// they pick has to be the same one — the probe commits to the parse on the
/// strength of a `]` followed by `=`/`+=`, so a parser that closed on a
/// *different* `]` would leave the scanner somewhere the probe never checked.
/// That is not hypothetical: when this was two searches with two predicates,
/// `m[$s ] x ]=9` closed on the first `]` and the ` x ` was then consumed as
/// if it were the `=`, silently assigning the literal `]=9`.
fn lhs_index_close_from(scanner: &mut Scanner, base: usize) -> Option<usize> {
    (base..base + MAX_LHS_INDEX_TOKENS).find(|&i| {
        matches!(&scanner.peek_n(i).token, Tok::Word(rb) if rb == "]")
            && matches!(scanner.peek_n(i + 1).token, Tok::Eq | Tok::PlusAssign)
    })
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
) -> Result<Statement, Diagnostic> {
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
    Ok(Statement {
        kind: StmtKind::Assignment(Assignment {
            name: var_name.to_string(),
            index: None,
            op: AssignOp::Set,
            value,
        }),
        span,
    })
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

/// A single-quoted command head denotes a literal command name.
///
/// Quoting is how a name containing a word-break byte is said at all —
/// `'/opt/my tool'` has no bare spelling, and before this the compiler simply
/// could not run such a path. It is *only* for that: DG, 2026-10-10 — a quoted
/// head "should only be used when there is no unquoted way to say the command
/// name", so `'echo'` is SB5096 rather than a second spelling of `echo`.
///
/// Normalizing here rather than in `validate` is deliberate. The rest of the
/// compiler holds "a command head is `ExprKind::Literal`" as an invariant —
/// `ssa_taint` and `capture_purity` both carry an `unreachable!` naming
/// SB5011 as its guarantee — so a `SingleQuoted` head that survived parsing
/// would reach `transform` and ICE. Rewriting it to `Literal` keeps the
/// invariant true by construction.
///
/// An *empty* head is SB4074, a syntax error rather than a resolution one.
/// DG, 2026-10-11: "an empty name can neither name an intrinsic nor an
/// external — strikes me as more of a syntax error". Quoting exists to say a
/// name that has no bare spelling; it is not a way to say the absence of a
/// name. Rejecting it here is what keeps every later pass free of the case:
/// `''` was previously the one spelling that could put an `ExprKind::Literal`
/// of zero length in command position, and it reached `validate` as a *pair*
/// of diagnostics (SB4025 + SB5011) where every other unrunnable head gets
/// exactly one.
///
/// Every *quoted constant* head is judged here, not just a wholly
/// single-quoted one (DG, 2026-10-11). The six shapes `""`, `"echo"`,
/// `"/bin/echo"`, `"ec"ho`, `ec"ho"` and `'ec'ho` used to fall through to
/// `validate`'s general SB5011, which explains the rejection as "a variable
/// or expansion can't be resolved at compile time" — a cause none of them
/// has. `bad_command_head_diag`'s own doc names that fault and special-cases
/// it for `a-b=1`; these were the same fault, unfixed. Deciding them here
/// leaves SB5011's text true of every input that still receives it.
///
/// The gate is `head_contains_quote`, and it is load-bearing: `a-b=1` is a
/// `Concat` of bare literals, so a constancy test alone would capture it and
/// replace its good diagnostic with a quoting one. This family is about
/// quoting, so an unquoted head is not its business.
fn normalize_command_head(name: Expr) -> Result<Expr, Diagnostic> {
    if !head_contains_quote(&name.kind) {
        return Ok(name);
    }
    let Some(text) = constant_head_text(&name.kind) else {
        return Ok(name);
    };
    if text.is_empty() {
        return Err(empty_command_head_diag(name.span));
    }
    if has_bare_spelling(&text) {
        return Err(superfluous_quoted_head_diag(&text, name.span));
    }
    // No bare spelling, so quoting is required — but only one spelling of it
    // is accepted, a single word in single quotes. Double quotes would make
    // the head a runtime value, and a concatenation says in pieces what one
    // quoted word says whole.
    let ExprKind::SingleQuoted(text) = name.kind else {
        return Err(wrong_quoted_head_diag(&text, name.span));
    };
    Ok(Expr {
        kind: ExprKind::Literal(text),
        span: name.span,
    })
}

/// Whether a command head is quoted anywhere within it.
///
/// **Total over `ExprKind` on purpose — do not add a wildcard.**
fn head_contains_quote(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::SingleQuoted(_) | ExprKind::DoubleQuoted(_) => true,
        ExprKind::Concat(parts) => parts.iter().any(|p| head_contains_quote(&p.kind)),
        ExprKind::Literal(_)
        | ExprKind::SimpleExpansion(_)
        | ExprKind::ParameterExpansion(_)
        | ExprKind::Subscript { .. }
        | ExprKind::CommandSubstitution(_)
        | ExprKind::SlurpFile(_)
        | ExprKind::Arithmetic(_)
        | ExprKind::Array(_)
        | ExprKind::AssocArray(_)
        | ExprKind::Test(_) => false,
    }
}

/// The compile-time text of a command head, or `None` when any part of it is
/// resolved at runtime.
///
/// **Total over `ExprKind` on purpose — do not add a wildcard.**
fn constant_head_text(kind: &ExprKind) -> Option<String> {
    match kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => Some(s.clone()),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            let mut text = String::new();
            for part in parts {
                text.push_str(&constant_head_text(&part.kind)?);
            }
            Some(text)
        }
        // A value, a capture, a file read or an arithmetic result — only the
        // runtime knows these, which is exactly what SB5011 says.
        ExprKind::SimpleExpansion(_)
        | ExprKind::ParameterExpansion(_)
        | ExprKind::Subscript { .. }
        | ExprKind::CommandSubstitution(_)
        | ExprKind::SlurpFile(_)
        | ExprKind::Arithmetic(_) => None,
        // Not a word at all. Rejected elsewhere, and not by a quoting rule.
        ExprKind::Array(_) | ExprKind::AssocArray(_) | ExprKind::Test(_) => None,
    }
}

/// SB4074. Deliberately does not name a spelling: `''` and `""` are both
/// zero-length names, and so is any concatenation of empty quoted pieces.
fn empty_command_head_diag(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4074",
        "empty command name — a name of zero length can denote neither a \
         sash intrinsic nor a program on disk; quotes in command position \
         say a name that has no bare spelling, such as a path containing a \
         space, not the absence of a name"
            .to_string(),
    )
}

/// SB5096 for a quoted constant that did not need quoting at all. Renders the
/// text unquoted, because the head may have been written with either quote
/// character or assembled from several pieces, and the advice is the same for
/// all of them: write the one bare word.
fn superfluous_quoted_head_diag(text: &str, span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB5096",
        format!(
            "command name `{text}` is quoted but can be written without \
             quotes — write `{text}`; quote a command name only when it has \
             no bare spelling, as with a path containing a space"
        ),
    )
}

/// SB5096 for a constant that *does* need quoting but is not said the one way
/// sash accepts. Same code as the superfluous case on purpose: both are the
/// single rule "a command name is one bare word, or one single-quoted word
/// when it has no bare spelling", and a second code would split one rule in
/// two.
///
/// Double quotes are not merely redundant here — they are the spelling that
/// interpolates, so accepting them would make the head's constancy an
/// accident of its contents.
fn wrong_quoted_head_diag(text: &str, span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB5096",
        format!(
            "command name `{text}` has no bare spelling, so it must be one \
             single-quoted word — write `'{text}'`; double quotes in command \
             position would make the name a runtime value, and a \
             concatenation says in pieces what one quoted word says whole"
        ),
    )
}

/// Collect a command's words and redirects until a terminator.
fn parse_command_words(
    scanner: &mut Scanner,
    span: Span,
) -> Result<(Vec<Expr>, Vec<Redirect>), Diagnostic> {
    let mut args = Vec::new();
    let mut redirects: Vec<Redirect> = Vec::new();
    while !scanner.at_eof() {
        if !parse_command_element(scanner, span, &mut args, &mut redirects)? {
            break;
        }
    }
    Ok((args, redirects))
}

fn parse_command(scanner: &mut Scanner, span: Span) -> Result<Statement, Diagnostic> {
    let name_expr = normalize_command_head(parse_expr(scanner)?)?;
    let (args, redirects) = parse_command_words(scanner, span)?;

    if !redirects.is_empty() {
        reject_redirect_on_no_output_builtin(&name_expr, span)?;
    }

    let mut stmt = Statement {
        kind: StmtKind::Command(Command {
            name: name_expr,
            args,
        }),
        span,
    };

    if !redirects.is_empty() {
        stmt = Statement {
            kind: StmtKind::Redirected(Redirected {
                body: Box::new(stmt),
                redirects,
            }),
            span,
        };
    }

    Ok(stmt)
}

/// A `{` has been reached while collecting a command's word list. Decide
/// whether it terminates the list or is an error.
///
/// Adjacency is the discriminator. `echo a{echo z}` glues the brace to the
/// word, which in bash is a literal `a{echo z}`; sash used to end the command
/// there and open an anonymous group, so it silently printed `a` then `z`.
/// That is SB4071. A *spaced* `{` (`cmd || { a; b; }`) reads as a deliberate
/// group and still terminates the list, as does any `{` inside a control-flow
/// header, where it opens the block body (DD#68) — `if /bin/true a{echo y}`
/// stays legal even though the brace is adjacent.
fn brace_after_word(scanner: &mut Scanner) -> Result<bool, Diagnostic> {
    if scanner.in_header() || !scanner.next_is_adjacent() {
        return Ok(false);
    }
    let sp = scanner.peek().span;
    Err(parse_error(
        span_from(scanner, sp.start),
        "SB4071",
        "a `{` here is glued to the preceding word — bash would keep it \
         literal, but sash would read it as a block. Quote it for a literal \
         brace (`'a{b}'`), or put a space before it for a block"
            .to_string(),
    ))
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
    if matches!(scanner.peek().token, Tok::LBrace) {
        return brace_after_word(scanner);
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
    // The guard above already consumed the only question about this token, and
    // nothing since has advanced the scanner, so this stage is unconditional.
    // It was a `while` once, and re-testing the same token was how the old code
    // could fall through without consuming anything — the path that dropped a
    // trailing `|` on the floor (SB4056).
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
    // `commands` started at one stage and `append_pipeline_stage` pushes or
    // extends on every arm — and no `Pipeline` is built anywhere with an empty
    // `commands` (the only two sites are this one, which needs two, and
    // `parse_negated`'s one-command wrapper). So this is a multi-stage pipeline
    // by construction and the stage checks are unconditional.
    // DD#55 / SB4023: `pipeline && C` / `pipeline || C` is rejected — see
    // reject_pipeline_and_or_postfix.
    reject_pipeline_and_or_postfix(&commands, span)?;
    reject_statement_form_pipeline_stage(&commands)?;
    reject_nonconsuming_final_stage(&commands)?;
    Ok(Statement {
        kind: StmtKind::Pipeline(Pipeline {
            commands,
            negated: false,
        }),
        span,
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
/// Compound commands (`if`, `for`, `match`) in a *non-final* stage are already
/// rejected by `compound_postfix_rejection` (the `|` follows the compound, so
/// the parser sees it as a postfix); in the *final* stage they are rejected by
/// `reject_nonconsuming_final_stage`. These three were the statement forms
/// neither classified as compound. `&&`/`||` on a test stays legal — only the
/// pipe discards the result.
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

/// DD#122: the final stage of a pipeline must *consume* the pipe.
///
/// A pipeline delivers its data one of two ways. An ordinary command stage is
/// forked and inherits the pipe as fd 0. A `while` final stage runs in the
/// **parent** — that is the whole point of the shape, since
/// `cmd | while readline v; do (( n = n + 1 )); done` leaves `n` set where
/// bash, which subshells the loop, loses it — so the pipe reaches it only
/// through a `readline`/`read`.
///
/// Every other compound is forked like bash's `( … )`: it reads the pipe and
/// then throws away everything it assigned. `cmd | { readline g; }` ran, read
/// a line into `g`, and left `g` unset with no diagnostic; `cmd | if readline
/// g; then …; fi` took the right branch and still lost `g`. That is exactly
/// the bundling DD#38 rejects `( … )` for, arriving through a different
/// syntactic door — so it gets DD#38's answer, and SB4022's existing advice:
/// wrap it in a function, which *is* a legal stage.
///
/// Runs after [`reject_statement_form_pipeline_stage`], which has specific
/// wording for `[[ … ]]`/`attest`/`(( … ))`. The default here is to reject, so
/// a new `StmtKind` fails closed.
fn reject_nonconsuming_final_stage(commands: &[Statement]) -> Result<(), Diagnostic> {
    let last = unwrap_stage_wrappers(&commands[commands.len() - 1]);
    match &last.kind {
        StmtKind::Command(_) => Ok(()),
        StmtKind::While(_) if last.binds_a_pipe_reader() => Ok(()),
        StmtKind::While(_) => Err(parse_error(
            last.span,
            "SB4073",
            "this `while` is a pipeline's last stage but never reads it — the \
             loop runs in the parent, so the pipe reaches it only through \
             `readline` or `read`; add one, or drop the `|`"
                .to_string(),
        )),
        _ => Err(parse_error(
            last.span,
            "SB4022",
            "a compound command isn't allowed as a pipeline's last stage — it \
             runs in a forked child, so everything it assigns is discarded; \
             consume the pipe with `while readline NAME` or `read ARRAY`, or \
             wrap the compound in a function"
                .to_string(),
        )),
    }
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
        let (op, spelling) = match scanner.peek().token {
            Tok::AndAnd => (ListOp::And, "&&"),
            Tok::OrOr => (ListOp::Or, "||"),
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
                    spelling
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
///
/// Keeps its `Option` unlike the rest of the statement parsers: `None` here
/// is live, and is what `parse_trailing_pipeline` turns into SB4056 for a
/// dangling `/bin/true &&`.
fn parse_list_operand(scanner: &mut Scanner) -> Result<Option<Statement>, Diagnostic> {
    skip_terminators(scanner);
    if scanner.at_eof() {
        return Ok(None);
    }
    let start = scanner.peek().span.start;
    let tok = scanner.peek().token.clone();
    let span = span_from(scanner, start);
    let stmt = dispatch_statement(scanner, tok, span)?;
    if is_compound(&stmt.kind) {
        // Compounds can't take a postfix `&&`/`||`/redirect/pipe (SB4023/…).
        reject_compound_postfix(stmt, scanner).map(Some)
    } else {
        Ok(Some(wrap_pipeline_part(scanner, stmt)?))
    }
}

/// SB4046 for a trailing redirect on a builtin that produces no output.
///
/// `parse_command` is the one place that can see both the command head and
/// its trailing redirects before they are fused into a `Redirected` node, so
/// it is where the closed allow-list is applied. Rejecting here rather than in
/// a later pass is deliberate: the two sites that used to discard these
/// redirects (the `_ => {}` in `attach_redirects_to_last_inst`, and the
/// codegen arms that take no `redirects` parameter) are both downstream, and a
/// gate upstream of both means neither can be reached holding a redirect it
/// would silently drop.
///
/// Only a *literal* head is judged. `$cmd >f` is an expansion whose value is
/// unknown here and is an external command anyway, so it is allowed through —
/// the same reasoning `normalize_command_head` uses to leave expansions to
/// SB5011.
///
/// Asks [`crate::resolve::head_may_take_redirect`], not
/// `BuiltinCmd::may_take_redirect`. The enum predicate cannot answer for a
/// name `from_name` does not know, and the twelve `INTRINSICS` value
/// producers are exactly those names.
fn reject_redirect_on_no_output_builtin(name: &Expr, span: Span) -> Result<(), Diagnostic> {
    let ExprKind::Literal(n) = &name.kind else {
        return Ok(());
    };
    if crate::resolve::head_may_take_redirect(n) {
        return Ok(());
    }
    Err(parse_error(
        span,
        "SB4046",
        format!(
            "a redirect cannot attach to `{n}` — redirects attach to a command that produces output"
        ),
    ))
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
/// caught ICE.
///
/// **That is not hypothetical — it happened, and this ICE is what caught it.**
/// `parse_attest` builds its `StmtKind::Command` by hand instead of going
/// through `parse_simple_command`, so it never absorbed its trailing
/// redirect, and `attest s [[ -n $s ]] >f` panicked the compiler for all
/// three redirect operators. The repair is an arm in
/// `noncommand_redirect_target`, not a wildcard here. So the older line of
/// this doc — "no source text can reach it" — was false when written, and the
/// unit test that stood in for source coverage could not tell: it constructs
/// the one shape it already knows about. Treat a panic from here as a report
/// that some parser path builds a command without absorbing its redirects,
/// and add the arm rather than silencing the ICE.
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
///
/// `attest` is the third, and it arrived as the ICE in `redirect_reject_noun`
/// firing on real source text — which that function's own doc had called
/// unreachable. `parse_attest` builds its `StmtKind::Command` directly and
/// never routes through `parse_simple_command`, so it is the one command that
/// reaches here still holding its trailing redirect. That is precisely the
/// "some statement kind later gains the ability to carry a redirect past
/// `parse_simple_command`" regression the ICE was placed to catch, and it
/// caught it: `attest s [[ -n $s ]] >f` panicked the compiler for every
/// redirect operator. An `attest` produces no output, so it belongs with the
/// tests above rather than being given a redirect to honour.
fn noncommand_redirect_target(kind: &StmtKind) -> Option<&'static str> {
    match kind {
        StmtKind::Function(_) => Some("a function definition"),
        StmtKind::Declaration(_) => Some("a variable declaration"),
        StmtKind::Assignment(_) => Some("a variable assignment"),
        StmtKind::ArithTest(_) => Some("an arithmetic test"),
        StmtKind::Command(c) if is_bracket_test(c) => Some("a `[[ … ]]` test"),
        StmtKind::Command(c) if is_named_command(c, "attest") => Some("an `attest`"),
        _ => None,
    }
}

/// The `[[ … ]]` form, which `parse_test_command` builds as a command whose
/// name is the literal `[[`.
fn is_bracket_test(c: &Command) -> bool {
    is_named_command(c, "[[")
}

/// A command whose head is exactly the literal `name`. Only meaningful for
/// the parser-built commands, whose heads are always `ExprKind::Literal`.
fn is_named_command(c: &Command, name: &str) -> bool {
    matches!(&c.name.kind, ExprKind::Literal(n) if n == name)
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
    // Every caller gates the token first: the `match` arm in
    // `parse_command_element` lists exactly `is_redirect_op`'s set,
    // `fd_prefix_binds` gates `parse_fd_redirect`, and `next_trailing_redirect`
    // calls `is_redirect_op` outright. `AmpGt` and `HereString` return above,
    // leaving precisely the six tokens `redirect_op_for_token` maps. This was an
    // `SB4014 "expected redirect operator"` diagnostic for a shape no source can
    // produce; the ICE instead claims the callers still gate, which none of them
    // can check about the others.
    let Some((fd, op)) = redirect_op_for_token(&tok.token) else {
        unreachable!("parse_redirect's callers gate on the redirect-token set")
    };
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
        _ => unreachable!("unrecognized parameter operator carries a literal remainder"),
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
/// `@X` transform): indirect `${!x}` (SB4003), an unsupported colon operator
/// on a cell (SB4039), and the `@`-transform family (SB4015). `None` if `op` is
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
                "unsupported parameter operator on an array cell (`${{{}[…]:…}}`) — \
                 use a disposition (`:-` / `:?` / `:~`), the `:!` trust modifier, \
                 substring `:off:len`, or sed `:s/…/…/`",
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

/// The `unreachable!` in [`build_expr_from_parts`] rests on an arithmetic
/// argument about its two call sites, not on a gate any program can trip:
/// both slice a `Concat` around a *middle* index, so neither side can be
/// empty. A unit test calling it with the empty slice is the only way to
/// prove the guard is loud rather than silently returning `Literal("")`.
#[cfg(test)]
mod invariant_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "dispatch_statement passes only declaration keywords")]
    fn declaration_axes_rejects_an_unknown_keyword() {
        let _ = declaration_axes(&mut Scanner::new("x"), "unknown");
    }

    #[test]
    #[should_panic(expected = "parse_attest builds each binder as a literal name")]
    fn attest_shorthand_rejects_a_missing_binder() {
        let _ = attest_shorthand_test("-n", &[], Span::new(1, 1));
    }

    #[test]
    #[should_panic(expected = "parse_attest builds each binder as a literal name")]
    fn attest_shorthand_rejects_a_nonliteral_binder() {
        let span = Span::new(1, 1);
        let lead = [Expr {
            kind: ExprKind::SimpleExpansion("x".into()),
            span,
        }];
        let _ = attest_shorthand_test("-n", &lead, span);
    }

    #[test]
    #[should_panic(expected = "unrecognized parameter operator carries a literal remainder")]
    fn unrecognized_operator_rejects_a_missing_remainder() {
        let span = Span::new(1, 1);
        let expr = Expr::param("x", Some("unrecognized"), None, span);
        let ExprKind::ParameterExpansion(pe) = expr.kind else {
            unreachable!();
        };
        let _ = unrecognized_operator_diag(&pe, span);
    }

    #[test]
    #[should_panic(expected = "unrecognized parameter operator carries a literal remainder")]
    fn unrecognized_operator_rejects_a_nonliteral_remainder() {
        let span = Span::new(1, 1);
        let operand = Expr {
            kind: ExprKind::SimpleExpansion("y".into()),
            span,
        };
        let expr = Expr::param("x", Some("unrecognized"), Some(Box::new(operand)), span);
        let ExprKind::ParameterExpansion(pe) = expr.kind else {
            unreachable!();
        };
        let _ = unrecognized_operator_diag(&pe, span);
    }

    #[test]
    fn nonword_heads_have_no_constant_text() {
        let span = Span::new(1, 1);
        let test = TestExpr::Unary {
            op: "-n".into(),
            operand: Box::new(Expr::literal("x", span)),
        };
        for kind in [
            ExprKind::Array(vec![Expr::literal("echo", span)]),
            ExprKind::AssocArray(vec![("key".into(), Expr::literal("echo", span))]),
            ExprKind::Test(test),
        ] {
            assert_eq!(constant_head_text(&kind), None, "{kind:?}");
        }
    }

    #[test]
    #[should_panic(expected = "neither side is empty")]
    fn build_expr_from_parts_rejects_the_empty_slice() {
        let _ = build_expr_from_parts(&[], Span::new(1, 1));
    }

    /// `parse_redirect` is only ever entered on a token its caller has already
    /// gated — `parse_command_element`'s match arm, `fd_prefix_binds`, or
    /// `is_redirect_op` — so `redirect_op_for_token` cannot return `None` there.
    /// It was an `SB4014` diagnostic, advertising a user-facing error for a
    /// shape no source produces. Only a direct call, deliberately breaking the
    /// caller contract, can reach it; the panic is therefore the claim that the
    /// three gates are still in place, which no one of them can assert alone.
    #[test]
    #[should_panic(expected = "gate on the redirect-token set")]
    fn parse_redirect_rejects_a_token_no_caller_would_pass() {
        let mut sc = Scanner::new("notaredirect");
        let _ = parse_redirect(&mut sc, Span::new(1, 1));
    }

    /// DD#116: `parse_assignment_lhs` reaches its `[` branch only through the
    /// lexer's *balanced* subscript scan, which returns `None` on `a[0` and
    /// leaves it a plain word for SB5011. The old `None => rest.to_string()`
    /// answered the impossible case by silently reading `a[0=5` as index
    /// `0=5`. Only a direct call can reach it.
    #[test]
    #[should_panic(expected = "must close its `[`")]
    fn parse_assignment_lhs_rejects_an_unclosed_subscript() {
        let mut sc = Scanner::new("=5");
        let _ = parse_assignment_lhs(&mut sc, "a[0".to_string(), Span::new(1, 1));
    }

    /// DD#116 ICE. `looks_like_dynamic_subscript_assign` runs
    /// `lhs_index_close_from` over the same window before committing, so a
    /// scanner with no `]` can only arrive here by calling this directly —
    /// which is what makes the panic the claim rather than a prediction.
    #[test]
    #[should_panic(expected = "already found this `]`")]
    fn parse_split_subscript_index_rejects_an_index_it_cannot_span() {
        let long = "$i + ".repeat(MAX_LHS_INDEX_TOKENS + 4);
        let mut sc = Scanner::new(&long);
        let _ = parse_split_subscript_index(&mut sc, Span::new(1, 1));
    }

    /// DD#116 ICE. Both of `looks_like_assignment`'s branches land this
    /// function on a `=`/`+=`; anything else means the probe and the parser
    /// disagreed, which is a compiler bug and not the user's.
    #[test]
    #[should_panic(expected = "commits only on a following")]
    fn parse_assign_op_rejects_a_token_that_is_not_an_assignment() {
        let mut sc = Scanner::new("x");
        let _ = parse_assign_op(&mut sc);
    }

    /// `parse_simple_or_assignment` calls `parse_assignment` only when
    /// `looks_like_assignment` matched, and that returns false unless the head
    /// token is a `Tok::Word`. The arm used to be a `SB4014 "expected variable
    /// name"` diagnostic for a shape no source can produce. Only a direct call
    /// with a non-word head can reach it.
    #[test]
    #[should_panic(expected = "looks_like_assignment guarantees a Tok::Word head")]
    fn parse_assignment_rejects_a_non_word_head() {
        let mut sc = Scanner::new("= 1");
        let _ = parse_assignment(&mut sc, Span::new(1, 1));
    }
}
