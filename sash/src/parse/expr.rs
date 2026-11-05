//! Expression parsing: words, strings, parameter expansions, command substitution.
//!
//! This module handles everything that produces a *value* in sash:
//!   - Literals, single-quoted, double-quoted (with interpolation)
//!   - $var, ${var...} parameter expansion (all operators: #, %, /, :, etc.)
//!   - $(...) command substitution, $((...)) arithmetic expansion
//!   - Array literals: =(a b c) and =([k]=v)
//!   - Concatenation of adjacent tokens (no whitespace gap)
//!
//! SECURITY: Parameter expansion operators are parsed structurally here.
//! The `!` (indirect) and `@X` (transform) operators are flagged for
//! rejection by the post-parse `check_rejected()` pass in parser.rs.

use super::error::parse_error;
use super::lexer::{Origin, Scanner, StringPart, Tok};
use crate::diagnostic::Diagnostic;
use crate::ir::*;

// ── Top-level expression parsing ───────────────────────────────────

/// Single/double/ansi-c quoted arms of `parse_expr`, factored out to keep
/// the main word loop under the complexity gate.
fn parse_quoted_arm(
    scanner: &mut Scanner,
    tok: &Tok,
    span: Span,
    parts: &mut Vec<Expr>,
) -> Result<(), Diagnostic> {
    scanner.next();
    let kind = match tok {
        Tok::SingleQuoted(s) => ExprKind::SingleQuoted(s.clone()),
        Tok::DoubleQuoted(parts_tok) => {
            ExprKind::DoubleQuoted(string_parts_to_exprs(parts_tok, span, scanner)?)
        }
        Tok::AnsiCQuoted(s) => ExprKind::Literal(s.clone()),
        _ => unreachable!("parse_quoted_arm called with non-quoted token"),
    };
    parts.push(Expr { kind, span });
    Ok(())
}

/// `$`-expansion and flat-backtick arms of `parse_expr`, factored out to
/// keep the main word loop under the complexity gate. `tok` is one of
/// `Dollar`/`DollarBrace`/`DollarParen`/`DollarDblParen`.
fn parse_expansion_arm(
    scanner: &mut Scanner,
    tok: &Tok,
    span: Span,
    start: usize,
    parts: &mut Vec<Expr>,
) -> Result<(), Diagnostic> {
    match tok {
        Tok::Dollar(name) => parse_dollar_arm(scanner, name.clone(), span, start, parts),
        Tok::DollarBrace(content) => {
            // `${…}` content starts past the `${`; a brace-less desugar
            // (`$a[K]`, `$x:-w`) copies the source from just past the `$`.
            let opener = if scanner.byte_at(start + 1) == Some(b'{') {
                2
            } else {
                1
            };
            let origin = scanner.origin_of(start + opener);
            scanner.next();
            parts.push(parse_parameter_expansion(content, span, origin)?);
        }
        Tok::DollarParen => {
            scanner.next();
            parts.push(parse_command_substitution(scanner, span)?);
        }
        Tok::DollarDblParen => {
            scanner.next();
            parts.push(super::arith::parse_arith_expansion(scanner, span)?);
        }
        _ => unreachable!("parse_expansion_arm called with non-expansion token"),
    }
    Ok(())
}

/// Parse a single word/expression that produces a value.
/// This handles: literals, $var, ${var...}, "...", '...', $(...), $((...)),
/// concatenation of adjacent tokens, and array literals.
pub fn parse_expr(scanner: &mut Scanner) -> Result<Expr, Diagnostic> {
    parse_word_parts(scanner, AbsorbMode::None)
}

/// DD#71: parse an *operand word* — like `parse_expr`, but regex/shell
/// metacharacters (`( ) { } | !`) are absorbed as ordinary literal
/// characters instead of terminating the word. Used for the `=~` RHS (and
/// dispatch patterns): the operand runs to the next unquoted whitespace /
/// `]]` / `&&` / `||`, with quotes and `$`-expansions honoured normally, so
/// a bare `^(a|b)$` is one word and `'a b'` preserves its space.
pub fn parse_operand_word(scanner: &mut Scanner) -> Result<Expr, Diagnostic> {
    parse_word_parts(scanner, AbsorbMode::Operand)
}

/// DD#71: parse an *assignment value word* — a value binding is not a
/// connectable command, so the command connectors `& && || |` (and the
/// operand puncts) are absorbed as literal characters. The value runs to the
/// next unquoted whitespace / `;` / newline (which still sequence statements),
/// so `a=foo|cat` is the literal `foo|cat` and `a=foo&&bar` is `foo&&bar`.
pub fn parse_assignment_value(scanner: &mut Scanner) -> Result<Expr, Diagnostic> {
    parse_word_parts(scanner, AbsorbMode::Assign)
}

/// How punctuation/operator tokens are treated while building a word.
#[derive(Clone, Copy, PartialEq)]
enum AbsorbMode {
    /// Normal shell word — operators terminate the word (command args).
    None,
    /// `[[ ]]` operand / dispatch pattern — regex metachars are literal.
    Operand,
    /// Assignment value — operand puncts *and* command connectors are literal.
    Assign,
}

/// The literal text a token contributes to a word under `mode`, or `None`
/// if the token terminates the word.
fn absorb_text(tok: &Tok, mode: AbsorbMode) -> Option<&'static str> {
    match mode {
        AbsorbMode::None => None,
        AbsorbMode::Operand | AbsorbMode::Assign => match tok {
            Tok::LParen => Some("("),
            Tok::RParen => Some(")"),
            Tok::LBrace => Some("{"),
            Tok::RBrace => Some("}"),
            Tok::Pipe => Some("|"),
            Tok::Bang => Some("!"),
            // Command connectors are inert after an assignment value only.
            Tok::Amp if mode == AbsorbMode::Assign => Some("&"),
            Tok::AndAnd if mode == AbsorbMode::Assign => Some("&&"),
            Tok::OrOr if mode == AbsorbMode::Assign => Some("||"),
            _ => None,
        },
    }
}

fn parse_word_parts(scanner: &mut Scanner, mode: AbsorbMode) -> Result<Expr, Diagnostic> {
    let mut parts: Vec<Expr> = Vec::new();
    // Captured before the loop for the empty-`parts` error: the loop's own
    // `span` is scoped to an iteration that, in exactly that case, never ran.
    let word_start = scanner.peek().span.start;
    let word_span = span_from(scanner, word_start);

    // Concatenation continues only while the next token is adjacent (no
    // whitespace); the first part has nothing to be adjacent to.
    while parts.is_empty() || scanner.next_is_adjacent() {
        let start = scanner.peek().span.start;
        let tok = scanner.peek().token.clone();
        let span = span_from(scanner, start);
        if !absorb_one_part(scanner, &tok, span, start, mode, &mut parts)? {
            break;
        }
    }

    parts_to_expr(parts, word_span)
}

/// Consume the one token `tok` contributes to the word, pushing its part(s)
/// onto `parts`. Returns `false` when the token does not belong to the word,
/// which ends the concatenation.
fn absorb_one_part(
    scanner: &mut Scanner,
    tok: &Tok,
    span: Span,
    start: usize,
    mode: AbsorbMode,
    parts: &mut Vec<Expr>,
) -> Result<bool, Diagnostic> {
    match tok {
        Tok::Word(w) => {
            let w = w.clone();
            parts.push(parse_word_arm(scanner, w, span)?);
        }
        Tok::SingleQuoted(_) | Tok::DoubleQuoted(_) | Tok::AnsiCQuoted(_) => {
            parse_quoted_arm(scanner, tok, span, parts)?;
        }
        Tok::Dollar(_) | Tok::DollarBrace(_) | Tok::DollarParen | Tok::DollarDblParen => {
            parse_expansion_arm(scanner, tok, span, start, parts)?;
        }
        // A `$( )` closer beats DD#71 absorb, exactly as `]]` does for a
        // `[[ ]]` operand. DD#71 scopes absorb "to whitespace / closer", and
        // inside a substitution body `)` *is* the closer. Without this
        // `$(int x=1)` absorbed its own terminator into the value word and
        // the body ran to end of input, reported as SB4057 "unterminated `$(`"
        // with the `)` plainly there.
        Tok::RParen if scanner.in_cmd_sub() => return Ok(false),
        _ => match literal_token_text(tok, mode) {
            Some(text) => {
                scanner.next();
                parts.push(Expr {
                    kind: ExprKind::Literal(text),
                    span,
                });
            }
            None => return Ok(false),
        },
    }
    Ok(true)
}

/// Text of a token that lowers to a literal word part: `=`, `-`, the
/// mode-dependent absorbable metacharacters, and reserved words used as
/// bare arguments. `None` for tokens handled by their own match arm.
///
/// The reserved-word set lives in `keyword_to_str` alone. This function
/// carried a duplicate of it until 2026-09-16, and the copy was one token
/// short: `Tok::Str` was in neither, so `echo str` failed to parse.
fn literal_token_text(tok: &Tok, mode: AbsorbMode) -> Option<String> {
    match tok {
        Tok::Eq => Some("=".to_string()),
        Tok::Dash => Some("-".to_string()),
        _ => keyword_to_str(tok)
            .or_else(|| absorb_text(tok, mode))
            .map(str::to_string),
    }
}

/// `Tok::Word` arm — handles tilde expansion (`~`, `~/path`), the
/// `@arr` array-values shorthand, and bare literal words. Consumes
/// the token before returning.
fn parse_word_arm(scanner: &mut Scanner, w: String, span: Span) -> Result<Expr, Diagnostic> {
    scanner.next();
    if w == "~" || w.starts_with("~/") {
        // DD#53: no home-directory magic. An unquoted leading `~` has no
        // clean compile-time meaning; reject it with a focused message
        // (not the misleading $HOME SB5018) directing to the explicit form.
        return Err(parse_error(
            span,
            "SB4036",
            "unquoted leading `~` is not expanded — use $(getenv HOME) (with \
             a attest), or quote it (`'~'` / `\"~\"`) for a literal tilde"
                .to_string(),
        ));
    }
    if w.starts_with("@{") {
        return parse_at_brace_disposition(&w, span);
    }
    if w.starts_with('@') && w.len() > 1 {
        return Ok(Expr::param(&w[1..], Some("arrvals"), None, span));
    }
    Ok(Expr {
        kind: ExprKind::Literal(w),
        span,
    })
}

