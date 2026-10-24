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

use super::expr::keyword_to_str;
use super::lexer::{Scanner, Tok};
use crate::diagnostic::Diagnostic;
use crate::ir::*;

/// Create a parse error diagnostic for arithmetic expressions.
fn arith_error(span: Span, message: String) -> Diagnostic {
    Diagnostic::error_at(span.line, span.col, "SB4020", message)
}

/// DD#111: `$$`/`$?`/`$!` are sugar, not variables. Inside `$(( ))` the
/// intrinsic's arith spelling is the sexpr call, so they desugar to exactly
/// what `(getpid)` parses to — `resolve_nullary_sexpr` then routes each to its
/// lowering. Anything else is a real `$name` operand and stays one.
fn dollar_var_atom(name: String) -> ArithExpr {
    match crate::parse::expr::magic_sugar_intrinsic(&name) {
        Some(intrinsic) => ArithExpr::FuncCall {
            name: intrinsic.to_string(),
            args: vec![],
        },
        None => ArithExpr::DollarVar(name),
    }
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
    // A stray `[` / `]` reaching the numeric-literal path means a subscript
    // did not form — an unbalanced `$a[1`, a misplaced `1[2]`. All three cell
    // spellings read here now (DG 2026-10-04), so name them rather than emit a
    // generic numeric-literal message about a bracket.
    if literal == "[" || literal == "]" {
        return Diagnostic::error_at(
            span.line,
            span.col,
            "SB4035",
            format!(
                "stray `{literal}` in arithmetic — a cell read needs a balanced \
                 subscript: `a[i]` (bare), `$a[i]`, or `${{a[i]}}`"
            ),
        );
    }
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
    let content = collect_arith_content(scanner, span)?;
    let expr = parse_arith_str(&content, span)?;
    // SB5007: reject mutations inside $(( )). The prefix form is already gone
    // — `parse_arith_str` rejects it for every caller.
    if let Some(brief) = expansion_mutation_brief(&expr) {
        return Err(arith_error(
            span,
            format!(
                "mutation of {brief} not allowed in $(( )) expression context — \
                 use (( )) statement instead"
            ),
        ));
    }
    Ok(Expr {
        kind: ExprKind::Arithmetic(Box::new(expr)),
        span,
    })
}

/// The target of a **top-level** `$(( … ))` mutation, or `None`.
///
/// Names the target rather than quoting the expression, matching every other
/// mutation diagnostic (DG, 2026-09-23). Quoting is what let `$(( x *= 2 ))`
/// be reported with an operator the author never wrote: a renderer that had
/// grown a `CompoundAssign` arm elsewhere had not grown one here.
///
/// Top-level only, so **not** `ir::mutation_target_in` — that one descends.
/// A nested mutation is `reject_nested_mutation`'s to report, and it has more
/// to say.
///
/// `SubscriptAssign` is deliberately **not** a mutation for this gate: saying
/// nothing lets through the specific "an assignment or `++`/`--` inside
/// `$(( … ))` can't take effect" that `check_arith_value_mutation` raises for
/// `$(( a[0] -= 2 ))`.
fn expansion_mutation_brief(expr: &crate::ir::ArithExpr) -> Option<String> {
    use crate::ir::ArithExpr;
    match expr {
        ArithExpr::Assign { var, .. }
        | ArithExpr::CompoundAssign { var, .. }
        | ArithExpr::PostIncDec { var, .. } => Some(crate::ir::mutation_target_brief(var, false)),
        _ => None,
    }
}

/// Parse an arithmetic command: (( expr )) as a statement.
/// Returns a structured ArithExpr for the caller to classify and lower.
pub fn parse_arith_command(scanner: &mut Scanner, span: Span) -> Result<ArithExpr, Diagnostic> {
    let content = collect_arith_content(scanner, span)?;
    parse_arith_slot(&content, span)
}

/// Reject prefix ++/-- anywhere in an arithmetic expression.
///
/// Delegates the descent to `ArithExpr::children()` rather than re-deriving
/// the shape. The hand-rolled walk this replaces recursed through `BinOp`,
/// `Assign`, `CompoundAssign`, `Comma` and `Paren` and ended in `_ => Ok(())`,
/// so a prefix form inside a **ternary branch**, a **unary operand**, or the
/// **value of a subscript assignment** was never seen:
///
/// ```text
/// (( 1 ? ++x : 0 ))     (( y = -(++x) ))     (( a[0] = ++i ))
/// ```
///
/// All ten such shapes survived parsing and then hit one of the downstream
/// `PREFIX_INCDEC_INVARIANT` ICEs — an internal-error crash for a form the
/// language simply forbids. The invariant says no pass after the parser can
/// receive a `PreIncDec`; this function is what has to make that true, and a
/// partial walk cannot.
///
/// This is the *third* partial re-derivation of a tree walk found in this
/// file. `reject_nested_mutation` directly below documents the second, with
/// the same remedy: ask the canonical whole-tree helper. `children()` is total
/// over `ArithExpr`, so a new variant forces a decision here instead of
/// silently defaulting to "contains nothing".
fn reject_prefix_incdec(
    expr: &crate::ir::ArithExpr,
    span: crate::ir::Span,
) -> Result<(), Diagnostic> {
    use crate::ir::ArithExpr;
    if let ArithExpr::PreIncDec { var, inc } = expr {
        return Err(prefix_incdec_error(var, *inc, span));
    }
    for child in expr.children() {
        reject_prefix_incdec(child, span)?;
    }
    Ok(())
}

/// SB4020 for `++x` / `--x`, naming the variable and both allowed rewrites.
fn prefix_incdec_error(var: &str, inc: bool, span: crate::ir::Span) -> Diagnostic {
    let op = if inc { "++" } else { "--" };
    let compound = if inc { "+=" } else { "-=" };
    arith_error(
        span,
        format!(
            "prefix {op}{var} is not allowed — use postfix ({var}{op}) or \
             (( {var} {compound} 1 )) instead"
        ),
    )
}