/// DD#69/#84: `@{a:-WORD}` / `@{a:?WORD}` / `@{a:~}` external-splat disposition.
/// The array splats per-element when non-empty; when empty, `:-` substitutes
/// WORD as a single argument (bare `@{a:-}` → zero args), `:?` aborts at runtime
/// with WORD, and `:~` (elide) drops the splat entirely (zero args) — the
/// explicit spelling of the empty→nothing behaviour a bare `@{a:-}` / `@a`
/// already has (the array analog of the scalar `${x:~}`, DD#84). WORD is a full
/// interpolating word (like the scalar `${x:-WORD}` default); `:~` takes no
/// word. `:=`/`:+` are invalid. Encoded as a `ParameterExpansion` with operator
/// `arrvals_default` / `arrvals_abort` and the WORD as its operand (`:~` ≡ a
/// wordless `arrvals_default`).
pub(crate) fn parse_at_brace_disposition(w: &str, span: Span) -> Result<Expr, Diagnostic> {
    // Both halves are guaranteed by the layers above: `parse_word_arm` only
    // calls this on a word that `starts_with("@{")`, and the lexer's `@{…}`
    // scan consumes through the matching `}` or fatals with SB4057 — so a word
    // that opens `@{` and reaches the parser always closes. A miss means those
    // layers have drifted, which is a compiler bug, so it is a loud ICE rather
    // than the SB5033 a malformed *program* gets (DG, 2026-09-19).
    let inner = w
        .strip_prefix("@{")
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or_else(|| {
            panic!("ICE: parse_at_brace_disposition on a word that is not a closed `@{{…}}`: {w:?}")
        });
    let colon = inner
        .find(':')
        .ok_or_else(|| splat_disposition_malformed(span))?;
    let name = &inner[..colon];
    let after = &inner[colon + 1..];
    let (op, operand) = parse_splat_op(after, name, span)?;
    if !is_identifier(name) {
        return Err(splat_disposition_malformed(span));
    }
    Ok(Expr {
        kind: ExprKind::ParameterExpansion(ParameterExpansion {
            name: name.to_string(),
            operator: Some(op.to_string()),
            operand,
            index: None,
            taint_off: after.starts_with('!'),
        }),
        span,
    })
}

/// The SB5033 "malformed `@{…}` disposition" diagnostic.
fn splat_disposition_malformed(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB5033",
        "`@{…}` splat disposition must be `@{a:-WORD}` (empty → the single \
         arg WORD; bare `@{a:-}` → one empty arg), `@{a:?WORD}` (empty → abort \
         with WORD), or `@{a:~}` (empty → drop the splat) — only `:-`, `:?`, \
         and `:~` are valid"
            .to_string(),
    )
}

/// Parse the `OP[WORD]` after the `:` in `@{name:OP…}` → (operator, operand).
/// The operand encodes the empty-array behaviour (has_word = operand.is_some()):
///
/// - `:-` → `arrvals_default`, **always an operand** — one element on empty:
///   WORD, or an empty element for bare `@{a:-}` (matches scalar `${x:-}` and
///   bash `"${a[@]:-}"`; DD#69/#84 addendum 2026-09-04).
/// - `:?` → `arrvals_abort`; bare `@{a:?}` aborts with no message (operand
///   `None`), `@{a:?WORD}` with WORD.
/// - `:~` (DD#84 elide) → `arrvals_default` with **no operand** — the sole
///   zero-args disposition: on an empty array the splat contributes nothing.
///   Like the scalar `${x:~}` it takes NO word, so `@{a:~WORD}` is an error.
fn parse_splat_op(
    after: &str,
    name: &str,
    span: Span,
) -> Result<(&'static str, Option<Box<Expr>>), Diagnostic> {
    if let Some(rest) = after.strip_prefix('-') {
        return Ok((
            "arrvals_default",
            Some(parse_disposition_literal(rest, span)?),
        ));
    }
    if let Some(rest) = after.strip_prefix('?') {
        return Ok(("arrvals_abort", optional_splat_word(rest, span)?));
    }
    if let Some(rest) = after.strip_prefix('~') {
        return parse_elide_splat_op(rest, name, span);
    }
    if let Some(rest) = after.strip_prefix('!') {
        // DD#91 addendum: `!` is a MODIFIER, not a disposition. A splat site
        // must still state its emptiness behaviour (DD#69), so `@{a:!}` alone
        // is bare and trips SB5032 exactly as `@a` does — the old coupling
        // silently supplied an elide the author never wrote.
        return parse_trusted_splat_op(rest, name, span);
    }
    Err(splat_disposition_malformed(span))
}

/// A **disposition default** operand — `${x:-D}`, `${x:?MSG}`, `@{a:-D}`,
/// `@{a:?MSG}` — is a plain literal, never an interpolating word.
///
/// DD#78 calls these operands disposition defaults and constrains them to be
/// clean and side-effect-free. That constraint used to be *checked*: the
/// operand was a full interpolating word, and a pass had to establish that
/// whatever it expanded to was safe. It is now established **by construction**
/// — there is nothing to expand.
///
/// The `:?` half came first (2026-09-30). An interpolating message admits
/// `$( )` and `$(( … ))`, and both are user-function call sites (DD#58's arith
/// sexpr calls a `-> int` fn), so `${y:?$(g)}` forked and exec'd — and did so
/// even when the subject was set and NO abort occurred, because the operand is
/// materialized eagerly. Arbitrary user code on the success path of a failsafe.
///
/// DG then extended it to `:-`: "the `:-` default, let's change it to match the
/// `:?` so eager/side-effects are a non-issue." That is the whole argument. A
/// conditionally-taken operand raises the question of *when* it is evaluated
/// and *what* it may do; a literal cannot raise either. It retires the
/// eager/lazy hybrid, SB5035's `$(random)` ban, and DD#78's read discipline for
/// `:-` — the taint hole DD#78 Stage 1 closed with a checker is now unreachable
/// by construction, which is the stronger form of the same guarantee.
///
/// A literal alone is not enough, because it degrades *silently*: `${y:-$m}`
/// looks like bash and quietly yields the two characters `$m`. That is the
/// failure DD#65's SB4047 machinery exists to prevent. So a `$` in the operand
/// is **SB4070** unless the whole operand is single-quoted, which is the
/// explicit way to ask for a literal `$`:
///
/// ```text
/// ${y:-costs $5}      SB4070
/// ${y:-'costs $5'}    → costs $5
/// ${y:?'costs $5'}    → aborts with:  prog: costs $5
/// ```
///
/// Anything computed goes in a conditional, where it is lazy by construction
/// and its side effects are visible. DG: "If I anticipate an empty array, I can
/// use a conditional to get whatever behavior I desire."
fn parse_disposition_literal(text: &str, span: Span) -> Result<Box<Expr>, Diagnostic> {
    let quoted = text.len() >= 2 && text.starts_with('\'') && text.ends_with('\'');
    let literal = if quoted {
        text[1..text.len() - 1].to_string()
    } else if text.contains('$') {
        return Err(disposition_default_expansion(span));
    } else {
        text.to_string()
    };
    Ok(Box::new(Expr {
        kind: ExprKind::Literal(literal),
        span,
    }))
}

fn disposition_default_expansion(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4070",
        "a `:-` default and a `:?` abort message are literals, so a `$` \
         expansion in one would be silently emitted as text rather than \
         expanded. Single-quote the operand to mean a literal `$` \
         (`${x:-'costs $5'}`), or, if you want a computed value, test for the \
         condition explicitly: `str v=$x; if [[ -z $v ]]; then v=$y; fi`"
            .to_string(),
    )
}

/// `@{a:?[WORD]}` operand: the WORD if present, else `None` (bare `@{a:?}`
/// aborts with no message word).
fn optional_splat_word(rest: &str, span: Span) -> Result<Option<Box<Expr>>, Diagnostic> {
    if rest.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parse_disposition_literal(rest, span)?))
    }
}

/// `@{a:~}` (DD#84 elide) → a wordless `arrvals_default` (has_word=0 → zero args
/// on an empty array). Like the scalar `${x:~}` it takes NO word, so
/// `@{a:~WORD}` is a syntax error.
fn parse_elide_splat_op(
    rest: &str,
    name: &str,
    span: Span,
) -> Result<(&'static str, Option<Box<Expr>>), Diagnostic> {
    if !rest.is_empty() {
        return Err(parse_error(
            span,
            "SB5033",
            format!(
                "`@{{{name}:~}}` (elide) takes no word — `:~` drops the splat \
                 when `{name}` is empty; use `@{{{name}:-WORD}}` to substitute \
                 a word, or `@{{{name}:-}}` for a single empty arg"
            ),
        ));
    }
    Ok(("arrvals_default", None))
}

/// `@{a:!DISP}` (DD#91 addendum) — a trusted splat. `!` is the modifier and
/// `DISP` the disposition; the caller sets `taint_off`. A bare `@{a:!}` states
/// no disposition, so it yields the bare `arrvals` operator and is rejected by
/// the ordinary DD#69 check (SB5032) rather than by a special case here.
fn parse_trusted_splat_op(
    rest: &str,
    name: &str,
    span: Span,
) -> Result<(&'static str, Option<Box<Expr>>), Diagnostic> {
    if rest.is_empty() {
        return Ok(("arrvals", None));
    }
    parse_splat_op(rest, name, span)
}

/// A plausible `[A-Za-z_][A-Za-z0-9_]*` identifier.
pub(crate) fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `Tok::Dollar(name)` arm — a simple variable expansion. Consumes the
/// `$name` token and pushes one part. A following `[idx]` never reaches here:
/// the lexer desugars `$a[K]` to the braced `${a[K]}` spelling
/// ([`Scanner::braceless_subscript`]), so there is one subscript grammar and
/// this arm does not need a second one. `$LINENO` gets no special handling —
/// it flows through as an ordinary expansion, so an unassigned `$LINENO` is
/// rejected as an undefined variable like any other (no magic vars).
fn parse_dollar_arm(
    scanner: &mut Scanner,
    name: String,
    span: Span,
    _start: usize,
    parts: &mut Vec<Expr>,
) {
    scanner.next();
    if let Some(e) = magic_sugar_expansion(&name, span) {
        parts.push(e);
        return;
    }
    parts.push(Expr {
        kind: ExprKind::SimpleExpansion(name),
        span,
    });
}

/// DD#111: variables are not magic in sash. Every `$X` punctuation form is
/// **sugar** for the intrinsic that is the real thing, and the front end is
/// the only place that knows it — downstream sees the intrinsic and nothing
/// else. This is the whole desugar: `$$` → `$(getpid)`, `$!` → `$(getcpid)`,
/// `$?` → `$(status)`.
///
/// Returns `None` for an ordinary variable name, so a caller can use it as a
/// filter. **Every `SimpleExpansion` construction site that can receive one of
/// these names must route through here**; `assert_no_magic_sugar` proves after
/// the fact that none survived, which is what makes a missed site loud instead
/// of silent. A missed site would keep a magic variable in that one context,
/// and the SB5012 `$?` state machine would stop seeing reads there.
///
/// `$?` went last, because it carried the SB5012 state machine with it:
/// `validate::find_status_reads` matched `SimpleExpansion("?")` and nothing
/// else, so reversing the fold without moving it would have left the
/// availability check finding nothing — every spec still green, and SB5012
/// never raised again. It now recognises `ir::intrinsic_call_name`, pinned to
/// this constructor by `intrinsic_expansion_round_trips`.
pub(crate) fn magic_sugar_expansion(name: &str, span: Span) -> Option<Expr> {
    Some(intrinsic_expansion(magic_sugar_intrinsic(name)?, span))
}

/// The name half of [`magic_sugar_expansion`]: `$X` punctuation → the
/// intrinsic it is sugar for, with no `Expr` built. The **single table** both
/// front-end paths share.
///
/// The word path wants an `Expr` (`magic_sugar_expansion`); the arithmetic
/// path cannot use one, because inside `$(( ))` the sugar has to become an
/// `ArithExpr::FuncCall` — the same node `(getpid)` produces — not a
/// `CommandSubstitution`. Before this split, `parse::arith::scan_arith_dollar`
/// had its **own** scan for `$$`/`$?`/`$!` that produced
/// `ArithExpr::DollarVar("$")` and never consulted this table at all. That is
/// a magic variable surviving parse, which DD#111 forbids, and
/// `assert_no_magic_sugar` could not see it: the guard walks `ExprKind`, and
/// an `ArithExpr` is a different type.
pub(crate) fn magic_sugar_intrinsic(name: &str) -> Option<&'static str> {
    match name {
        "$" => Some("getpid"),
        "!" => Some("getcpid"),
        "?" => Some("status"),
        _ => None,
    }
}

/// Build `$(name)` — a single-command substitution with a bare literal head
/// and no arguments. The shape `transform::resolve_single_cmdsubst` recognises.
pub(crate) fn intrinsic_expansion(name: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::CommandSubstitution(vec![Statement {
            kind: StmtKind::Command(Command {
                name: Expr {
                    kind: ExprKind::Literal(name.to_string()),
                    span,
                },
                args: vec![],
            }),
            span,
        }]),
        span,
    }
}