/// Reject nested mutations: e.g. `(( x = y++ ))` has `y++` inside an assignment.
///
/// This must ask `ir::mutation_target_in` — the canonical whole-tree predicate,
/// which answers both "is there one" and "which" — rather than re-deriving the
/// walk. A local copy used to stop at `BinOp` and
/// `Paren` and to omit `SubscriptAssign` from the mutation set entirely, so
/// `(( c = n ? (d++) : 1 ))`, `(( c = n + v[d++] ))`, `(( c = n + -(d++) ))`
/// and `(( c = n + (v[0] = 3) ))` all parsed clean. In a plain `(( … ))`
/// statement `validate::check_arith_test_placement` caught them anyway, but an
/// `attest`'s predicate is an `ExprKind::Arithmetic` that DD#93 deliberately
/// exempts from `check_arith_value_mutation`, leaving this the only gate — and
/// the mutation then reached the value path and hit the `unreachable!()` in
/// `transform::expr_handlers`. A partial re-derivation of a shared predicate is
/// how an ICE gets built.
///
/// The **outer** match stays selective on purpose — it names the nodes that
/// evaluate a sub-expression as an rvalue, which is not the same as "every
/// node". Descending blindly would reject `(( (a=2) ))`, a form the transform
/// hoists and DD#31 allows. But it must list *all* of them: `SubscriptAssign`
/// was missing, so `(( a[0] = y++ ))` fell past this gate to
/// `check_arith_value_mutation` and was told to look inside a `$(( … ))` its
/// author never wrote. Scalar `(( x = y++ ))` named the culprit correctly; the
/// two halves of one construct disagreeing is the same tell as the prefix-`++`
/// index/value split above.
fn reject_nested_mutation(
    expr: &crate::ir::ArithExpr,
    span: crate::ir::Span,
) -> Result<(), Diagnostic> {
    use crate::ir::ArithExpr;
    match expr {
        ArithExpr::Assign { value, .. }
        | ArithExpr::CompoundAssign { value, .. }
        | ArithExpr::SubscriptAssign { value, .. } => {
            match crate::ir::mutation_target_in(value) {
                Some(target) => Err(arith_error(span, format!(
                    "mutation of {target} not allowed in statement (( )) expression context — use (( )) statement instead"
                ))),
                None => Ok(()),
            }
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

/// Parse an arithmetic string (already extracted from delimiters).
/// This is the entry point for the Pratt parser.
pub fn parse_arith_str(input: &str, span: Span) -> Result<ArithExpr, Diagnostic> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(ArithExpr::Literal(0));
    }
    let tokens = tokenize_arith(trimmed);
    // A run of nothing but *terminators* is a legitimately empty expression —
    // `$(( ; ))` is the spelling that reaches here. (The C-style `for (( ;; ))`
    // header does *not*: it splits into three empty slots, each of which takes
    // the `trimmed.is_empty()` return above.) Returning 0 for it here is what
    // lets an EOF in *operand* position below be a hard error.
    // A `,` is a separator and owes an expression on each side, so `,,` is an
    // error rather than an empty expression (DD#94).
    if tokens.iter().all(|t| matches!(t, ATok::Sep)) {
        return Ok(ArithExpr::Literal(0));
    }
    let mut parser = ArithParser {
        tokens,
        pos: 0,
        span,
    };
    let expr = parser.parse_expr(0)?;
    let result = parser.fold_sequence(expr)?;
    reject_trailing_token(&parser, span)?;
    // The single funnel. Every route into arithmetic lands here, so rejecting
    // the prefix form at this one point is what makes
    // `ir::PREFIX_INCDEC_INVARIANT` true for every pass downstream. It used to
    // sit in the two *entry points* instead, which two callers bypass
    // (`emit_string_part` for a `$(( … ))` inside a double-quoted string, and
    // the bare-subscript index re-parse) — so `"$(( ++x ))"` survived to
    // validate and was caught only by riding along in the generic
    // mutation arm, giving a vaguer message than the identical unquoted
    // construct.
    reject_prefix_incdec(&result, span)?;
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
        ArithExpr::PreIncDec { .. } => unreachable!("{}", crate::ir::PREFIX_INCDEC_INVARIANT),
        ArithExpr::Assign { .. }
        | ArithExpr::CompoundAssign { .. }
        | ArithExpr::PostIncDec { .. }
        | ArithExpr::SubscriptAssign { .. } => ArithKind::Mutation,
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

/// Parse one arithmetic *statement* slot and apply the rules every such
/// context shares: no prefix `++`/`--` (DD#31 — "Only postfix `x++` and
/// `x--`. No prefix forms."), and no nested mutation.
///
/// `(( … ))` and all three slots of a C-style `for` header are the same kind
/// of context and must answer to the same rules. They did not: the `for`
/// header called `parse_arith_str` directly and then **discarded the Err**,
/// so a typo in a header was not a diagnostic but a silently different
/// program — see `lower_c_style_for`.
pub fn parse_arith_slot(text: &str, span: Span) -> Result<ArithExpr, Diagnostic> {
    let expr = parse_arith_str(text, span)?;
    reject_nested_mutation(&expr, span)?;
    Ok(expr)
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
///
/// Every slot **propagates** its parse error. It used to swallow all three
/// with `if let Ok(...)`, which did not merely lose a diagnostic — it built a
/// different program. A malformed condition fell back to `true_cmd`, so
/// `for (( i=0; i <* 3; i++ ))` compiled to an infinite loop; a malformed
/// update was dropped, so the counter never advanced (also an infinite loop);
/// a malformed init was dropped, so the loop silently started from the
/// variable's default. The identical typo in a bare `(( … ))` has always been
/// a hard SB4020.
pub fn lower_c_style_for(
    init: &str,
    cond: &str,
    update: &str,
    body: Vec<Statement>,
    span: Span,
) -> Result<Statement, Diagnostic> {
    // Init
    let mut init_stmts = Vec::new();
    let init_trimmed = init.trim();
    if !init_trimmed.is_empty() {
        init_stmts.push(lower_arith_command(
            parse_arith_slot(init_trimmed, span)?,
            span,
        ));
    }

    // Condition. An empty slot is the deliberate `for (( ;; ))` spelling and
    // is the ONLY thing that may become `true` here.
    //
    // This goes through `lower_arith_command`, not straight to
    // `lower_arith_test_structured`, so that a DD#31 comma condition decomposes
    // into a `Group` exactly as it does for `if` / `while` / `until`. Bypassing
    // it left the mutation embedded in a single `ArithTest`, where
    // `check_arith_test_placement` rejected it (SB4020) — so
    // `for (( ; k++, k<3; ))` was the one condition context of four that
    // refused a form bash accepts, and that the other three compile.
    let cond_trimmed = cond.trim();
    let condition = if cond_trimmed.is_empty() {
        vec![Statement::true_cmd(span)]
    } else {
        vec![lower_arith_command(
            parse_arith_slot(cond_trimmed, span)?,
            span,
        )]
    };

    // Update
    let mut loop_body = body;
    let update_trimmed = update.trim();
    if !update_trimmed.is_empty() {
        loop_body.push(lower_arith_command(
            parse_arith_slot(update_trimmed, span)?,
            span,
        ));
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
    Ok(Statement {
        kind: StmtKind::Group(Group { body: group_body }),
        span,
    })
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
    /// `${name[index]}` inside `$(( ))` — a normal-parse subscript rvalue
    /// (name, raw index text). The parser builds the ordinary `Subscript` Expr
    /// (so a map's string key works) and wraps it as `ArithExpr::Expansion`.
    Subscript(String, String),
    /// arith-subscript model B (Stage 2): a **bare** `name[index]` in `$(( ))`
    /// (no `${}`) — an int-array cell, rvalue+lvalue. Carries the name and the
    /// raw index text (a full arith expression, re-parsed by the arith parser).
    BareSubscript(String, String),
    /// `${var:OP…}` inside `$(( ))` — any operator-bearing parameter expansion
    /// (`:-`, `:?`, `#`, `%`, `:s/`, …). Carries the raw `${…}` body; the
    /// parser routes it through the real parameter-expansion parser and wraps
    /// the result as `ArithExpr::Expansion` (DD#77). Before this, such bodies
    /// fell through to a bogus `DollarVar("var:OP…")` → undefined → silent 0
    /// (bug-arith-param-expansion-operator-zero).
    ParamExpansion(String),
    UnterminatedBrace(String),
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
    /// DD#94: a sequence *terminator* — a newline (after a complete
    /// sub-expression) or a `;`. Unlike `,` it tolerates emptiness: a run of
    /// them is trivia, and one may be swallowed by a pending operator.
    Sep,
    LParen,
    RParen,
}

/// The source spelling of an arith token, for diagnostics.
///
/// User-facing messages must name what the user *wrote*. Before this existed,
/// SB4020 formatted the token with `{:?}` and told people
/// `expected an operand in arithmetic expression, found Amp` — an internal
/// variant name for a character they had typed as `&`. The match is total so
/// a new `ATok` is a compile error here rather than another leaked name.
///
/// **Total over `ATok` on purpose — do not add a wildcard.**
fn atok_to_text(tok: &ATok) -> String {
    match tok {
        ATok::Num(n) => n.to_string(),
        ATok::BadNum(s) | ATok::BadLit(s) | ATok::Var(s) | ATok::UnterminatedBrace(s) => s.clone(),
        ATok::DollarVar(name) => format!("${name}"),
        ATok::Subscript(name, index) => format!("${{{name}[{index}]}}"),
        ATok::BareSubscript(name, index) => format!("{name}[{index}]"),
        ATok::ParamExpansion(body) => format!("${{{body}}}"),
        ATok::PlusPlus => "++".to_string(),
        ATok::MinusMinus => "--".to_string(),
        ATok::StarStar => "**".to_string(),
        ATok::LtLt => "<<".to_string(),
        ATok::GtGt => ">>".to_string(),
        ATok::LtEq => "<=".to_string(),
        ATok::GtEq => ">=".to_string(),
        ATok::EqEq => "==".to_string(),
        ATok::BangEq => "!=".to_string(),
        ATok::AmpAmp => "&&".to_string(),
        ATok::PipePipe => "||".to_string(),
        ATok::PlusEq => "+=".to_string(),
        ATok::MinusEq => "-=".to_string(),
        ATok::StarEq => "*=".to_string(),
        ATok::SlashEq => "/=".to_string(),
        ATok::PercentEq => "%=".to_string(),
        ATok::AmpEq => "&=".to_string(),
        ATok::PipeEq => "|=".to_string(),
        ATok::CaretEq => "^=".to_string(),
        ATok::LtLtEq => "<<=".to_string(),
        ATok::GtGtEq => ">>=".to_string(),
        ATok::Plus => "+".to_string(),
        ATok::Minus => "-".to_string(),
        ATok::Star => "*".to_string(),
        ATok::Slash => "/".to_string(),
        ATok::Percent => "%".to_string(),
        ATok::Lt => "<".to_string(),
        ATok::Gt => ">".to_string(),
        ATok::Amp => "&".to_string(),
        ATok::Pipe => "|".to_string(),
        ATok::Caret => "^".to_string(),
        ATok::Tilde => "~".to_string(),
        ATok::Bang => "!".to_string(),
        ATok::Eq => "=".to_string(),
        ATok::Question => "?".to_string(),
        ATok::Colon => ":".to_string(),
        ATok::Comma => ",".to_string(),
        // DD#94: a newline or `;`. Both spell the same role; `;` is the one
        // that can be shown inline without mangling the message.
        ATok::Sep => ";".to_string(),
        ATok::LParen => "(".to_string(),
        ATok::RParen => ")".to_string(),
    }
}

fn tokenize_arith(input: &str) -> Vec<ATok> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        i = push_arith_token(input, bytes, i, &mut tokens);
    }
    tokens
}

/// One step of [`tokenize_arith`]: push at most one token and return the index
/// just past what was consumed.
fn push_arith_token(input: &str, bytes: &[u8], i: usize, tokens: &mut Vec<ATok>) -> usize {
    // DD#94: a newline or `;` is a sequence separator. Whether it actually
    // separates or merely continues the expression is settled in the parser —
    // an operand position skips separators (so `1 +` ⏎ `2` is one expression),
    // while a complete sub-expression ends at one.
    if bytes[i] == b'\n' || bytes[i] == b';' {
        tokens.push(ATok::Sep);
        return i + 1;
    }
    if bytes[i].is_ascii_whitespace() {
        return i + 1;
    }
    if let Some((tok, ni)) = next_arith_token(input, bytes, i) {
        tokens.push(tok);
        return ni;
    }
    // Unknown byte (DD#72): a hard error, not a silent skip. Emit a BadLit
    // spanning the byte so the parser rejects it (SB4035).
    let ch = input[i..].chars().next().unwrap_or('?');
    tokens.push(ATok::BadLit(ch.to_string()));
    i + ch.len_utf8()
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

/// Parse a COMPLETE DD#72 integer-literal string to its value. The shared
/// authority for the arith literal grammar used outside the tokenizer — the
/// type-check D4 `${s:-N}` arith-default validation and the arith `:-` default
/// lowering both call this so "what is a valid arith literal" is defined once.
///
/// Grammar (must match the whole trimmed string): optional leading `-`, then a
/// decimal integer (no leading zero except `0`) or a `0x`/`0o`/`0b`-prefixed
/// radix literal. Rejects (returns `None`) leading-zero octal (`010`), bash
/// `base#digits`, empty digit spans, interpolations, and anything non-literal.
pub fn parse_dd72_literal(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let mag = parse_dd72_unsigned(body)?;
    Some(if neg { mag.wrapping_neg() } else { mag })
}

fn parse_dd72_unsigned(body: &str) -> Option<i64> {
    let bytes = body.as_bytes();
    if let Some(radix) = radix_prefix(bytes) {
        let digits = &body[2..];
        if digits.is_empty() || !digits.bytes().all(|b| (b as char).is_digit(radix)) {
            return None;
        }
        return u64::from_str_radix(digits, radix).ok().map(|v| v as i64);
    }
    parse_dd72_decimal(body)
}

/// A plain DD#72 decimal magnitude: non-empty ASCII digits, no leading zero
/// except a lone `0` (leading-zero octal is rejected). `None` otherwise.
fn parse_dd72_decimal(body: &str) -> Option<i64> {
    let bytes = body.as_bytes();
    if body.is_empty() || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if body.len() > 1 && bytes[0] == b'0' {
        return None;
    }
    body.parse::<i64>().ok()
}

/// The radix for a `0x`/`0o`/`0b` prefix (case-insensitive), or `None`.
fn radix_prefix(bytes: &[u8]) -> Option<u32> {
    if bytes.len() >= 2 && bytes[0] == b'0' {
        return match bytes[1] {
            b'x' | b'X' => Some(16),
            b'o' | b'O' => Some(8),
            b'b' | b'B' => Some(2),
            _ => None,
        };
    }
    None
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
    // Note: a bash arbitrary-base literal `base#digits` (`16#ff`) never reaches
    // here as a contiguous run — `collect_arith_content` tokenizes the body with
    // the logos lexer (which emits `#` as its own token) and rejoins with spaces,
    // so `16#ff` arrives as `16 # ff`. The lone `#` is rejected as an unknown byte
    // by `tokenize_arith`'s fallback (SB4035). (DD#72 dropped base#.)
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
///
/// The end of the body comes from the *shared* balanced scanner, not from the
/// first `}`: a body may contain a nested `${ … }` of its own (`${a[${i}]}`),
/// and stopping early truncated it to `a[${i`, so the trailing `]` lexed as a
/// stray and the read died with SB4035 — while the three other spellings of
/// that same read all accepted it. An unbalanced body is an error token, never
/// a valid variable read with its missing closer silently supplied.
fn scan_arith_braced_var(input: &str, bytes: &[u8], start: usize) -> (ATok, usize) {
    match crate::parse::expr::balanced_close(bytes, start - 1) {
        Some(close) => (braced_body_token(&input[start..close]), close + 1),
        None => (
            ATok::UnterminatedBrace(input[start - 2..].to_string()),
            bytes.len(),
        ),
    }
}

/// Classify a `${…}` body inside `$(( ))`. A `${#…}` length body is retired
/// (DD#75 §4, amended): it is not recognized here, so it falls through to the
/// parameter-expansion path and the real parser rejects it (`$(len x)` /
/// `(len x)` is the sole length spelling). A bare `${name}` is a var read.
fn braced_body_token(body: &str) -> ATok {
    if let Some(sub) = try_braced_subscript(body) {
        return sub;
    }
    // A plain identifier stays a simple var read (the working path).
    if is_bare_ident(body) {
        return ATok::DollarVar(body.to_string());
    }
    // Anything else carries a parameter-expansion operator (`:-`, `:?`, `#`,
    // `%`, `:s/`, …) — route it through the real parser (DD#77) rather than the
    // old bogus `DollarVar(body)` → silent 0.
    ATok::ParamExpansion(body.to_string())
}

/// True for a non-empty run of `[A-Za-z0-9_]` — a bare variable name.
fn is_bare_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `${a[i]}` / `${m[k]}` — a normal-parse subscript rvalue. Split `name[index]`;
/// the parser builds the ordinary `Subscript` Expr, so the array-int-index vs
/// map-string-key distinction (DD#33) is handled by the normal lowering.
/// arith-subscript-values.
fn try_braced_subscript(body: &str) -> Option<ATok> {
    let open = body.find('[')?;
    let name = &body[..open];
    // The name must be a *variable* name, not merely a run of `[A-Za-z0-9_]`:
    // `${1[0]}` used to become a subscript on a collection called `1`, which
    // nothing declares, and codegen panicked ("ICE: undefined array: 1").
    // Falling through here routes the body to the real parser, which rejects
    // it with SB4047 (`try_subscript`, parse/expr.rs).
    if !crate::ir::is_plain_identifier(name) || !body.ends_with(']') {
        return None;
    }
    let index = &body[open + 1..body.len() - 1];
    if index.is_empty() {
        return None;
    }
    Some(ATok::Subscript(name.to_string(), index.to_string()))
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
    let name = input[start..j].to_string();
    if j < bytes.len() && bytes[j] == b'[' {
        if let Some(end) = scan_balanced_brackets(bytes, j) {
            let index = input[j + 1..end - 1].to_string();
            return (ATok::BareSubscript(name, index), end);
        }
    }
    (ATok::Var(name), j)
}

/// From the `[` at `open`, return the byte index just past the matching `]` for
/// the balanced `[ … ]` run (handles nesting like `a[b[0]]`). `None` if
/// unbalanced or empty (`a[]`).
fn scan_balanced_brackets(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (k, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => continue,
        }
        if depth == 0 {
            return (k > open + 1).then_some(k + 1);
        }
    }
    None
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
                format!(
                    "expected `{}` in arithmetic expression",
                    atok_to_text(expected)
                ),
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
        match lhs {
            ArithExpr::Var(name) => {
                let name = name.clone();
                self.advance();
                Some(ArithExpr::PostIncDec { var: name, inc })
            }
            // arith-subscript model B (Stage 2): `a[i]++` / `a[i]--` → a
            // subscript assign of `a[i] ± 1`.
            ArithExpr::Subscript { name, index } => {
                let (name, index) = (name.clone(), index.clone());
                self.advance();
                let op = if inc { IrArithOp::Add } else { IrArithOp::Sub };
                Some(ArithExpr::SubscriptAssign {
                    name,
                    index,
                    op: Some(op),
                    value: Box::new(ArithExpr::Literal(1)),
                })
            }
            _ => None,
        }
    }

    /// Handle assignment operators (`=`, `+=`, `-=`, ...). Returns the new
    /// LHS if an assignment was consumed; `None` if the next token isn't an
    /// assignment operator or LHS isn't an assignable (scalar var or bare
    /// subscript).
    fn try_assignment(&mut self, lhs: &ArithExpr) -> Result<Option<ArithExpr>, Diagnostic> {
        let Some(tok) = self.peek() else {
            return Ok(None);
        };
        let assign_op = match assign_op_for(tok) {
            Some(op) => op,
            None => return Ok(None),
        };
        match lhs {
            ArithExpr::Var(name) => {
                let name = name.clone();
                self.advance();
                let rhs = self.parse_expr(1)?; // right-associative
                Ok(Some(match assign_op {
                    None => ArithExpr::Assign {
                        var: name,
                        value: Box::new(rhs),
                    },
                    Some(op) => ArithExpr::CompoundAssign {
                        var: name,
                        op,
                        value: Box::new(rhs),
                    },
                }))
            }
            // arith-subscript model B (Stage 2): `a[i] = …` / `a[i] op= …`.
            ArithExpr::Subscript { name, index } => {
                let (name, index) = (name.clone(), index.clone());
                self.advance();
                let rhs = self.parse_expr(1)?;
                Ok(Some(ArithExpr::SubscriptAssign {
                    name,
                    index,
                    op: assign_op,
                    value: Box::new(rhs),
                }))
            }
            _ => Ok(None),
        }
    }

    /// DD#94: consume a run of *terminators* (`;` / newline).
    ///
    /// A terminator says "that one is done", so repeating it terminates
    /// nothing twice and a run of them is trivia — blank lines, and a
    /// newline swallowed by a pending operator. `,` is deliberately **not**
    /// included: it is a separator, which asserts an expression on each
    /// side, and an empty side is a contradiction (see `fold_sequence`).
    fn skip_terminators(&mut self) {
        while matches!(self.peek(), Some(ATok::Sep)) {
            self.advance();
        }
    }

    /// DD#94: fold a run of separator-delimited expressions into a sequence
    /// (value = last), generalising the old comma-only fold. A separator is
    /// *required* between expressions, so `(( 1 2 ))` stays the "unexpected
    /// token" error it is today rather than silently becoming a sequence. A
    /// trailing terminator (`(( a=1 ⏎ ))`) simply ends the block.
    ///
    /// `,` separates and `;` / newline terminate, and the two differ in how
    /// they treat emptiness: a run of terminators is trivia, while a comma
    /// owes a non-empty expression on its far side. So `(( a=1 ⏎ , b=2 ))`
    /// is fine — the newline is trivia and both sides of the comma are real
    /// — but `(( 1 ,, 2 ))` and `(( 1, ))` are errors, as in bash.
    /// Consume one element boundary, and report whether an expression follows.
    ///
    /// A run of terminators is trivia, so it ends the sequence if nothing
    /// comes after it. A comma is a separator and owes an expression on its
    /// far side even at the end of the block: `(( 1, ))` is not a trailing
    /// terminator, it is a missing operand, so this says an expression
    /// follows and lets the operand parser produce the diagnostic.
    fn take_separator(&mut self) -> bool {
        self.skip_terminators();
        if matches!(self.peek(), Some(ATok::Comma)) {
            self.advance();
            self.skip_terminators();
            return true;
        }
        self.peek().is_some()
    }

    fn fold_sequence(&mut self, first: ArithExpr) -> Result<ArithExpr, Diagnostic> {
        let mut exprs = vec![first];
        while matches!(self.peek(), Some(ATok::Sep) | Some(ATok::Comma)) {
            if !self.take_separator() {
                break; // trailing terminator
            }
            exprs.push(self.parse_expr(0)?);
        }
        Ok(if exprs.len() == 1 {
            exprs.pop().unwrap()
        } else {
            ArithExpr::Comma(exprs)
        })
    }

    fn parse_prefix(&mut self) -> Result<ArithExpr, Diagnostic> {
        // DD#94: an operand is still owed here — a binary operator or `=` is
        // pending — so a *terminator* is a line continuation, not a break.
        // A `,` is not: it separates, and `1 + , 2` is a missing operand.
        self.skip_terminators();
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

    /// The braced/subscript atom forms — `${#name}` length, `${a[i]}`/`${m[k]}`
    /// value-import, bare `a[i]` (model B), and `${var:OP…}` (DD#77). Returns
    /// `None` if `tok` is not one of these (the caller handles the rest). Split
    /// out of `parse_prefix_atom` to keep the dispatcher small.
    fn parse_expansion_atom(&mut self, tok: &ATok) -> Option<Result<ArithExpr, Diagnostic>> {
        match tok {
            ATok::Subscript(name, idx) => {
                // `${a[i]}` / `${m[k]}` — a normal-parse `Subscript` Expr imported
                // as an rvalue value (arith-subscript-values).
                self.advance();
                let index = crate::parse::expr::subscript_index_expr(idx, self.span);
                let expr = Expr {
                    kind: ExprKind::Subscript {
                        name: name.clone(),
                        index: Box::new(index),
                    },
                    span: self.span,
                };
                Some(Ok(ArithExpr::Expansion(Box::new(expr))))
            }
            ATok::BareSubscript(name, idx) => {
                // Model B (Stage 2): bare `a[i]` — the index is a full arith
                // expression, re-parsed here.
                self.advance();
                Some(
                    parse_arith_str(idx, self.span).map(|index| ArithExpr::Subscript {
                        name: name.clone(),
                        index: Box::new(index),
                    }),
                )
            }
            ATok::ParamExpansion(body) => {
                // `${var:OP…}` — parse the body with the real parameter-expansion
                // parser and import the result as an rvalue value (DD#77). The
                // arithmetic lexer rebuilt `body` from its tokens, so nothing
                // inside it maps back to a column: it reports the expression's.
                self.advance();
                let origin = super::lexer::Origin::Within(self.span.line, self.span.col);
                Some(
                    crate::parse::expr::parse_parameter_expansion(body, self.span, origin)
                        .map(|expr| ArithExpr::Expansion(Box::new(expr))),
                )
            }
            _ => None,
        }
    }

    /// A bare word in arithmetic that names something the compiler owns is a
    /// **nullary call**, not a variable read: `$(( getpid ))` is `(getpid)`,
    /// exactly as `$(( f ))` already calls a nullary `-> int` function.
    ///
    /// DD#125 is what makes this unambiguous — a compiler-owned name cannot
    /// be bound (SB5092), so a surviving *read* of one can only mean the
    /// intrinsic. Emitting the very node the sexpr form produces is the
    /// point: validate, `type_check` and the transform each see one shape,
    /// so a bare `echo` reports the same SB4030 as `(echo)` and a bare `len`
    /// the same SB4045 arity error. Teaching three passes to special-case
    /// `ArithExpr::Var` separately would have been three lists for one
    /// question, which is exactly how the sexpr denotation rule rotted.
    ///
    /// The lookahead is load-bearing. An assignment target or a `++`/`--`
    /// operand is not a read, and `try_assignment`/`try_postfix` match on
    /// `Var`; leaving those as `Var` keeps `(( getpid = 5 ))` on SB5092
    /// rather than a parse error about a stray `=`.
    fn bare_word_atom(&self, name: String) -> ArithExpr {
        let assignable = match self.peek() {
            Some(tok) => {
                assign_op_for(tok).is_some() || matches!(tok, ATok::PlusPlus | ATok::MinusMinus)
            }
            None => false,
        };
        if !assignable && crate::resolve::arith_denotation(&name).is_some() {
            return ArithExpr::FuncCall { name, args: vec![] };
        }
        ArithExpr::Var(name)
    }

    /// Atoms and operands: literals, variables, parenthesised forms. An
    /// out-of-range `BadNum` is rejected here (SB4026); the `-i64::MIN`
    /// case is folded earlier by the unary-neg peephole in `parse_unary`.
    fn parse_prefix_atom(&mut self) -> Result<ArithExpr, Diagnostic> {
        let tok = self.peek().cloned();
        if let Some(result) = tok.as_ref().and_then(|tok| self.parse_expansion_atom(tok)) {
            return result;
        }
        match tok {
            Some(ATok::Num(n)) => {
                self.advance();
                Ok(ArithExpr::Literal(n))
            }
            Some(ATok::Var(name)) => {
                self.advance();
                Ok(self.bare_word_atom(name))
            }
            Some(ATok::DollarVar(name)) => {
                self.advance();
                Ok(dollar_var_atom(name))
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
            Some(tok @ ATok::UnterminatedBrace(_)) => Err(Diagnostic::error_at(
                self.span.line,
                self.span.col,
                "SB4057",
                format!(
                    "unterminated `${{` in arithmetic: `{}` — expected `}}`",
                    atok_to_text(&tok)
                ),
            )),
            // An operand is owed here. Recovering silently to 0 turned a
            // malformed expression into a *wrong answer* rather than an error:
            // `$(( 1 +* 2 ))` became 0 and `$(( 1 + ))` became 1, both of which
            // bash rejects outright. Both cases are now a hard SB4020.
            Some(tok) => Err(arith_error(
                self.span,
                format!(
                    "expected an operand in arithmetic expression, found `{}`",
                    atok_to_text(&tok)
                ),
            )),
            None => Err(arith_error(
                self.span,
                "arithmetic expression ends where an operand is expected".to_string(),
            )),
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
    ///
    /// The subscript and parameter-expansion forms are operands and can
    /// never be read as a binary operator, so admitting them adds no
    /// grouping ambiguity: a real grouping cannot have a `Var` juxtaposed
    /// with a second operand. Leaving them out made SB4035 recommend
    /// `${a[i]}` and then SB4020 reject it inside `(strlen …)`.
    fn is_func_arg_start(&self) -> bool {
        matches!(
            self.peek(),
            Some(
                ATok::Num(_)
                    | ATok::Var(_)
                    | ATok::DollarVar(_)
                    | ATok::LParen
                    | ATok::Subscript(_, _)
                    | ATok::BareSubscript(_, _)
                    | ATok::ParamExpansion(_)
            )
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
/// The `name[index] = <arith>` statement every mutation form lowers to. All
/// four spellings — `v = e`, `v op= e`, `v++`, `a[i] op= e` — differ only in
/// the name, the optional index, and the right-hand expression.
fn arith_assign_stmt(name: &str, index: Option<Expr>, value: ArithExpr, span: Span) -> Statement {
    Statement {
        kind: StmtKind::Assignment(Assignment {
            name: name.to_string(),
            index,
            op: AssignOp::Set,
            value: Expr {
                kind: ExprKind::Arithmetic(Box::new(value)),
                span,
            },
        }),
        span,
    }
}

/// `v op= e` and `v++` are both a read of the variable combined with an
/// operand, so they share one shape.
fn binop_on_var(var: &str, op: IrArithOp, right: Box<ArithExpr>) -> ArithExpr {
    ArithExpr::BinOp {
        left: Box::new(ArithExpr::Var(var.to_string())),
        op,
        right,
    }
}

/// The right-hand side of a subscript mutation: the bare value for `a[i] = v`,
/// a read-modify against the `a[i]` cell for `a[i] op= v`.
fn subscript_assign_rhs(
    name: &str,
    index: &ArithExpr,
    op: Option<IrArithOp>,
    value: &ArithExpr,
) -> ArithExpr {
    match op {
        None => value.clone(),
        Some(o) => ArithExpr::BinOp {
            left: Box::new(ArithExpr::Subscript {
                name: name.to_string(),
                index: Box::new(index.clone()),
            }),
            op: o,
            right: Box::new(value.clone()),
        },
    }
}

/// Lower a mutating arithmetic expression to a Statement.
///
/// **Total over `ArithExpr` on purpose — do not add a wildcard.**
/// `lower_arith_command` routes only `ArithKind::Mutation` here, and
/// `classify_arith_expr` answers `Mutation` for exactly the six variants
/// handled below. Naming the other ten makes a *new* `ArithExpr` variant a
/// compile error here rather than a silent one: the arm used to be `_ =>`
/// lowering any unexpected shape to `ArithTest`, which evaluates its operand
/// and discards it — a mutation that quietly stopped mutating.
fn lower_arith_mutation_structured(expr: &ArithExpr, span: Span) -> Statement {
    match expr {
        ArithExpr::Assign { var, value } => arith_assign_stmt(var, None, (**value).clone(), span),
        ArithExpr::CompoundAssign { var, op, value } => {
            arith_assign_stmt(var, None, binop_on_var(var, *op, value.clone()), span)
        }
        ArithExpr::PreIncDec { .. } => unreachable!("{}", crate::ir::PREFIX_INCDEC_INVARIANT),
        ArithExpr::PostIncDec { var, inc } => {
            let op = if *inc { IrArithOp::Add } else { IrArithOp::Sub };
            let step = Box::new(ArithExpr::Literal(1));
            arith_assign_stmt(var, None, binop_on_var(var, op, step), span)
        }
        // arith-subscript model B (Stage 2): `a[i] = v`, `a[i] op= v`,
        // `a[i]++`/`--`. Reuse the statement subscript-assign write path — the
        // index and the computed RHS are arithmetic exprs; the read side (for
        // `op=`/`++`) is the bare `a[i]` cell read (`ArithExpr::Subscript`).
        ArithExpr::SubscriptAssign {
            name,
            index,
            op,
            value,
        } => {
            let index_expr = Expr {
                kind: ExprKind::Arithmetic(index.clone()),
                span,
            };
            let rhs = subscript_assign_rhs(name, index, *op, value);
            arith_assign_stmt(name, Some(index_expr), rhs, span)
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
        ArithExpr::Literal(_)
        | ArithExpr::Var(_)
        | ArithExpr::DollarVar(_)
        | ArithExpr::BinOp { .. }
        | ArithExpr::UnaryOp { .. }
        | ArithExpr::Ternary { .. }
        | ArithExpr::FuncCall { .. }
        | ArithExpr::Paren(_)
        | ArithExpr::Expansion(_)
        | ArithExpr::Subscript { .. } => unreachable!(
            "classify_arith_expr answers Mutation only for Assign, CompoundAssign, \
             PostIncDec, PreIncDec, SubscriptAssign and Comma, and lower_arith_command \
             routes only Mutation here: {expr:?}"
        ),
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
pub fn collect_arith_content_raw(scanner: &mut Scanner, span: Span) -> Result<String, Diagnostic> {
    collect_arith_content(scanner, span)
}

/// Collect raw content between the opening (( or $(( and closing )).
/// Respects nesting of (( )) and ( ).
fn collect_arith_content(scanner: &mut Scanner, span: Span) -> Result<String, Diagnostic> {
    let collected = scan_arith_content(scanner);
    if !collected.closed {
        return Err(arith_error(
            span,
            "unterminated arithmetic — expected `))`. Note a `#` comment runs to \
             the end of its line, so a `))` after one is part of the comment"
                .to_string(),
        ));
    }
    Ok(collected.content)
}

pub(super) struct CollectedArith {
    pub content: String,
    pub closed: bool,
}

/// Shared by ordinary arithmetic and the scanner's double-quoted interpolation.
/// Closing parentheses stay individual: in `)))`, the first may close a group
/// and the remaining two close the arithmetic construct.
/// Preserve physical token adjacency: a shell token boundary can split `!=`.
pub(super) fn scan_arith_content(scanner: &mut Scanner) -> CollectedArith {
    let mut content = String::new();
    let mut depth = 2i32;

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

    CollectedArith {
        content,
        closed: depth == 0,
    }
}

/// One iteration of collect_arith_content. Mutates `content` and `depth`;
/// returns true when the outer `))` delimiter has been consumed.
fn step_arith_collect(
    scanner: &mut Scanner,
    tok: &Tok,
    content: &mut String,
    depth: &mut i32,
) -> bool {
    match tok {
        Tok::DblParen => step_open_paren(scanner, content, depth, "(("),
        Tok::DollarDblParen => step_open_paren(scanner, content, depth, "$(("),
        Tok::LParen => {
            *depth += 1;
            content.push('(');
            scanner.next();
            false
        }
        Tok::RParen => step_rparen(scanner, content, depth),
        _ => step_default_token(scanner, tok, content),
    }
}

/// Opening `((` or `$((` — increment depth, append the text.
fn step_open_paren(
    scanner: &mut Scanner,
    content: &mut String,
    depth: &mut i32,
    text: &str,
) -> bool {
    *depth += 2;
    content.push_str(text);
    scanner.next();
    false
}

/// Close an inner group, or consume the two adjacent outer closes.
/// An unmatched single close stays in the text for the arithmetic diagnostic.
fn step_rparen(scanner: &mut Scanner, content: &mut String, depth: &mut i32) -> bool {
    scanner.next();
    if *depth > 2 {
        *depth -= 1;
    } else if scanner.next_is_adjacent() && matches!(scanner.peek().token, Tok::RParen) {
        scanner.next();
        *depth = 0;
        return true;
    }
    content.push(')');
    false
}

fn step_default_token(scanner: &mut Scanner, tok: &Tok, content: &mut String) -> bool {
    let text = tok_to_text(tok);
    content.push_str(&text);
    scanner.next();
    if !scanner.next_is_adjacent() && needs_separator_space(&scanner.peek().token, &text) {
        content.push(' ');
    }
    false
}

fn needs_separator_space(next: &Tok, prev_text: &str) -> bool {
    !matches!(
        next,
        Tok::RParen | Tok::Semi | Tok::Newline | Tok::Eof | Tok::DblParen | Tok::LParen
    ) && !prev_text.ends_with(' ')
}

/// Convert a token back to source text for arithmetic content collection.
///
/// The match is deliberately exhaustive (DD#116, totality). It used to end in
/// `_ => String::new()`, which silently *deleted* 35 of the 59 token kinds
/// from the collected text: the re-lexer never saw them, so `(( x = 1 done ))`
/// collapsed to `x = 1` and compiled, where bash reports a syntax error. A new
/// `Tok` variant must be a compile error here, not another silent drop.
///
/// Only two renderings are empty, and both are deliberate: a comment is
/// trivia (DD#94 — comments are trivia inside `(( ))`, a deliberate
/// divergence from bash), and `Eof` has no source text. Neither token
/// actually reaches here today — the scanner strips comments and the collect
/// loop stops at EOF — but both arms state the *specified* rule rather than
/// the shape the current scanner happens to deliver (DD#116, property 3).
///
/// `DoubleQuoted` is rendered as the placeholder `"..."` rather than its
/// parts. That is lossy but safe: `"` is never valid in sash arithmetic, so
/// the shape is always rejected, and SB4035 names the real `"` the user typed
/// rather than the fabricated ellipsis. Rendering the parts faithfully would
/// mean re-entering the display-vs-semantic trap that made `argv "1" a` ICE.
///
/// **Total over `Tok` on purpose — do not add a wildcard.**
fn tok_to_text(tok: &Tok) -> String {
    match tok {
        // Keywords. The spellings live in `keyword_to_str`, the single
        // reserved-word list. This arm enumerates the variants so a new
        // keyword is still a compile error here, but it must not restate the
        // text: the one time that list was duplicated the two copies drifted,
        // `Tok::Str` went missing from both, and the bare word `str` failed to
        // lower in every word context (journal 347).
        Tok::If
        | Tok::Then
        | Tok::Else
        | Tok::Elif
        | Tok::Fi
        | Tok::While
        | Tok::Until
        | Tok::Do
        | Tok::Done
        | Tok::For
        | Tok::In
        | Tok::Case
        | Tok::Esac
        | Tok::Fn
        | Tok::Function
        | Tok::Str
        | Tok::Int
        | Tok::Const
        | Tok::Array
        | Tok::Map
        | Tok::Attest
        | Tok::End => keyword_to_str(tok)
            .expect("every variant in this arm is in keyword_to_str's domain")
            .to_string(),
        // Operators and punctuation.
        Tok::Pipe => "|".to_string(),
        Tok::AndAnd => "&&".to_string(),
        Tok::OrOr => "||".to_string(),
        Tok::Amp => "&".to_string(),
        Tok::Semi => ";".to_string(),
        // DD#94: newlines are separators inside `(( ))`, so the raw
        // collector must preserve them for the arith re-lexer.
        Tok::Newline => "\n".to_string(),
        Tok::LBrace => "{".to_string(),
        Tok::RBrace => "}".to_string(),
        // Parens never arrive here. `step_arith_collect` intercepts all four
        // *before* `step_default_token` (the sole caller of this function),
        // because rendering them is depth-sensitive: `))` emits nothing when
        // it closes the expression, rather than being content.
        // A plain spelling here would be a second,
        // already-disagreeing answer to that question (DD#116, rival). The
        // arms stay so a new token is still a compile error (property 2).
        Tok::LParen | Tok::RParen | Tok::DollarDblParen | Tok::DblParen => {
            unreachable!("paren tokens are handled by step_arith_collect, not tok_to_text")
        }
        Tok::Bang => "!".to_string(),
        Tok::Dash => "-".to_string(),
        Tok::DblBracketOpen => "[[".to_string(),
        Tok::DblBracketClose => "]]".to_string(),
        // Assignment.
        Tok::Eq => "=".to_string(),
        Tok::PlusAssign => "+=".to_string(),
        // Redirects.
        Tok::HereString => "<<<".to_string(),
        Tok::DblGt => ">>".to_string(),
        Tok::GtAmp => ">&".to_string(),
        Tok::LtAmp => "<&".to_string(),
        Tok::LtGt => "<>".to_string(),
        Tok::AmpGt => "&>".to_string(),
        Tok::Gt => ">".to_string(),
        Tok::Lt => "<".to_string(),
        // Terminators. `for (( ;; ))` reaches the arith parser through this
        // arm, as a run of bare separators.
        Tok::DblSemi => ";;".to_string(),
        // Literals and values.
        Tok::Word(w) => w.clone(),
        Tok::SingleQuoted(s) => format!("'{}'", s),
        Tok::DoubleQuoted(_) => "\"...\"".to_string(),
        Tok::AnsiCQuoted(s) => format!("$'{}'", s),
        // Expansions.
        Tok::Dollar(v) => format!("${}", v),
        Tok::DollarBrace(c) => format!("${{{}}}", c),
        Tok::DollarParen => "$(".to_string(),
        // Deliberately empty: trivia, and end-of-input.
        Tok::Comment(_) => String::new(),
        Tok::Eof => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_text_for_trivia_and_end_is_empty() {
        assert_eq!(tok_to_text(&Tok::Comment("not expression text".into())), "");
        assert_eq!(tok_to_text(&Tok::Eof), "");
    }

    // The scanner's deferred lexical error masks this raw API's result at the CLI.
    #[test]
    fn raw_arithmetic_rejects_an_unterminated_braced_operand() {
        for input in ["${x", "1 + ${x", "${x:-${y}"] {
            let diagnostic = parse_arith_str(input, Span::new(7, 11)).unwrap_err();
            assert_eq!(diagnostic.code, "SB4057");
            assert!(diagnostic
                .message
                .contains("unterminated `${` in arithmetic:"));
            assert!(diagnostic.message.contains("expected `}`"));
            assert_eq!((diagnostic.line, diagnostic.col), (7, 11));
        }
    }

    /// `++x`. `reject_prefix_incdec` raises SB4020 at both arith entry
    /// points, so `ArithExpr::PreIncDec` never reaches a later pass. These
    /// call the two accessors in this file directly with the node the parser
    /// would never hand them. A test that stops panicking means the parser
    /// has a hole.
    fn prefix_incdec() -> ArithExpr {
        ArithExpr::PreIncDec {
            var: "x".into(),
            inc: true,
        }
    }

    #[test]
    #[should_panic(expected = "prefix ++/-- never survives the parser")]
    fn classify_arith_expr_rejects_prefix_incdec() {
        let _ = classify_arith_expr(&prefix_incdec());
    }

    #[test]
    #[should_panic(expected = "prefix ++/-- never survives the parser")]
    fn lower_arith_mutation_structured_rejects_prefix_incdec() {
        let _ = lower_arith_mutation_structured(&prefix_incdec(), Span::new(1, 1));
    }

    /// `lower_arith_command` routes only `ArithKind::Mutation` into
    /// `lower_arith_mutation_structured`. These are the non-mutation shapes:
    /// `classify_arith_expr` answers `Comparison` or `Test` for every one, so
    /// none can arrive. The arm used to be `_ =>`, silently lowering them to
    /// `ArithTest` — a mutation that evaluates its operand and discards it. A
    /// test that stops panicking means the classifier and the lowering have
    /// drifted apart.
    #[test]
    fn lower_arith_mutation_rejects_every_non_mutation_shape() {
        let shapes = [
            ArithExpr::Literal(1),
            ArithExpr::Var("x".into()),
            ArithExpr::DollarVar("x".into()),
            ArithExpr::BinOp {
                left: Box::new(ArithExpr::Literal(1)),
                op: IrArithOp::Add,
                right: Box::new(ArithExpr::Literal(2)),
            },
            ArithExpr::Paren(Box::new(ArithExpr::Literal(1))),
            ArithExpr::Subscript {
                name: "a".into(),
                index: Box::new(ArithExpr::Literal(0)),
            },
            ArithExpr::FuncCall {
                name: "f".into(),
                args: vec![],
            },
            ArithExpr::Ternary {
                cond: Box::new(ArithExpr::Literal(1)),
                true_val: Box::new(ArithExpr::Literal(2)),
                false_val: Box::new(ArithExpr::Literal(3)),
            },
        ];
        for shape in shapes {
            assert_ne!(
                classify_arith_expr(&shape),
                ArithKind::Mutation,
                "{shape:?} must not classify as a mutation"
            );
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                lower_arith_mutation_structured(&shape, Span::new(1, 1))
            }));
            let err = caught.expect_err("a non-mutation shape must ICE here");
            let msg = err
                .downcast_ref::<&'static str>()
                .map(|s| (*s).to_string())
                .or_else(|| err.downcast_ref::<String>().cloned())
                .expect("ICE payload is a string");
            assert!(
                msg.contains("classify_arith_expr answers Mutation only"),
                "ICE must name the invariant that guarantees it, got: {msg}"
            );
        }
    }

    /// `step_arith_collect` intercepts all four paren tokens before
    /// `step_default_token` — the sole caller of `tok_to_text` — because
    /// rendering them is depth-sensitive. A spelling here would be a second,
    /// already-disagreeing answer (DD#116, rival).
    #[test]
    fn tok_to_text_rejects_every_paren_token() {
        for tok in [Tok::LParen, Tok::RParen, Tok::DollarDblParen, Tok::DblParen] {
            let caught = std::panic::catch_unwind(|| tok_to_text(&tok));
            assert!(caught.is_err(), "{tok:?} should be unreachable here");
        }
    }

    /// The operand and prefix-operator spellings, which neither caller of
    /// `atok_to_text` can currently reach.
    ///
    /// `expect` renders the token it *wanted*, and it is only ever called with
    /// `Colon` or `RParen`. The other caller is `parse_prefix_atom`'s
    /// fallthrough, which sees only what no earlier arm took: `parse_prefix`
    /// consumes the six prefix operators, `skip_terminators` eats every `Sep`,
    /// `parse_expansion_atom` takes the three expansion forms, and the match's
    /// own arms take `Num`/`Var`/`DollarVar`/`LParen`/`BadNum`/`BadLit`.
    ///
    /// That is a property of these two callers, not an invariant any pass
    /// enforces, so these arms are *correct and unexercised* rather than
    /// unreachable by construction — a third caller would need every one. The
    /// spellings are pinned here instead of being deleted or made to ICE.
    ///
    /// The `{:?}` assertion is the one this function exists for: SB4020 used
    /// to format the token with `Debug` and tell people
    /// `found Amp` for a `&` they had typed.
    #[test]
    fn atok_to_text_spells_the_operand_and_prefix_tokens() {
        let cases: [(ATok, &str); 16] = [
            (ATok::Num(42), "42"),
            (
                ATok::BadNum("99999999999999999999".into()),
                "99999999999999999999",
            ),
            (ATok::BadLit("08".into()), "08"),
            (ATok::Var("x".into()), "x"),
            (ATok::DollarVar("x".into()), "$x"),
            (ATok::Subscript("a".into(), "0".into()), "${a[0]}"),
            (ATok::BareSubscript("a".into(), "0".into()), "a[0]"),
            (ATok::ParamExpansion("x:-7".into()), "${x:-7}"),
            (ATok::PlusPlus, "++"),
            (ATok::MinusMinus, "--"),
            (ATok::Plus, "+"),
            (ATok::Minus, "-"),
            (ATok::Tilde, "~"),
            (ATok::Bang, "!"),
            (ATok::Sep, ";"),
            (ATok::LParen, "("),
        ];
        for (tok, want) in cases {
            let got = atok_to_text(&tok);
            assert_eq!(got, want, "{tok:?} must render as its source spelling");
            assert_ne!(
                got,
                format!("{tok:?}"),
                "a diagnostic must never leak the Rust variant name"
            );
        }
    }
}