/// Collapse the collected `parts` into a final `Expr`: empty → error,
/// single → unwrap, multiple → `Concat`. `span` is the word's own span — it
/// used to be a hardcoded `1:1`, so every "expected expression" pointed at the
/// first byte of the file no matter which line the offending token was on.
fn parts_to_expr(parts: Vec<Expr>, span: Span) -> Result<Expr, Diagnostic> {
    match parts.len() {
        0 => Err(parse_error(
            span,
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

// ── Double-quoted string interpolation ───────────────────────────

/// Convert lexer StringParts to IR Exprs for double-quoted strings.
pub fn string_parts_to_exprs(
    parts: &[StringPart],
    span: Span,
    _scanner: &Scanner,
) -> Result<Vec<Expr>, Diagnostic> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        i += emit_string_part(&parts[i..], span, &mut result)?;
    }
    Ok(result)
}

/// Convert the next [StringPart] into one or more Exprs appended to `result`.
/// Returns the number of input parts consumed.
///
/// A `"$a[K]"` subscript never arrives as a `Variable` + literal pair: the
/// lexer desugars it to the braced `Expansion("a[K]")`
/// ([`Scanner::dq_subscript_or_variable`]), so it lands on the
/// `StringPart::Expansion` arm with the one subscript grammar behind it.
fn emit_string_part(
    parts_from_here: &[StringPart],
    span: Span,
    result: &mut Vec<Expr>,
) -> Result<usize, Diagnostic> {
    Ok(match &parts_from_here[0] {
        StringPart::Literal(s) => {
            result.push(Expr {
                kind: ExprKind::Literal(s.clone()),
                span,
            });
            1
        }
        StringPart::Variable(name) => {
            result.push(magic_sugar_expansion(name, span).unwrap_or_else(|| Expr {
                kind: ExprKind::SimpleExpansion(name.clone()),
                span,
            }));
            1
        }
        StringPart::Expansion(content, origin) => {
            result.push(parse_parameter_expansion(content, span, *origin)?);
            1
        }
        StringPart::CommandSub(content, origin) => {
            result.push(command_sub_to_expr(content, span, *origin)?);
            1
        }
        StringPart::ArithSub(content) => {
            // A malformed `$(( … ))` inside a double-quoted string used to be
            // swallowed into a literal 0 here, so `"$(( 1 +* 2 ))"` printed 0
            // while the identical expression outside quotes was a hard error.
            let arith = super::arith::parse_arith_str(content.trim(), span)?;
            result.push(Expr {
                kind: ExprKind::Arithmetic(Box::new(arith)),
                span,
            });
            1
        }
    })
}

/// Build the index expression for a subscript `name[idx]`. A `$var` or
/// `${var}` index expands the variable at runtime (dynamic subscript,
/// DD#33): for an indexed array it yields the integer index, for a map
/// the string key. Any other text is a literal key (a bare integer for
/// indexed arrays, a bare string for maps).
pub(crate) fn subscript_index_expr(idx: &str, span: Span) -> Expr {
    if let Some(var) = dynamic_subscript_var(idx) {
        return Expr {
            kind: ExprKind::SimpleExpansion(var.to_string()),
            span,
        };
    }
    Expr {
        kind: ExprKind::Literal(idx.to_string()),
        span,
    }
}

/// If `idx` is `$name` or `${name}` with `name` a valid identifier,
/// return `name`; otherwise `None`.
fn dynamic_subscript_var(idx: &str) -> Option<&str> {
    let rest = idx.strip_prefix('$')?;
    let var = match rest.strip_prefix('{') {
        Some(inner) => inner.strip_suffix('}')?,
        None => rest,
    };
    let mut chars = var.chars();
    let first = chars.next()?;
    if (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Some(var)
    } else {
        None
    }
}

/// `$(…)` content inside a double-quoted string.
///
/// **This delegates to `parse_command_substitution` — the same entry the
/// unquoted path uses — and that is the whole point.** The two used to be
/// separate implementations, and they diverged in the two ways separate
/// implementations always do:
///
/// - The unquoted path probes `Tok::Lt` first and yields a real
///   `ExprKind::SlurpFile`. This one did not, so `"$(<f)"` parsed as a
///   command substitution whose body was *empty* — `echo "$(</etc/hostname)"`
///   printed nothing where bash prints the file, and `str v="$(<f)"` laundered
///   tainted file contents into a clean-looking variable while the unquoted
///   spelling was SB5020.
/// - The old `parse_sub_script` was declared `-> Vec<Statement>`, so it could
///   not carry a diagnostic. `parse_statement` *did* report the error;
///   `Err(_) => break` threw it away and returned an empty body. `"$(if)"` and
///   `"$(echo a; if)"` compiled silently (the latter dropping its tail) while
///   the identical unquoted text was SB4014.
///
/// The `ArithSub` arm beside the caller carries the same story for `$(( … ))`.
/// Reusing one parser is what stops this recurring a third time.
///
/// The lexer consumed the closing `)` while extracting `content`, so it is
/// re-appended: the sub-scanner then sees exactly the token stream the
/// unquoted path sees after `$(`. It starts at `origin`, so the statements
/// inside report where they are in the program.
///
/// A third divergence was closed here earlier and is worth not re-introducing:
/// this used to carve out content starting with a backtick and keep it as a
/// verbatim `Literal`, so `"$(`cmd`)"` became a literal string that dodged
/// both the backtick rejection and the taint check. Backticks are rejected
/// everywhere now (SB5076), and re-lexing the inner script is what surfaces
/// the sentinel.
fn command_sub_to_expr(content: &str, span: Span, origin: Origin) -> Result<Expr, Diagnostic> {
    let src = format!("{})", content);
    let mut sub = Scanner::with_origin(&src, origin);
    let parsed = parse_command_substitution(&mut sub, span);
    with_scanner_fatal(&sub, parsed)
}

/// The result of parsing a re-lexed construct, unless its scanner hit one of
/// its own fatals — an unterminated construct, a rejected `$'…'` escape, a
/// `#` glued to an operator — which then wins, exactly as the top-level
/// scanner's fatal outranks every other diagnostic in `parse`.
///
/// Every re-lex site routes through this. The sub-scanner used to be dropped
/// with its fatals unread, so whatever only it could see was lost: the
/// rejected escape in `"${x:s/b/$'\101'/}"` printed `a101c`, the unclosed `$(`
/// in `${x:s/$(echo/b/}` was quietly closed and printed `babc`, and the glued
/// `#` in `"$(echo a;# c)"` was looked up as a command (SB5011).
fn with_scanner_fatal<T>(sub: &Scanner, parsed: Result<T, Diagnostic>) -> Result<T, Diagnostic> {
    match super::parser::scanner_fatal(sub) {
        Some(fatal) => Err(fatal),
        None => parsed,
    }
}

// ── Parameter expansion (${...}) ─────────────────────────────────
//
// Parses the content between ${ and }. Handles:
//   ${#var}         → strlen
//   ${!var[@]}      → arrkeys_retired (rejected, SB4038 — use $(keys m))
//   ${var[idx]}     → subscript
//   ${var:s/re/r/}  → DD#39 sed expression
//   ${var:off:len}  → substring
//   ${var:-default} → standard operators
//   ${var/pat/rep}  → substitution (glob → regex)
//   ${var@X}        → rejected (SB4015)

/// Parse ${...} content into an IR ParameterExpansion. `origin` is where
/// `content` starts in the program, for the nested parts that are re-lexed.
pub fn parse_parameter_expansion(
    content: &str,
    span: Span,
    origin: Origin,
) -> Result<Expr, Diagnostic> {
    if let Some(e) = try_leading_form(content, span, origin)? {
        return Ok(e);
    }

    let (name, name_end) = scan_param_name(content);
    if name_end >= content.len() {
        return Ok(Expr::param(name, None, None, span));
    }

    if let Some(e) = try_subscript(content, name, name_end, span, origin)? {
        return Ok(e);
    }

    let rest = &content[name_end..];

    if let Some(e) = parse_cell_operator(rest, name, span, origin.after(name))? {
        return Ok(e);
    }

    // DG (2026-08-29): no ambiguity. Reaching here means there is trailing
    // content after the name that no parser recognised (name_end < len, so
    // `rest` is non-empty) — e.g. `${x&y}`, `${x:foo}`. It used to silently
    // degrade to a bare `${name}` (dropping `rest`). Carry the raw `rest` on a
    // sentinel `unrecognized` operator so `rejected_param_operator_diag` rejects
    // it (SB4047): the compiler must never emit code for input it doesn't fully
    // understand.
    Ok(unrecognized_parameter_operator(rest, name, span))
}

/// The forms told apart by their leading byte, before any name is scanned: no
/// name at all (`${}`, `${:…}`), a braced splat `${@}` / `${*[…]}`, `${#…}`,
/// and `${!…}`.
fn try_leading_form(content: &str, span: Span, origin: Origin) -> Result<Option<Expr>, Diagnostic> {
    if names_no_variable(content) {
        return Ok(Some(unrecognized_parameter_operator(content, "", span)));
    }
    if let Some(e) = try_braced_splat(content, span, origin)? {
        return Ok(Some(e));
    }
    let bytes = content.as_bytes();
    Ok(try_strlen_or_arrlen(content, bytes, span)
        .or_else(|| try_indirection_or_arrkeys(content, bytes, span)))
}

/// `${}` and the `:`-led operators on no name (`${:-x}`, `${:s/a/b/}`,
/// `${:0:1}`). Every one of these used to build a real operator on a variable
/// called `` — `${}` printed nothing, `${:-x}` printed `x`, `${:0:1}` reached a
/// codegen ICE — where bash rejects them all as a bad substitution. They take
/// the `unrecognized` sentinel, whose diagnostic names the missing variable.
///
/// The other empty-name spellings are already rejected under their own codes
/// and keep them: `${#}`/`${?}`/`${-}` are dropped operators (SB4034),
/// `${[0]}` has no collection (SB4047), `${!}` is indirection (SB4003), and
/// `${@}`/`${*}` are splats (SB5045). A name is the leading `[A-Za-z0-9_]*`
/// run (`scan_param_name`), so empty or `:`-led content has none.
fn names_no_variable(content: &str) -> bool {
    content.is_empty() || content.starts_with(':')
}

fn unrecognized_parameter_operator(rest: &str, name: &str, span: Span) -> Expr {
    Expr::param(
        name,
        Some("unrecognized"),
        Some(Box::new(Expr {
            kind: ExprKind::Literal(rest.to_string()),
            span,
        })),
        span,
    )
}

/// The operator-suffixed `${name OP…}` forms: `:s/…` sed, `:off:len`
/// substring, `@X` transforms, and the standard `:-`/`:=`/`#`/`%`/… operators.
/// Returns `None` for a bare `${name}` (empty/unmatched `rest`).
fn parse_cell_operator(
    rest: &str,
    name: &str,
    span: Span,
    origin: Origin,
) -> Result<Option<Expr>, Diagnostic> {
    if let Some(e) = try_sed_op(rest, name, span, origin)?
        .or_else(|| try_substring_op(rest, name, span))
        .or_else(|| try_at_transform(rest, name, span))
    {
        return Ok(Some(e));
    }
    parse_standard_operator(rest, name, span)
}

/// `${@…}` / `${*…}` are braced spellings of the `$@` / `$*` splat — bare
/// `${@}`/`${*}`, or with a modifier (`${@:-x}`, `${@Q}`). `scan_param_name`
/// does not treat `@`/`*` as name characters, so these otherwise fall through
/// to an empty-name `ParameterExpansion` (the sigil lost) and slip past
/// validation. Parse any such form as the unbraced `SimpleExpansion` so the
/// SB5045 "sash has no `$@`/`$*`" reject (`validate::check_dollar_at`) sees it;
/// a dropped modifier is immaterial — the whole form is rejected. Array
/// `[@]`/`[*]` forms are unaffected (their content starts with the array name).
///
/// A **subscript** on the splat (`${@[0]}`) is the one modifier that is not
/// immaterial: it has its own diagnostic, SB5031 "the positional splat isn't
/// subscriptable", which names `$1`..`$N` and `${args[i]}` instead of the
/// generic "sash has no `$@`". Keep the `Subscript` shape so
/// `validate::splat_not_subscriptable_diag` can see it — the brace-less
/// `$a[K]` desugar routes `$@[0]` through here, and the two spellings must
/// land on the same diagnostic (DG, 2026-10-03). Specs 80.20/80.21.
fn try_braced_splat(content: &str, span: Span, origin: Origin) -> Result<Option<Expr>, Diagnostic> {
    let sigil = match content.as_bytes().first() {
        Some(b'@') => "@",
        Some(b'*') => "*",
        _ => return Ok(None),
    };
    if content.as_bytes().get(1) == Some(&b'[') {
        let Some((idx, _)) = capture_balanced_subscript(content, 1) else {
            return Ok(None);
        };
        let index = parse_subscript_index(&idx, span, origin.after(&content[..2]))?;
        return Ok(Some(Expr {
            kind: ExprKind::Subscript {
                name: sigil.to_string(),
                index: Box::new(index),
            },
            span,
        }));
    }
    Ok(Some(Expr {
        kind: ExprKind::SimpleExpansion(sigil.to_string()),
        span,
    }))
}

/// `${#…}` length sugar — retired entirely (DD#75 §4, amended 2026-08-27).
/// Every form (`${#x}`, `${#a}`, `${#a[@]}`, `${#a[i]}`) is tagged
/// `len_retired` for the parse-level reject; `$(len x)` (value) / `(len x)`
/// (arithmetic) is the sole length spelling.
fn try_strlen_or_arrlen(content: &str, bytes: &[u8], span: Span) -> Option<Expr> {
    if bytes.first() != Some(&b'#') || bytes.len() <= 1 || bytes[1] == b'#' {
        return None;
    }
    let rest = &content[1..];
    let name = rest.split('[').next().unwrap_or(rest);
    Some(Expr::param(name, Some("len_retired"), None, span))
}

/// `${!var}` → indirection, or `${!var[@]}` → array keys.
fn try_indirection_or_arrkeys(content: &str, bytes: &[u8], span: Span) -> Option<Expr> {
    if bytes.first() != Some(&b'!') {
        return None;
    }
    let rest = &content[1..];
    if rest.contains("[@]") || rest.contains("[*]") {
        // DD#75 §4: the `${!a[@]}` / `${!a[*]}` bracket-keys form is retired
        // (use `$(keys m)`, maps only); tag it for the parse-level reject.
        let name = rest.split('[').next().unwrap_or(rest);
        return Some(Expr::param(name, Some("arrkeys_retired"), None, span));
    }
    Some(Expr::param(rest, Some("!"), None, span))
}

/// Scan the leading identifier in a parameter expansion body.
fn scan_param_name(content: &str) -> (&str, usize) {
    let bytes = content.as_bytes();
    let mut name_end = 0;
    while name_end < bytes.len()
        && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
    {
        name_end += 1;
    }
    (&content[..name_end], name_end)
}

/// DG (2026-09-16, option C): a subscript's collection name has to be a
/// variable name. `scan_param_name` scans a run of `[A-Za-z0-9_]`, which also
/// accepts a leading digit and an empty run, so `${1[0]}` and `${[0]}` built a
/// `Subscript` on a collection nothing can declare and codegen panicked with
/// "ICE: undefined array". Same principle as the other SB4047 rejects: never
/// emit code for input the compiler does not fully understand.
fn bad_subscript_name_diag(content: &str, name: &str, span: Span) -> Diagnostic {
    let detail = if name.is_empty() {
        "there is no name before the `[`".to_string()
    } else {
        format!("`{name}` is not a variable name")
    };
    parse_error(
        span,
        "SB4047",
        format!(
            "`${{{content}}}` has no collection to subscript — {detail}. A name \
             is letters, digits and `_`, and cannot start with a digit"
        ),
    )
}

/// `${var[idx]}` — `@`/`*` produce arrvals (or chained operator); literal
/// index produces a `Subscript` expression.
fn try_subscript(
    content: &str,
    name: &str,
    name_end: usize,
    span: Span,
    origin: Origin,
) -> Result<Option<Expr>, Diagnostic> {
    if content.as_bytes()[name_end] != b'[' {
        return Ok(None);
    }
    if !is_plain_identifier(name) {
        return Err(bad_subscript_name_diag(content, name, span));
    }
    // DD#80: capture the index balanced across `[] {} ()` and skipping quotes,
    // so a `]` inside a nested `${…}` / `$(( … ))` / quoted key does not close
    // the subscript early. `close` is the byte index of the matching `]`.
    let Some((idx, close)) = capture_balanced_subscript(content, name_end) else {
        return Ok(None);
    };
    let after_bracket = &content[close + 1..];
    let index_origin = origin.after(&content[..=name_end]);
    subscript_with_index(name, &idx, after_bracket, span, index_origin).map(Some)
}

/// `${name[idx]…}` once the index is captured: `idx` is the text between the
/// brackets, starting at `origin` in the program, and `after_bracket` is
/// whatever follows the `]`.
fn subscript_with_index(
    name: &str,
    idx: &str,
    after_bracket: &str,
    span: Span,
    origin: Origin,
) -> Result<Expr, Diagnostic> {
    if idx == "@" || idx == "*" {
        return Ok(splat_subscript_expr(name, after_bracket, span));
    }

    // DD#80: re-parse the index as a full expression so an explicit nested
    // context (`$var`, `${…}`, `$(…)`, `$(( … ))`, quoted string) works and
    // lowers to SSA temporaries like any other operand. An implicit/ambiguous
    // bare form (brackets/operators/mixed) parses to a `Literal`/`Concat` that
    // `validate::check_subscript_index` rejects (SB4041).
    let index = Box::new(parse_subscript_index(idx, span, origin)?);

    if after_bracket.is_empty() {
        return Ok(Expr {
            kind: ExprKind::Subscript {
                name: name.to_string(),
                index,
            },
            span,
        });
    }

    // `${a[i]OP}` — a trailing operator applies to the *cell* value. DG: a
    // string cell behaves like a string variable, so route the operator through
    // the same scalar parsers and attach the index. The `:-`/`:?` dispositions,
    // substring (`:o:l`), and sed (`:s/`) all lower for a cell (edge 4); the
    // pruned families (`#`/`%`/`/`/`@X`) reject on their operator string exactly
    // as for a scalar, and any other `:`-led op rejects as `cell_string_op`.
    let operator_origin = origin.after(idx).after("]");
    subscript_operator_expr(name, index, after_bracket, span, operator_origin)
}

/// Capture the `[ … ]` subscript index that starts at `content[open] == '['`,
/// balanced across `[] {} ()` and skipping quoted spans, so a `]` inside a
/// nested `${…}` / `$(… )` / `$(( … ))` / `"…"` / `'…'` does not close the
/// subscript early (DD#80). Returns `(index text, byte index of the matching
/// `]`)`, or `None` if unbalanced.
fn capture_balanced_subscript(content: &str, open: usize) -> Option<(String, usize)> {
    let close = balanced_subscript_close(content.as_bytes(), open)?;
    Some((content[open + 1..close].to_string(), close))
}

/// Byte index of the `]` that closes the subscript opening at `bytes[open]`,
/// or `None` if the brackets are unbalanced (or `open` is not a `[`).
///
/// The lexer calls this too, so that the brace-less `$a[K]` captures **exactly**
/// the span the braced `${a[K]}` does. It is one function on purpose: the two
/// spellings disagreeing about where a subscript ends is precisely the class of
/// bug this seam was introduced to remove (DG, 2026-10-03 — "all paths must
/// agree"). Do not grow a second scanner for either caller.
pub(super) fn balanced_subscript_close(bytes: &[u8], open: usize) -> Option<usize> {
    if bytes.get(open) != Some(&b'[') {
        return None;
    }
    balanced_close(bytes, open)
}

/// Byte index of the closer that balances the `[`, `{` or `(` at `bytes[open]`,
/// or `None` if unbalanced (or `open` is not one of those three openers).
///
/// The scan is the subscript scan — it was always generic over `[] {} ()` and
/// quoted spans, and only its *entry* was pinned to `[`. The arithmetic lexer
/// needs the `${ … }` entry: it used to stop a braced body at the first `}`,
/// so `$(( ${a[${i}]} ))` truncated to `a[${i` and died on the stray `]`
/// (SB4035) while the three other spellings of that same read — `${a[${i}]}`
/// in a word, `$(( a[${i}] ))`, and the lexer-desugared `$(( $a[${i}] ))` —
/// all accepted it. One scanner, every opener; do not grow a second.
pub(super) fn balanced_close(bytes: &[u8], open: usize) -> Option<usize> {
    let want = match bytes.get(open)? {
        b'[' => b']',
        b'{' => b'}',
        b'(' => b')',
        _ => return None,
    };
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        if subscript_scan_step(&mut quote, &mut depth, b) {
            // A balanced run closes on its own closer; a different closer at
            // depth 0 means the brackets were mismatched.
            return (b == want).then_some(i);
        }
    }
    None
}

/// One byte of `balanced_subscript_close`'s scan: track quoting and bracket
/// depth, reporting `true` when `b` is the closer that balances the opening
/// `[` (which the caller then checks is actually a `]`).
fn subscript_scan_step(quote: &mut Option<u8>, depth: &mut i32, b: u8) -> bool {
    if advance_in_quote(quote, b) {
        return false;
    }
    match b {
        b'"' | b'\'' => *quote = Some(b),
        b'[' | b'{' | b'(' => *depth += 1,
        b']' | b'}' | b')' => return step_close_depth(depth),
        _ => {}
    }
    false
}

/// While inside a `"`/`'` quote, consume the byte (closing the quote on its
/// matching delimiter) and report `true` so the caller skips it.
fn advance_in_quote(quote: &mut Option<u8>, b: u8) -> bool {
    if let Some(q) = *quote {
        if b == q {
            *quote = None;
        }
        return true;
    }
    false
}

/// Decrement bracket depth for a closer; `true` when the subscript's opening
/// `[` is now balanced (depth 0).
fn step_close_depth(depth: &mut i32) -> bool {
    *depth -= 1;
    *depth == 0
}

/// Re-parse a captured subscript index as a full value expression (DD#80), so
/// nested contexts survive. A leftover tail (a mixed/bare form the value parser
/// cannot consume whole), or a parse failure of an implicit form, falls back to
/// a `Literal` — which `validate::check_subscript_index` then rejects as
/// ambiguous (SB4041). The index scanner's own fatal is an error, not a
/// fallback ([`parse_whole_index`]), and so is an explicit form that fails to
/// parse ([`opens_explicit_index`]). `origin` is where `idx` starts in the
/// program.
pub(super) fn parse_subscript_index(
    idx: &str,
    span: Span,
    origin: Origin,
) -> Result<Expr, Diagnostic> {
    let trimmed = idx.trim();
    let origin = origin.after(&idx[..idx.len() - idx.trim_start().len()]);
    Ok(match parse_whole_index(trimmed, origin)? {
        // The sub-scanner starts at the index's own position, so everything
        // nested inside the index reports exactly where it is. The expression
        // the caller reports on is still re-anchored to the whole
        // `${name[…]}` (DD#114 Span).
        Some(mut expr) => {
            expr.span = span;
            expr
        }
        None => Expr {
            kind: ExprKind::Literal(trimmed.to_string()),
            span,
        },
    })
}

/// The index text as a value expression, or `None` if it is not one the value
/// parser can consume *whole* — a leftover tail means a mixed or bare form.
///
/// A bare dollar-subscript `$name[…]` is excluded even though it parses: it is
/// ambiguous as an index (DD#80), because the explicit cell-read index form is
/// the braced `${name[…]}`. Returning `None` for both cases leaves the caller
/// holding a `Literal`, which `validate::check_subscript_index` then rejects as
/// ambiguous (SB4041). The index scanner's own fatal is an error, not a
/// fallback: it is the one thing only this scanner can see. An explicit form's
/// parse failure is an error too ([`opens_explicit_index`]).
fn parse_whole_index(trimmed: &str, origin: Origin) -> Result<Option<Expr>, Diagnostic> {
    if is_bare_dollar_subscript(trimmed) {
        return Ok(None);
    }
    let mut scanner = Scanner::with_origin(trimmed, origin);
    let whole = match parse_expr(&mut scanner) {
        Ok(expr) if scanner.at_eof() => Some(expr),
        Err(d) if opens_explicit_index(trimmed) => return with_scanner_fatal(&scanner, Err(d)),
        _ => None,
    };
    with_scanner_fatal(&scanner, Ok(whole))
}

/// DD#80's explicit forms 2-4 — `$var`, an expansion (`${…}`, `$(…)`,
/// `$(( … ))`) and a quoted key — open with `$`, `"` or `'`. When one of them
/// fails to parse, the failure is that form's own error: SB4041's advice is to
/// write an explicit form, which would send the reader back to the one they
/// wrote. `${a[$(( 1 + ))]}` used to get exactly that advice. An implicit form
/// (`(1+2)`, an empty index) has no construct of its own to report, so its
/// failure is still SB4041.
fn opens_explicit_index(trimmed: &str) -> bool {
    trimmed.starts_with(['$', '"', '\''])
}

/// `$name[…]` — a `$`-sigil identifier immediately followed by a `[` subscript,
/// with no brace (`${…}` returns false). Ambiguous as a braced index.
fn is_bare_dollar_subscript(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('$') else {
        return false;
    };
    match rest.bytes().next() {
        Some(b) if b.is_ascii_alphabetic() || b == b'_' => {}
        _ => return false,
    }
    let after_ident = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
    after_ident.starts_with('[')
}

/// Build the `${a[i]OP}` expansion. `:-`/`:?` attach the cell index to the
/// scalar `ParameterExpansion` (so codegen reads the cell); `:s/` desugars to
/// `$(sub|gsub "${a[i]}" …)`; substring `:o:l` carries the index (the transform
/// builds a cell-reading `Substring` node); every other operator parses as the
/// scalar form so `#`/`%`/`/`/`@X` reject identically. `origin` is where
/// `after` starts in the program.
fn subscript_operator_expr(
    name: &str,
    index: Box<Expr>,
    after: &str,
    span: Span,
    origin: Origin,
) -> Result<Expr, Diagnostic> {
    // Sed on a cell (`${a[i]:s/RE/SUB/}`) desugars to `$(sub|gsub "${a[i]}" RE
    // SUB)` — the cell read as the subject — reusing the scalar `$(sub)`
    // machinery (edge 4). A cell behaves like a string variable (DG). It is the
    // one family that *consumes* the index (as the subject) rather than
    // attaching it, which is why it cannot share the tail below.
    if let Some(body) = after.strip_prefix(":s/") {
        let subject = quoted_cell_expr(name, index, span);
        return build_sed_or_malformed(body, subject, name, after, span, origin.after(":s/"));
    }
    let mut expr = cell_attaching_operator(name, after, span)?;
    attach_cell_index(&mut expr, index);
    Ok(expr)
}

/// The `${a[i]OP}` families that parse as their scalar form and then have the
/// cell index attached by the caller. Split out of `subscript_operator_expr`
/// to keep that dispatcher under the complexity gate; `:s/` is deliberately
/// absent (it consumes the index instead of attaching it).
fn cell_attaching_operator(name: &str, after: &str, span: Span) -> Result<Expr, Diagnostic> {
    // DD#91's trust modifier applies to a cell's value just as to a scalar.
    let is_disposition_or_trust = after.starts_with(":-")
        || after.starts_with(":?")
        || after.starts_with(":~")
        || after.starts_with(":!");
    if is_disposition_or_trust {
        return Ok(scalar_op_or_unrecognized(
            parse_standard_operator(after, name, span)?,
            after,
            name,
            span,
        ));
    }
    if after.starts_with(':') {
        // substring (`:o:l`) on a cell — carry the cell index; the transform
        // builds a `Substring` node that reads the cell as the source (edge 4).
        // A cell behaves like a string variable (DG). Any other `:`-led op that
        // isn't substring/sed/a disposition/trust has no cell lowering — reject
        // explicitly (SB4039) rather than silently drop.
        return Ok(try_substring_op(after, name, span)
            .unwrap_or_else(|| Expr::param(name, Some("cell_string_op"), None, span)));
    }
    // `#`/`##`/`%`/`%%`/`/`-family/`@X`: parse as the scalar operator so the
    // same reject fires; the index rides along but the operator rejects first.
    if let Some(expr) = try_at_transform(after, name, span) {
        return Ok(expr);
    }
    Ok(scalar_op_or_unrecognized(
        parse_standard_operator(after, name, span)?,
        after,
        name,
        span,
    ))
}

fn scalar_op_or_unrecognized(parsed: Option<Expr>, rest: &str, name: &str, span: Span) -> Expr {
    parsed.unwrap_or_else(|| unrecognized_parameter_operator(rest, name, span))
}

/// Attach a cell subscript to a freshly-parsed `${name OP}` expansion, turning
/// it into `${name[index] OP}`. Sed consumes its index before reaching here;
/// every remaining cell-operator parser produces a `ParameterExpansion`.
fn attach_cell_index(expr: &mut Expr, index: Box<Expr>) {
    let ExprKind::ParameterExpansion(pe) = &mut expr.kind else {
        unreachable!("non-sed cell operators produce parameter expansions");
    };
    pe.index = Some(index);
}

/// The `${a[@]}` / `${a[*]}` splat branch. A trailing operator (e.g.
/// `${a[@]/x/y}`) parses via `parse_subscript_operator`; otherwise the bare
/// splat is the DD#75 §4 retired form (`arrvals_retired`), rejected downstream.
fn splat_subscript_expr(name: &str, after_bracket: &str, span: Span) -> Expr {
    if !after_bracket.is_empty() {
        if let Some(op_result) = parse_subscript_operator(name, after_bracket, span) {
            return op_result;
        }
    }
    Expr::param(name, Some("arrvals_retired"), None, span)
}

/// `${var:s/regex/replacement/flags}` — DD#63 substitution sugar. Desugars
/// to `$(sub|gsub "$var" RE SUB)` with RE/SUB parsed as interpolating words
/// (so `$x` in a field expands). A payload missing either mandatory `/`
/// field delimiter is malformed: return a `s_malformed` sentinel that the
/// validator turns into SB4032. (A malformed `$(( … ))` *inside* one of the
/// RE/SUB words is a different matter — that is a real parse diagnostic and
/// propagates out, which is why this path is fallible.)
fn try_sed_op(
    rest: &str,
    name: &str,
    span: Span,
    origin: Origin,
) -> Result<Option<Expr>, Diagnostic> {
    let Some(body) = rest.strip_prefix(":s/") else {
        return Ok(None);
    };
    let subject = quoted_var_expr(name, span);
    Ok(Some(build_sed_or_malformed(
        body,
        subject,
        name,
        rest,
        span,
        origin.after(":s/"),
    )?))
}

/// Build `$(sub|gsub SUBJECT RE SUB)` from a `:s/RE/SUB/` payload with the given
/// interpolating `subject` (a scalar `"$var"` or a cell `"${a[i]}"`), or the
/// `s_malformed` sentinel (→ SB4032) when a `/` delimiter is missing. Shared by
/// the scalar and cell sed forms. `origin` is where `body` starts.
fn build_sed_or_malformed(
    body: &str,
    subject: Expr,
    name: &str,
    rest: &str,
    span: Span,
    origin: Origin,
) -> Result<Expr, Diagnostic> {
    if let Some(expr) = build_sed_sub(body, subject, span, origin)? {
        return Ok(expr);
    }
    Ok(Expr {
        kind: ExprKind::ParameterExpansion(ParameterExpansion {
            name: name.to_string(),
            operator: Some("s_malformed".to_string()),
            operand: Some(Box::new(Expr {
                kind: ExprKind::Literal(rest.to_string()),
                span,
            })),
            index: None,
            taint_off: false,
        }),
        span,
    })
}

/// `"${a[i]}"` as a single interpolating argument — the cell read as a sed
/// subject, mirroring `quoted_var_expr` for a scalar variable.
fn quoted_cell_expr(name: &str, index: Box<Expr>, span: Span) -> Expr {
    Expr {
        kind: ExprKind::DoubleQuoted(vec![Expr {
            kind: ExprKind::Subscript {
                name: name.to_string(),
                index,
            },
            span,
        }]),
        span,
    }
}

/// `"$var"` as a single interpolating argument (the substitution subject).
fn quoted_var_expr(name: &str, span: Span) -> Expr {
    let inner = magic_sugar_expansion(name, span).unwrap_or_else(|| Expr {
        kind: ExprKind::SimpleExpansion(name.to_string()),
        span,
    });
    Expr {
        kind: ExprKind::DoubleQuoted(vec![inner]),
        span,
    }
}

/// Split ONE edit off the front of a chain body: `RE/SUB/FLAGS` plus whatever
/// follows it. Flags are the leading run of letters; `rest` is everything
/// after them — empty for a lone edit, or a `,`-introduced continuation
/// (DD#96). Anything else there is malformed, which is what stops the old
/// silent drop: the tail used to be swallowed whole as "flags" and only
/// scanned for `g`, so `${x:s/a/b/:anything}` quietly applied just the first
/// edit (and `…/,s/c/d/g` even turned the FIRST edit into a gsub).
fn split_sed_edit(body: &str) -> Option<(&str, &str, &str, &str)> {
    let re_end = body.find('/')?;
    let rest = &body[re_end + 1..];
    let sub_end = rest.find('/')?;
    let after = &rest[sub_end + 1..];
    let flag_end = after
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(after.len());
    Some((
        &body[..re_end],
        &rest[..sub_end],
        &after[..flag_end],
        &after[flag_end..],
    ))
}

/// Build `$(sub|gsub SUBJECT RE SUB)` from an `RE/SUB/flags` payload. RE/SUB
/// become interpolating double-quoted words (`$x` expands); regex
/// metacharacters are ordinary literal bytes. `g` in the flags selects
/// `gsub`. Returns `None` for a malformed (missing-delimiter) payload.
/// `origin` is where `body` starts in the program.
pub(crate) fn build_sed_sub(
    body: &str,
    subject: Expr,
    span: Span,
    origin: Origin,
) -> Result<Option<Expr>, Diagnostic> {
    let mut value = subject;
    let mut rest = body;
    loop {
        let Some((re, sub, flags, after)) = split_sed_edit(rest) else {
            return Ok(None);
        };
        // `rest` is always a suffix of `body`, so this edit starts here.
        let edit_origin = origin.after(&body[..body.len() - rest.len()]);
        value = build_one_sed_sub(re, sub, flags, value, span, edit_origin)?;
        // DD#96: `,` chains edits, applied left to right — the next edit's
        // subject is this one's result. Only `s///` chains.
        let after = after.trim_start();
        if after.is_empty() {
            return Ok(Some(value));
        }
        let Some(next) = after
            .strip_prefix(',')
            .map(str::trim_start)
            .and_then(|r| r.strip_prefix("s/"))
        else {
            return Ok(None);
        };
        rest = next;
    }
}

/// One `$(sub|gsub SUBJECT RE SUB)` call. RE/SUB become interpolating
/// double-quoted words (`$x` expands); `g` in this edit's own flags selects
/// `gsub`, so a flag binds to its edit rather than the whole chain. `origin`
/// is where `re` starts in the program; `sub` follows it and one `/`.
fn build_one_sed_sub(
    re: &str,
    sub: &str,
    flags: &str,
    subject: Expr,
    span: Span,
    origin: Origin,
) -> Result<Expr, Diagnostic> {
    // DD#63: "`g` is the `sub`/`gsub` choice, not a flag argument. There are
    // **no other flags**" — the engine has no `REG_ICASE` and takes nothing
    // else. This slot used to be read with a bare `flags.contains('g')`, so
    // every other flag was silently ignored: `s/a/b/i` promised a
    // case-insensitive substitution and delivered a case-sensitive one, and a
    // typo'd `s/a/b/x` did nothing at all. Real `sed` rejects `x`. Same shape
    // as the tail-junk drop DD#96 fixed — junk accepted because no spec
    // pinned the rejection.
    if !flags.is_empty() && flags != "g" {
        return Err(parse_error(
            span,
            "SB4032",
            format!(
                "unsupported substitution flag `{flags}` — `g` (replace every \
                 match) is the only one; the regex engine has no \
                 case-insensitive mode, so use `$(tolower …)` on both sides \
                 for `i`"
            ),
        ));
    }
    let cmd_name = if flags == "g" { "gsub" } else { "sub" };
    let re_expr = *parse_interpolating_word(re, span, origin)?;
    let sub_expr = *parse_interpolating_word(sub, span, origin.after(re).after("/"))?;
    let cmd = Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal(cmd_name.to_string()),
                span,
            },
            args: vec![subject, re_expr, sub_expr],
        }),
        span,
    };
    Ok(Expr {
        kind: ExprKind::CommandSubstitution(vec![cmd]),
        span,
    })
}

/// `${var:offset}` / `${var:offset:length}` — substring.
fn try_substring_op(rest: &str, name: &str, span: Span) -> Option<Expr> {
    if !rest.starts_with(':')
        || rest.starts_with(":-")
        || rest.starts_with(":+")
        || rest.starts_with(":=")
        || rest.starts_with(":?")
    {
        return None;
    }
    let sub_content = &rest[1..];
    let first_ch = sub_content.as_bytes().first().copied().unwrap_or(0);
    if !first_ch.is_ascii_digit() && first_ch != b'-' && first_ch != b'(' {
        return None;
    }
    let cleaned = sub_content.replace(['(', ')'], "");
    Some(Expr {
        kind: ExprKind::ParameterExpansion(ParameterExpansion {
            name: name.to_string(),
            operator: Some("substring".to_string()),
            operand: Some(Box::new(Expr {
                kind: ExprKind::Literal(cleaned),
                span,
            })),
            index: None,
            taint_off: false,
        }),
        span,
    })
}

/// `${var@X}` — parameter transform (rejected SB4015 downstream).
///
/// The `[@` guard keeps an unterminated subscript (`${a[@U}`, where the body
/// scanner leaves `[@U` in `rest`) from reading as an `@U` transform. A
/// preceding `!rest.contains("[@]")` test was redundant: `"[@]"` contains
/// `"[@"`, so it could never decide an outcome the `[@` test did not.
fn try_at_transform(rest: &str, name: &str, span: Span) -> Option<Expr> {
    let at_pos = rest.rfind('@')?;
    let after = &rest[at_pos + 1..];
    if after.len() == 1
        && after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && !rest.contains("[@")
    {
        return Some(Expr::param(name, Some(&format!("@{}", after)), None, span));
    }
    None
}

/// Standard parameter-expansion operators (`:-`, `:+`, `:=`, `:?`, `#`, `##`,
/// `%`, `%%`, `/`, `//`, `/#`, `/%`). The slash family compiles the glob
/// pattern to a regex and packs it as `pattern\0replacement` for the runtime.
fn parse_standard_operator(rest: &str, name: &str, span: Span) -> Result<Option<Expr>, Diagnostic> {
    if let Some(peeled) = rest.strip_prefix(":!") {
        return Ok(Some(parse_trusted_operator(peeled, name, span)?));
    }
    parse_disposition_operator(rest, name, span)
}

/// DD#91 (2026-09-09 addendum): `${x:!…}` — the `!` taint-off MODIFIER, then an
/// ordinary disposition or nothing.
///
/// `!` leads, and the grammar forces that: `-` and `?` take a word that runs
/// greedily to `}` (`${u:-foo!}` is the word `foo!`), so a trailing modifier
/// could never be seen. What follows is re-parsed with the colon PUT BACK —
/// `:-W` (empty or unset) and `-W` (unset only) are different operators, and
/// dropping it would silently change which one `${x:!-W}` means.
fn parse_trusted_operator(peeled: &str, name: &str, span: Span) -> Result<Expr, Diagnostic> {
    // `${x:!}` — no disposition at all: the value, trusted. Identical to a bare
    // `${x}` but for the flag, which is exactly the intent.
    if peeled.is_empty() {
        return Ok(trusted(Expr::param(name, None, None, span)));
    }
    Ok(
        match parse_disposition_operator(&format!(":{peeled}"), name, span)? {
            Some(e) => trusted(e),
            // `${x:!foo}` — `!` followed by something that is not a disposition.
            // It must not quietly degrade to a trusted bare `${x}`; carry the raw
            // text on a sentinel the validator rejects (mirrors `s_malformed`).
            None => Expr::param(name, Some("trust_malformed"), None, span),
        },
    )
}

/// Set the DD#91 `!` modifier on an already-parsed expansion.
fn trusted(mut e: Expr) -> Expr {
    let ExprKind::ParameterExpansion(pe) = &mut e.kind else {
        unreachable!("disposition parsers produce parameter expansions for the trust modifier");
    };
    pe.taint_off = true;
    e
}

fn parse_disposition_operator(
    rest: &str,
    name: &str,
    span: Span,
) -> Result<Option<Expr>, Diagnostic> {
    let (op, operand_str) = split_operator(rest);
    let Some(op_str) = op else {
        return Ok(None);
    };
    // DD#65: the `/`-family glob-replace is dropped. Give it a distinct
    // operator (`glob_replace`) so `check_rejected_param_expansion` can reject
    // it (SB4034) without touching the kept `:s/` sed op, which shares the raw
    // `/`//` operator strings. No glob→regex lowering — it never reaches codegen.
    let is_slash_op = matches!(op_str.as_str(), "/" | "//" | "/#" | "/%");
    if is_slash_op {
        return Ok(Some(Expr {
            kind: ExprKind::ParameterExpansion(ParameterExpansion {
                name: name.to_string(),
                operator: Some("glob_replace".to_string()),
                operand: operand_str.map(|s| {
                    Box::new(Expr {
                        kind: ExprKind::Literal(s),
                        span,
                    })
                }),
                index: None,
                taint_off: false,
            }),
            span,
        }));
    }
    // DD#65 + DD#78 (DG, 2026-09-30): the `:-` default and the `:?` abort
    // message are both plain LITERALS. See `parse_disposition_literal` for the
    // argument; the short form is that a conditionally-taken operand raises
    // "when is this evaluated" and "what may it do", and a literal raises
    // neither. Every other operator here (`:off:len`, `glob_replace`, …) was
    // already literal.
    let operand = if matches!(op_str.as_str(), ":-" | ":?") {
        operand_str
            .map(|s| parse_disposition_literal(&s, span))
            .transpose()?
    } else {
        operand_str.map(|s| {
            Box::new(Expr {
                kind: ExprKind::Literal(s),
                span,
            })
        })
    };
    Ok(Some(Expr {
        kind: ExprKind::ParameterExpansion(ParameterExpansion {
            name: name.to_string(),
            operator: Some(op_str),
            operand,
            index: None,
            taint_off: false,
        }),
        span,
    }))
}

/// A `"…"`-body word: `$var`/`${…}`/`$(…)`/`$((…))`/`$'…'` interpolate; every
/// other byte — including whitespace (kept verbatim) and quotes (`'`/`"` are
/// literal) — is literal text. A `` `…` `` is literal too: backticks are
/// top-level `$( )` sugar with no meaning inside quotes (DG 2026-08-28), and
/// this is a quotes body. Any byte can be encoded via a `$'…'` C-string (e.g.
/// `$'a\x7db'` → `a}b`). Yields an `ExprKind::DoubleQuoted`, identical to a
/// real double-quoted word. Empty → empty literal.
///
/// This used to parse the `${var:-WORD}` default as well, which is where the
/// name came from. It no longer does — a disposition default is a literal
/// (see [`parse_disposition_literal`]) — so the only caller left is the `:s/`
/// sed operator's pattern and substitution.
///
/// `:s/` is deliberately **not** in scope for the literal rule. The question
/// that rule answers is "when is a conditionally-taken operand evaluated, and
/// what may it do"; a sed pattern is evaluated unconditionally, every time, so
/// it never raises it.
fn parse_interpolating_word(s: &str, span: Span, origin: Origin) -> Result<Box<Expr>, Diagnostic> {
    let mut scanner = Scanner::with_origin(s, origin);
    let parts = scanner.scan_default_body();
    let exprs = string_parts_to_exprs(&parts, span, &scanner);
    let exprs = with_scanner_fatal(&scanner, exprs)?;
    Ok(Box::new(Expr {
        kind: ExprKind::DoubleQuoted(exprs),
        span,
    }))
}

fn parse_subscript_operator(name: &str, after: &str, span: Span) -> Option<Expr> {
    let (op, operand_str) = split_operator(after);
    let op_str = op?;
    if !matches!(
        op_str.as_str(),
        "/" | "//" | "#" | "##" | "%" | "%%" | "^" | "^^" | "," | ",,"
    ) {
        return None;
    }
    // DD#65: the per-element array modifiers `${arr[@]OP}` are dropped — they
    // reject at parse (SB4034, operator `arrvals_<op>`). No glob→regex lowering:
    // the operand is never used because the operator is rejected before codegen.
    let operand = operand_str.map(|s| {
        Box::new(Expr {
            kind: ExprKind::Literal(s),
            span,
        })
    });
    Some(Expr {
        kind: ExprKind::ParameterExpansion(ParameterExpansion {
            name: name.to_string(),
            operator: Some(format!("arrvals_{}", op_str)),
            operand,
            index: None,
            taint_off: false,
        }),
        span,
    })
}

// ── Operator splitting ─────────────────────────────────────────────

fn split_operator(s: &str) -> (Option<String>, Option<String>) {
    // Two-char ops first (`:-` before `-`, `##` before `#`, …) so the longer
    // operator wins. Both tiers share `match_leading_operator`.
    let two_char_ops = &[
        ":-", ":=", ":+", ":?", ":~", "##", "%%", "//", "/#", "/%", "^^", ",,",
    ];
    let one_char_ops = &["%", "#", "/", "^", ",", "-", "+", "=", "?"];
    match_leading_operator(s, two_char_ops)
        .or_else(|| match_leading_operator(s, one_char_ops))
        .unwrap_or((None, None))
}

/// The first `ops` entry that prefixes `s`, split into `(operator, operand)`
/// (operand `None` when nothing follows). `None` if no operator matches.
fn match_leading_operator(s: &str, ops: &[&str]) -> Option<(Option<String>, Option<String>)> {
    for op in ops {
        if let Some(rest) = s.strip_prefix(op) {
            let operand = (!rest.is_empty()).then(|| rest.to_string());
            return Some((Some(op.to_string()), operand));
        }
    }
    None
}

// ── Command substitution ───────────────────────────────────────────

/// Parse a command substitution after the opening $(
pub fn parse_command_substitution(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    if matches!(scanner.peek().token, Tok::Lt) {
        return parse_slurp_form(scanner, span);
    }

    let mut stmts = Vec::new();
    let mut closed = false;
    scanner.enter_cmd_sub();
    let shortcut = collect_cmd_sub_body(scanner, span, &mut stmts, &mut closed)
        .map_err(|d| rediagnose_stray(scanner, span, d));
    scanner.exit_cmd_sub();
    if let Some(e) = shortcut? {
        return Ok(e);
    }

    if !closed {
        return Err(unterminated_cmd_sub_diag(span));
    }

    Ok(Expr {
        kind: ExprKind::CommandSubstitution(stmts),
        span,
    })
}

/// Consume the body of a `$( … )`, pushing statements into `stmts`.
///
/// `Ok(Some(expr))` when a first-word shortcut (`$(status)`, `$(getcpid)`)
/// consumed the whole substitution and produced the expression itself.
/// `Ok(None)` when the body ended — and then `closed` says *how*: `true`
/// for a `)`, `false` for end of input, which the caller turns into
/// SB4057. Keeping the two apart is the whole point; the predecessor
/// broke out of this loop on either and could no longer tell.
fn collect_cmd_sub_body(
    scanner: &mut Scanner,
    span: Span,
    stmts: &mut Vec<Statement>,
    closed: &mut bool,
) -> Result<Option<Expr>, Diagnostic> {
    while !scanner.at_eof() {
        match cmd_sub_body_step(scanner, span, stmts)? {
            CmdSubStep::Closed => {
                *closed = true;
                return Ok(None);
            }
            CmdSubStep::Return(e) => return Ok(Some(e)),
            CmdSubStep::Skipped | CmdSubStep::Pushed => {}
        }
    }
    Ok(None)
}

/// A `$(` that is never closed. The substitution loop used to break at end
/// of input and hand back a well-formed node, so `echo $(echo hi` compiled
/// and ran with the missing `)` silently ignored, and `echo $(status`
/// emitted a bare command name that tripped a runtime assertion. The quoted
/// path has always reported this as SB4057; the unquoted one now agrees.
fn unterminated_cmd_sub_diag(span: Span) -> Diagnostic {
    parse_error(
        span,
        "SB4057",
        "unterminated `$(` — it is never closed before the end of the \
         source, so everything after it was read as part of it"
            .to_string(),
    )
}

/// A stray `}` or block keyword (SB4072) inside a `$(` that is never closed
/// is the `$(`'s fault, not the token's. In `if c {` / `x=$(cmd` / `}` the `}`
/// was written to close the `if`; it reads as stray only because the open
/// `$(` swallowed it, and naming it would send the reader to the wrong line.
/// So when the substitution cannot be closed from where that token stands,
/// the answer is SB4057 at the `$(`. Any other diagnostic passes through.
fn rediagnose_stray(scanner: &mut Scanner, span: Span, d: Diagnostic) -> Diagnostic {
    if d.code == "SB4072" && !skip_to_cmd_sub_close(scanner) {
        unterminated_cmd_sub_diag(span)
    } else {
        d
    }
}

/// Consume tokens through the `)` that closes the current `$(`, and report
/// whether there is one. It reads tokens, not bytes, so a `)` inside quotes,
/// a `${…}` or a comment does not count; a nested `( … )` or `$( … )` is
/// balanced by depth, and `(( … ))` / `$(( … ))` are skipped whole because
/// arithmetic pairs its own parentheses.
fn skip_to_cmd_sub_close(scanner: &mut Scanner) -> bool {
    let mut depth = 0usize;
    loop {
        if let Some(closed) = cmd_sub_close_step(scanner, &mut depth) {
            return closed;
        }
    }
}

/// One token of [`skip_to_cmd_sub_close`]'s scan: `Some(closed)` once the
/// answer is known, `None` to read on.
fn cmd_sub_close_step(scanner: &mut Scanner, depth: &mut usize) -> Option<bool> {
    match scanner.next().token {
        Tok::Eof => Some(false),
        Tok::DollarParen | Tok::LParen => {
            *depth += 1;
            None
        }
        Tok::DollarDblParen | Tok::DblParen => {
            (!super::arith::scan_arith_content(scanner).closed).then_some(false)
        }
        Tok::RParen if *depth == 0 => Some(true),
        Tok::RParen => {
            *depth -= 1;
            None
        }
        _ => None,
    }
}

/// Outcome of one iteration of [`collect_cmd_sub_body`]'s loop.
///
/// `Closed` and `Skipped` are here rather than in the loop so that the loop
/// holds exactly one `match`: the structural tokens and the statement step
/// are answered in one place and reported in one vocabulary.
enum CmdSubStep {
    /// A `)` ended the substitution; the caller records *how* it ended.
    Closed,
    /// A `;` or newline between statements — nothing to record.
    Skipped,
    /// First-word special shortcut consumed the whole $(...); caller returns this Expr.
    Return(Expr),
    /// A statement was appended; caller falls through to the next iteration.
    Pushed,
}

/// One iteration: consume a structural token, or parse one statement.
fn cmd_sub_body_step(
    scanner: &mut Scanner,
    span: Span,
    stmts: &mut Vec<Statement>,
) -> Result<CmdSubStep, Diagnostic> {
    let tok = scanner.peek().token.clone();
    match tok {
        Tok::RParen => {
            scanner.next();
            Ok(CmdSubStep::Closed)
        }
        Tok::Newline | Tok::Semi => {
            scanner.next();
            Ok(CmdSubStep::Skipped)
        }
        _ => parse_cmd_sub_step(scanner, span, &tok, stmts),
    }
}

fn parse_cmd_sub_step(
    scanner: &mut Scanner,
    span: Span,
    tok: &Tok,
    stmts: &mut Vec<Statement>,
) -> Result<CmdSubStep, Diagnostic> {
    if let Some(step) = try_first_word_shortcut(scanner, span, tok, stmts) {
        return Ok(step);
    }
    // The loop rules out EOF and cmd_sub_body_step consumes terminators.
    stmts.push(super::parser::parse_statement_here(scanner)?);
    Ok(CmdSubStep::Pushed)
}

/// Probe the first word (when no statements have been parsed yet) for one
/// of the bareword shortcuts (status / getcpid). Returns Some(step) when
/// a shortcut fired; None to fall through to parse_statement_here.
fn try_first_word_shortcut(
    scanner: &mut Scanner,
    span: Span,
    tok: &Tok,
    stmts: &[Statement],
) -> Option<CmdSubStep> {
    if !stmts.is_empty() {
        return None;
    }
    let Tok::Word(w) = tok else {
        return None;
    };
    try_special_word(scanner, span, w).map(CmdSubStep::Return)
}

/// `$(<file)` — the slurp form. Caller has just consumed `(`.
///
/// Yields a real `ExprKind::SlurpFile`. It previously desugared to
/// `$(cat file)` here, which is why a static binary containing `$(<f)`
/// carried a runtime dependency on `/usr/bin/cat`.
fn parse_slurp_form(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    scanner.next();
    let file_expr = parse_expr(scanner)?;
    expect_rparen(scanner, span)?;
    Ok(Expr {
        kind: ExprKind::SlurpFile(Box::new(file_expr)),
        span,
    })
}

/// Probe for the bareword stateful-slot substitutions `$(status)` and
/// `$(getcpid)` — DD#111, where `$?` / `$!` are the sugar and these are the
/// real thing. Consumes the head and the `)` and yields the intrinsic
/// expansion; returns `None` for every other spelling.
///
/// **Only the exact nullary form is claimed here, and the lookahead is what
/// enforces that.** `status` and `getcpid` with anything following are
/// ordinary commands, so declining lets `parse_statement_here` parse them and
/// every ordinary gate apply. Two hand-rolled `Command` reconstructions used
/// to live here instead, and both were wrong in the same way: they bypassed
/// `parse_command`, so SB4046 never saw the head, and a trailing redirect
/// reported something else entirely. `$(status 2>/dev/null)` drew SB4014
/// "expected expression" (its private arg loop fed `2>` to `parse_expr`),
/// and `$(getcpid 2>/dev/null)` drew SB5073 *"recognised only when it is the
/// whole substitution, not one statement among several"* — false when
/// written, and self-fulfilling, because `parse_getcpid_word` discarded the
/// rest of the command without consuming it, so `2>/dev/null` became the
/// second statement the message complained about. A private reconstruction
/// of a shared parse is how a diagnostic ends up describing a program nobody
/// wrote.
fn try_special_word(scanner: &mut Scanner, span: Span, w: &str) -> Option<Expr> {
    if !matches!(w, "status" | "getcpid") {
        return None;
    }
    if !matches!(scanner.peek_n(1).token, Tok::RParen) {
        return None;
    }
    scanner.next();
    scanner.next();
    Some(intrinsic_expansion(w, span))
}

fn expect_rparen(scanner: &mut Scanner, span: Span) -> Result<(), Diagnostic> {
    if matches!(scanner.peek().token, Tok::RParen) {
        scanner.next();
        Ok(())
    } else {
        Err(parse_error(span, "SB4014", "expected ')'".to_string()))
    }
}

// ── Array literals ─────────────────────────────────────────────────

/// Parse an array literal after = in assignment: =(a b c) or =([k1]=v1 [k2]=v2)
pub fn parse_array_literal(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    let mut elems = Vec::new();
    let mut assoc_pairs: Vec<(String, Expr)> = Vec::new();
    let mut is_assoc = false;

    loop {
        if array_literal_at_end(scanner) {
            break;
        }
        if let Some((key, val)) = try_parse_assoc_element(scanner)? {
            is_assoc = true;
            assoc_pairs.push((key, val));
            continue;
        }
        elems.push(parse_expr(scanner)?);
    }

    let kind = if is_assoc {
        ExprKind::AssocArray(assoc_pairs)
    } else {
        ExprKind::Array(elems)
    };
    Ok(Expr { kind, span })
}

/// Skip blank lines inside an `=(...)` literal and report whether the list is
/// finished: consumes and returns `true` at the closing `)` (or at EOF).
fn array_literal_at_end(scanner: &mut Scanner) -> bool {
    while matches!(scanner.peek().token, Tok::Newline) {
        scanner.next();
    }
    if matches!(scanner.peek().token, Tok::RParen) {
        scanner.next();
        return true;
    }
    scanner.at_eof()
}

/// Recognise an associative-array element at the scanner cursor.
/// Bash accepts two surface forms inside an `=(...)` literal:
/// `[key]=value` (the `]=` lives inside one Word token) and
/// `[key] = value` (the scanner already split on the `=`).
/// Returns `Ok(Some((key, value)))` if one matched and the cursor
/// has been advanced past the value; `Ok(None)` if the next token
/// is not an assoc element and the caller should parse it as a
/// plain element.
fn try_parse_assoc_element(scanner: &mut Scanner) -> Result<Option<(String, Expr)>, Diagnostic> {
    let tok = scanner.peek().token.clone();
    let Tok::Word(ref w) = tok else {
        return Ok(None);
    };
    if !w.starts_with('[') {
        return Ok(None);
    }

    if w.ends_with(']') && w.len() > 2 && matches!(scanner.peek_n(1).token, Tok::Eq) {
        let key = w[1..w.len() - 1].to_string();
        scanner.next(); // consume [key]
        scanner.next(); // consume =
        let val = parse_expr(scanner)?;
        return Ok(Some((key, val)));
    }

    Ok(None)
}

// ── Utilities ─────────────────────────────────────────────────────

/// Compute an ir::Span from a byte offset using the scanner.
pub fn span_from(scanner: &Scanner, offset: usize) -> Span {
    let (line, col) = scanner.line_col(offset);
    Span { line, col }
}

/// The source spelling of a reserved word, or `None` if `tok` is not one.
///
/// This is the **only** list of reserved-word tokens in the word parser.
/// `literal_token_text` consults it rather than repeating the set; it used to
/// repeat it, and the two drifted. `Tok::Str` was missing from both, so the
/// bare word `str` was a parse error in *every* word context â command
/// argument, assignment value, `[[ ]]` operand, `for` list â while all 21 of
/// its siblings lowered to a literal. Keep this list and
/// `lexer::classify_word_token`'s promotion table in step; `spec/coverage.sh`
/// cov.299-305 pin every spelling end-to-end (journal 347).
pub(super) fn keyword_to_str(tok: &Tok) -> Option<&'static str> {
    Some(match tok {
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
        Tok::Str => "str",
        Tok::Int => "int",
        Tok::Const => "const",
        Tok::Array => "array",
        Tok::Map => "map",
        Tok::Attest => "attest",
        Tok::End => "END",
        Tok::Exit => "EXIT",
        Tok::Err => "ERR",
        _ => return None,
    })
}

/// Both arms below are dispatched *by token* from `parse_expr`'s word loop,
/// which names the tokens it routes to each one. The `unreachable!` is the
/// claim that the loop and the helper agree about that set; a unit test is
/// the only thing that can hold them to it, because no source text can ever
/// put a mismatched token here. DG's rule for the coverage campaign: an
/// unreachable assertion earns a unit test proving it catches what it claims.
#[cfg(test)]
mod invariant_tests {
    use super::*;

    #[test]
    fn balanced_close_supports_the_parenthesis_entry() {
        // Source callers enter on `[` or `{`; the shared scanner also accepts `(`.
        assert_eq!(balanced_close(br#"(x [y] "{z}") tail"#, 0), Some(12));
    }

    #[test]
    fn balanced_close_declines_an_invalid_opener() {
        assert_eq!(balanced_close(b"x", 0), None);
    }

    #[test]
    #[should_panic(expected = "non-sed cell operators produce parameter expansions")]
    fn attach_cell_index_rejects_a_non_parameter_result() {
        let span = Span::new(1, 1);
        let mut expr = Expr::literal("not a parameter", span);
        attach_cell_index(&mut expr, Box::new(Expr::literal("0", span)));
    }

    #[test]
    #[should_panic(
        expected = "disposition parsers produce parameter expansions for the trust modifier"
    )]
    fn trusted_rejects_a_non_parameter_result() {
        let _ = trusted(Expr::literal("not a parameter", Span::new(1, 1)));
    }

    #[test]
    #[should_panic(expected = "parse_quoted_arm called with non-quoted token")]
    fn parse_quoted_arm_rejects_a_token_it_is_not_routed() {
        let mut scanner = Scanner::new("x");
        let mut parts = Vec::new();
        let _ = parse_quoted_arm(
            &mut scanner,
            &Tok::Word("x".to_string()),
            Span::new(1, 1),
            &mut parts,
        );
    }

    #[test]
    #[should_panic(expected = "parse_expansion_arm called with non-expansion token")]
    fn parse_expansion_arm_rejects_a_token_it_is_not_routed() {
        let mut scanner = Scanner::new("x");
        let mut parts = Vec::new();
        let _ = parse_expansion_arm(
            &mut scanner,
            &Tok::Word("x".to_string()),
            Span::new(1, 1),
            0,
            &mut parts,
        );
    }
}
