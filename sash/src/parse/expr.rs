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
use super::lexer::{Scanner, StringPart, Tok};
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
/// `Dollar`/`DollarBrace`/`DollarParen`/`Backtick`/`DollarDblParen`.
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
            scanner.next();
            parts.push(parse_parameter_expansion(content, span)?);
        }
        Tok::DollarParen => {
            scanner.next();
            parts.push(parse_command_substitution(scanner, span)?);
        }
        Tok::Backtick(content) => {
            scanner.next();
            parts.push(command_sub_to_expr(content, span));
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
                parts.push(parse_word_arm(scanner, w, span)?);
            }
            Tok::SingleQuoted(_) | Tok::DoubleQuoted(_) | Tok::AnsiCQuoted(_) => {
                parse_quoted_arm(scanner, &tok, span, &mut parts)?;
            }
            Tok::Dollar(_)
            | Tok::DollarBrace(_)
            | Tok::DollarParen
            | Tok::Backtick(_)
            | Tok::DollarDblParen => {
                parse_expansion_arm(scanner, &tok, span, start, &mut parts)?;
            }
            _ if literal_token_text(&tok, mode).is_some() => {
                let text = literal_token_text(&tok, mode).unwrap();
                scanner.next();
                parts.push(Expr {
                    kind: ExprKind::Literal(text),
                    span,
                });
            }
            _ => break,
        }
    }

    parts_to_expr(parts)
}

/// Text of a token that lowers to a literal word part: `=`, `-`, the
/// mode-dependent absorbable metacharacters, and reserved words used as
/// bare arguments. `None` for tokens handled by their own match arm.
fn literal_token_text(tok: &Tok, mode: AbsorbMode) -> Option<String> {
    match tok {
        Tok::Eq => Some("=".to_string()),
        Tok::Dash => Some("-".to_string()),
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
        | Tok::Int
        | Tok::Const
        | Tok::Array
        | Tok::Map
        | Tok::Attest
        | Tok::End => Some(keyword_to_str(tok).to_string()),
        _ => absorb_text(tok, mode).map(str::to_string),
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
fn parse_at_brace_disposition(w: &str, span: Span) -> Result<Expr, Diagnostic> {
    let inner = w
        .strip_prefix("@{")
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| splat_disposition_malformed(span))?;
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
        return Ok(("arrvals_default", Some(parse_default_word(rest, span)?)));
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

/// `@{a:?[WORD]}` operand: the WORD if present, else `None` (bare `@{a:?}`
/// aborts with no message word).
fn optional_splat_word(rest: &str, span: Span) -> Result<Option<Box<Expr>>, Diagnostic> {
    if rest.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parse_default_word(rest, span)?))
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

/// `Tok::Dollar(name)` arm — checks for `$arr[i]` subscript syntax
/// (DD#41), then falls back to a simple variable expansion. Consumes
/// the `$name` token (and the `[i]rest` word when subscript is
/// recognised), and pushes one or two parts onto `parts`. `$LINENO`
/// gets no special handling — it flows through as an ordinary
/// expansion, so an unassigned `$LINENO` is rejected as an undefined
/// variable like any other (no magic vars).
fn parse_dollar_arm(
    scanner: &mut Scanner,
    name: String,
    span: Span,
    _start: usize,
    parts: &mut Vec<Expr>,
) {
    scanner.next();
    if try_subscript_arm(scanner, &name, span, parts) {
        return;
    }
    parts.push(Expr {
        kind: ExprKind::SimpleExpansion(name),
        span,
    });
}

/// Probe for `$arr[i]` / `$arr[@]` / `$arr[*]` subscript form. The
/// caller has already consumed `$name`. Returns true (and pushes the
/// subscript part plus any trailing literal text) if the next token
/// is a word beginning with `[` and containing `]`.
fn try_subscript_arm(scanner: &mut Scanner, name: &str, span: Span, parts: &mut Vec<Expr>) -> bool {
    let Tok::Word(ref w) = scanner.peek().token.clone() else {
        return false;
    };
    if !w.starts_with('[') {
        return false;
    }
    let Some(close) = w.find(']') else {
        // The lexer split a dynamic subscript `[`, `$idx`, `]` into separate
        // tokens (DD#33 read side, decision B 2026-06-21). Gather them into a
        // Subscript read so `$a[$i]` matches `${a[$i]}`.
        return try_split_subscript_arm(scanner, name, span, parts);
    };
    let idx = w[1..close].to_string();
    let rest = w[close + 1..].to_string();
    scanner.next();
    if idx == "@" || idx == "*" {
        parts.push(Expr::param(name, Some("arrvals_retired"), None, span));
    } else {
        parts.push(Expr {
            kind: ExprKind::Subscript {
                name: name.to_string(),
                index: Box::new(subscript_index_expr(&idx, span)),
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
    true
}

/// Collapse the collected `parts` into a final `Expr`: empty → error,
/// single → unwrap, multiple → `Concat`.
fn parts_to_expr(parts: Vec<Expr>) -> Result<Expr, Diagnostic> {
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

/// Convert the next [StringPart] (and possibly the one after it,
/// for $arr[idx] lookahead) into one or more Exprs appended to
/// `result`. Returns the number of input parts consumed (1 or 2).
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
            // DD#41: detect $arr[i] / $arr[@] inside double quotes
            // by peeking at the next part for a [idx]suffix literal.
            if let Some(advance) =
                try_emit_array_subscript_in_string(parts_from_here, name, span, result)
            {
                return Ok(advance);
            }
            result.push(Expr {
                kind: ExprKind::SimpleExpansion(name.clone()),
                span,
            });
            1
        }
        StringPart::Expansion(content) => {
            result.push(parse_parameter_expansion(content, span)?);
            1
        }
        StringPart::CommandSub(content) => {
            result.push(command_sub_to_expr(content, span));
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

/// If `parts_from_here` starts with `$name` followed by a literal
/// of the shape `[idx]rest…`, push the corresponding subscript /
/// arrvals expansion (plus a literal for `rest…` if non-empty)
/// and return `Some(2)`. Otherwise return `None` and the caller
/// falls back to a plain `$name` expansion.
fn try_emit_array_subscript_in_string(
    parts_from_here: &[StringPart],
    name: &str,
    span: Span,
    result: &mut Vec<Expr>,
) -> Option<usize> {
    let StringPart::Literal(lit) = parts_from_here.get(1)? else {
        return None;
    };
    if !lit.starts_with('[') {
        return None;
    }
    let close = lit.find(']')?;
    let idx = &lit[1..close];
    let rest = &lit[close + 1..];
    result.push(subscript_expr(name, idx, span));
    if !rest.is_empty() {
        result.push(Expr {
            kind: ExprKind::Literal(rest.to_string()),
            span,
        });
    }
    Some(2)
}

/// Build the subscript / arrvals expansion for `$name[idx]`.
/// `idx` of `@` or `*` becomes arrvals; anything else becomes a
/// subscript whose index is parsed by `subscript_index_expr`.
fn subscript_expr(name: &str, idx: &str, span: Span) -> Expr {
    if idx == "@" || idx == "*" {
        Expr::param(name.to_string(), Some("arrvals_retired"), None, span)
    } else {
        Expr {
            kind: ExprKind::Subscript {
                name: name.to_string(),
                index: Box::new(subscript_index_expr(idx, span)),
            },
            span,
        }
    }
}

/// Build the index expression for a subscript `name[idx]`. A `$var` or
/// `${var}` index expands the variable at runtime (dynamic subscript,
/// DD#33): for an indexed array it yields the integer index, for a map
/// the string key. Any other text is a literal key (a bare integer for
/// indexed arrays, a bare string for maps).
/// Gather a lexer-split dynamic subscript read `name[$idx]` into a
/// `Subscript`. The current token is a bare `[` (the index `$var`/`${var}`
/// and the closing `]` were split into following tokens by the lexer).
/// Speculative: consumes nothing unless the full `[ $idx ]` shape is present.
fn try_split_subscript_arm(
    scanner: &mut Scanner,
    name: &str,
    span: Span,
    parts: &mut Vec<Expr>,
) -> bool {
    if !matches!(&scanner.peek().token, Tok::Word(w) if w == "[") {
        return false;
    }
    let idx_str = match &scanner.peek_n(1).token {
        Tok::Dollar(v) => format!("${}", v),
        Tok::DollarBrace(content) => format!("${{{}}}", content),
        _ => return false,
    };
    if !matches!(&scanner.peek_n(2).token, Tok::Word(w) if w == "]") {
        return false;
    }
    let Some(var) = dynamic_subscript_var(&idx_str) else {
        return false;
    };
    let index = Expr {
        kind: ExprKind::SimpleExpansion(var.to_string()),
        span,
    };
    scanner.next(); // [
    scanner.next(); // $idx
    scanner.next(); // ]
    parts.push(Expr {
        kind: ExprKind::Subscript {
            name: name.to_string(),
            index: Box::new(index),
        },
        span,
    });
    true
}

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

/// Backticked content gets preserved verbatim (legacy holdover);
/// modern `$(…)` content lowers to CommandSubstitution.
fn command_sub_to_expr(content: &str, span: Span) -> Expr {
    if content.starts_with('`') {
        return Expr {
            kind: ExprKind::Literal(content.to_string()),
            span,
        };
    }
    let stmts = parse_sub_script(content, span);
    if let Some(e) = bareword_shortcut_expansion(&stmts, span) {
        return e;
    }
    Expr {
        kind: ExprKind::CommandSubstitution(stmts),
        span,
    }
}

/// `$(status)` is sugar for `$?` and `$(getcpid)` for `$!` (and DG: `$?` is
/// *the* sugar, so the two spellings must agree everywhere).
///
/// The unquoted `$( )` path rewrites them while scanning, via
/// `try_special_word`. This path parses a content STRING instead, so it never
/// reached that probe: the substitution survived as a `Command` no backend
/// recognises as a builtin, was dispatched as an external, and the runtime's
/// "bare command passed to sash_exec" complaint was substituted into the
/// program's output with a 0 exit. Every compile-time check passed, because
/// `status` really is a registered builtin.
///
/// Applied to the parsed result rather than the raw text so the two paths
/// agree on what counts as the bare form (whitespace, `;`, a trailing
/// newline) instead of re-deciding it with a string compare.
fn bareword_shortcut_expansion(stmts: &[Statement], span: Span) -> Option<Expr> {
    let [stmt] = stmts else { return None };
    let StmtKind::Command(c) = &stmt.kind else {
        return None;
    };
    if !c.args.is_empty() {
        return None;
    }
    let ExprKind::Literal(name) = &c.name.kind else {
        return None;
    };
    let var = match name.as_str() {
        "status" => "?",
        "getcpid" => "!",
        _ => return None,
    };
    Some(Expr {
        kind: ExprKind::SimpleExpansion(var.to_string()),
        span,
    })
}

// ── Parameter expansion (${...}) ─────────────────────────────────
//
// Parses the content between ${ and }. Handles:
//   ${#var}         → strlen
//   ${!var[@]}      → arrkeys
//   ${var[idx]}     → subscript
//   ${var:s/re/r/}  → DD#39 sed expression
//   ${var:off:len}  → substring
//   ${var:-default} → standard operators
//   ${var/pat/rep}  → substitution (glob → regex)
//   ${var@X}        → rejected (SB4015)

/// Parse ${...} content into an IR ParameterExpansion.
pub fn parse_parameter_expansion(content: &str, span: Span) -> Result<Expr, Diagnostic> {
    let bytes = content.as_bytes();

    if let Some(e) = try_braced_splat(content, span) {
        return Ok(e);
    }
    if let Some(e) = try_strlen_or_arrlen(content, bytes, span) {
        return Ok(e);
    }
    if let Some(e) = try_indirection_or_arrkeys(content, bytes, span) {
        return Ok(e);
    }

    let (name, name_end) = scan_param_name(content);
    if name_end >= bytes.len() {
        return Ok(Expr::param(name, None, None, span));
    }

    if let Some(e) = try_subscript(content, name, name_end, span)? {
        return Ok(e);
    }

    let rest = &content[name_end..];

    if let Some(e) = parse_cell_operator(rest, name, span)? {
        return Ok(e);
    }

    // DG (2026-08-29): no ambiguity. Reaching here means there is trailing
    // content after the name that no parser recognised (name_end < len, so
    // `rest` is non-empty) — e.g. `${x&y}`, `${x:foo}`. It used to silently
    // degrade to a bare `${name}` (dropping `rest`). Carry the raw `rest` on a
    // sentinel `unrecognized` operator so `rejected_param_operator_diag` rejects
    // it (SB4047): the compiler must never emit code for input it doesn't fully
    // understand.
    Ok(Expr::param(
        name,
        Some("unrecognized"),
        Some(Box::new(Expr {
            kind: ExprKind::Literal(rest.to_string()),
            span,
        })),
        span,
    ))
}

/// The operator-suffixed `${name OP…}` forms: `:s/…` sed, `:off:len`
/// substring, `@X` transforms, and the standard `:-`/`:=`/`#`/`%`/… operators.
/// Returns `None` for a bare `${name}` (empty/unmatched `rest`).
fn parse_cell_operator(rest: &str, name: &str, span: Span) -> Result<Option<Expr>, Diagnostic> {
    if let Some(e) = try_sed_op(rest, name, span)?
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
fn try_braced_splat(content: &str, span: Span) -> Option<Expr> {
    match content.as_bytes().first() {
        Some(b'@') | Some(b'*') => Some(Expr {
            kind: ExprKind::SimpleExpansion(content[..1].to_string()),
            span,
        }),
        _ => None,
    }
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

/// `${var[idx]}` — `@`/`*` produce arrvals (or chained operator); literal
/// index produces a `Subscript` expression.
fn try_subscript(
    content: &str,
    name: &str,
    name_end: usize,
    span: Span,
) -> Result<Option<Expr>, Diagnostic> {
    if content.as_bytes()[name_end] != b'[' {
        return Ok(None);
    }
    // DD#80: capture the index balanced across `[] {} ()` and skipping quotes,
    // so a `]` inside a nested `${…}` / `$(( … ))` / quoted key does not close
    // the subscript early. `close` is the byte index of the matching `]`.
    let Some((idx, close)) = capture_balanced_subscript(content, name_end) else {
        return Ok(None);
    };
    let after_bracket = &content[close + 1..];

    if idx == "@" || idx == "*" {
        return Ok(Some(splat_subscript_expr(name, after_bracket, span)));
    }

    // DD#80: re-parse the index as a full expression so an explicit nested
    // context (`$var`, `${…}`, `$(…)`, `$(( … ))`, quoted string) works and
    // lowers to SSA temporaries like any other operand. An implicit/ambiguous
    // bare form (brackets/operators/mixed) parses to a `Literal`/`Concat` that
    // `validate::check_subscript_index` rejects (SB4041).
    let index = Box::new(parse_subscript_index(&idx, span));

    if after_bracket.is_empty() {
        return Ok(Some(Expr {
            kind: ExprKind::Subscript {
                name: name.to_string(),
                index,
            },
            span,
        }));
    }

    // `${a[i]OP}` — a trailing operator applies to the *cell* value. DG: a
    // string cell behaves like a string variable, so route the operator through
    // the same scalar parsers and attach the index. The `:-`/`:?` dispositions,
    // substring (`:o:l`), and sed (`:s/`) all lower for a cell (edge 4); the
    // pruned families (`#`/`%`/`/`/`@X`) reject on their operator string exactly
    // as for a scalar, and any other `:`-led op rejects as `cell_string_op`.
    Ok(Some(subscript_operator_expr(
        name,
        index,
        after_bracket,
        span,
    )?))
}

/// Capture the `[ … ]` subscript index that starts at `content[open] == '['`,
/// balanced across `[] {} ()` and skipping quoted spans, so a `]` inside a
/// nested `${…}` / `$(… )` / `$(( … ))` / `"…"` / `'…'` does not close the
/// subscript early (DD#80). Returns `(index text, byte index of the matching
/// `]`)`, or `None` if unbalanced.
fn capture_balanced_subscript(content: &str, open: usize) -> Option<(String, usize)> {
    let bytes = content.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        if advance_in_quote(&mut quote, b) {
            continue;
        }
        match b {
            b'"' | b'\'' => quote = Some(b),
            b'[' | b'{' | b'(' => depth += 1,
            b']' | b'}' | b')' if step_close_depth(&mut depth) => {
                // A balanced subscript closes on `]`; another closer at depth 0
                // means the brackets were mismatched.
                return (b == b']').then_some((content[open + 1..i].to_string(), i));
            }
            _ => {}
        }
    }
    None
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
/// nested contexts survive. A parse failure or a leftover tail (a mixed/bare
/// form the value parser cannot consume whole) falls back to a `Literal` — which
/// `validate::check_subscript_index` then rejects as ambiguous (SB4041).
fn parse_subscript_index(idx: &str, span: Span) -> Expr {
    let trimmed = idx.trim();
    // A bare dollar-subscript `$name[…]` is ambiguous as an index (DD#80): it
    // parses as a clean cell read, but the explicit cell-read index form is the
    // braced `${name[…]}`. Keep it a `Literal` so validate rejects it (SB4041)
    // rather than silently reading a cell.
    if !is_bare_dollar_subscript(trimmed) {
        let mut scanner = Scanner::new(trimmed);
        if let Ok(expr) = parse_expr(&mut scanner) {
            if scanner.at_eof() {
                return expr;
            }
        }
    }
    Expr {
        kind: ExprKind::Literal(trimmed.to_string()),
        span,
    }
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
/// scalar form so `#`/`%`/`/`/`@X` reject identically.
fn subscript_operator_expr(
    name: &str,
    index: Box<Expr>,
    after: &str,
    span: Span,
) -> Result<Expr, Diagnostic> {
    // Sed on a cell (`${a[i]:s/RE/SUB/}`) desugars to `$(sub|gsub "${a[i]}" RE
    // SUB)` — the cell read as the subject — reusing the scalar `$(sub)`
    // machinery (edge 4). A cell behaves like a string variable (DG). It is the
    // one family that *consumes* the index (as the subject) rather than
    // attaching it, which is why it cannot share the tail below.
    if let Some(body) = after.strip_prefix(":s/") {
        let subject = quoted_cell_expr(name, index, span);
        return build_sed_or_malformed(body, subject, name, after, span);
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
    // DD#84: `:~` (elide) joins `:-`/`:?` as a cell disposition.
    let is_disposition =
        after.starts_with(":-") || after.starts_with(":?") || after.starts_with(":~");
    if is_disposition {
        return Ok(scalar_op_or_bare(
            parse_standard_operator(after, name, span)?,
            name,
            span,
        ));
    }
    if after.starts_with(':') {
        // substring (`:o:l`) on a cell — carry the cell index; the transform
        // builds a `Substring` node that reads the cell as the source (edge 4).
        // A cell behaves like a string variable (DG). Any other `:`-led op that
        // isn't substring/sed/`:-`/`:?` has no cell lowering — reject
        // explicitly (SB4039) rather than silently drop.
        return Ok(try_substring_op(after, name, span)
            .unwrap_or_else(|| Expr::param(name, Some("cell_string_op"), None, span)));
    }
    // `#`/`##`/`%`/`%%`/`/`-family/`@X`: parse as the scalar operator so the
    // same reject fires; the index rides along but the operator rejects first.
    if let Some(expr) = try_at_transform(after, name, span) {
        return Ok(expr);
    }
    Ok(scalar_op_or_bare(
        parse_standard_operator(after, name, span)?,
        name,
        span,
    ))
}

/// A parsed scalar operator, or a bare `${name}` when `rest` matched none.
fn scalar_op_or_bare(parsed: Option<Expr>, name: &str, span: Span) -> Expr {
    parsed.unwrap_or_else(|| Expr::param(name, None, None, span))
}

/// Attach a cell subscript to a freshly-parsed `${name OP}` expansion, turning
/// it into `${name[index] OP}`. A non-`ParameterExpansion` (never produced by
/// the callers here) is left unchanged.
fn attach_cell_index(expr: &mut Expr, index: Box<Expr>) {
    if let ExprKind::ParameterExpansion(pe) = &mut expr.kind {
        pe.index = Some(index);
    }
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
fn try_sed_op(rest: &str, name: &str, span: Span) -> Result<Option<Expr>, Diagnostic> {
    let Some(body) = rest.strip_prefix(":s/") else {
        return Ok(None);
    };
    let subject = quoted_var_expr(name, span);
    Ok(Some(build_sed_or_malformed(
        body, subject, name, rest, span,
    )?))
}

/// Build `$(sub|gsub SUBJECT RE SUB)` from a `:s/RE/SUB/` payload with the given
/// interpolating `subject` (a scalar `"$var"` or a cell `"${a[i]}"`), or the
/// `s_malformed` sentinel (→ SB4032) when a `/` delimiter is missing. Shared by
/// the scalar and cell sed forms.
fn build_sed_or_malformed(
    body: &str,
    subject: Expr,
    name: &str,
    rest: &str,
    span: Span,
) -> Result<Expr, Diagnostic> {
    if let Some(expr) = build_sed_sub(body, subject, span)? {
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
    Expr {
        kind: ExprKind::DoubleQuoted(vec![Expr {
            kind: ExprKind::SimpleExpansion(name.to_string()),
            span,
        }]),
        span,
    }
}

/// Split an `RE/SUB/flags` payload (the text after the leading `s/`) into
/// its three fields. Returns `None` when either mandatory `/` delimiter is
/// absent (a malformed substitution).
pub(crate) fn split_sed_fields(body: &str) -> Option<(&str, &str, &str)> {
    let (re, sub, flags, _) = split_sed_edit(body)?;
    Some((re, sub, flags))
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
pub(crate) fn build_sed_sub(
    body: &str,
    subject: Expr,
    span: Span,
) -> Result<Option<Expr>, Diagnostic> {
    let mut value = subject;
    let mut rest = body;
    loop {
        let Some((re, sub, flags, after)) = split_sed_edit(rest) else {
            return Ok(None);
        };
        value = build_one_sed_sub(re, sub, flags, value, span)?;
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
/// `gsub`, so a flag binds to its edit rather than the whole chain.
fn build_one_sed_sub(
    re: &str,
    sub: &str,
    flags: &str,
    subject: Expr,
    span: Span,
) -> Result<Expr, Diagnostic> {
    let cmd_name = if flags.contains('g') { "gsub" } else { "sub" };
    let re_expr = *parse_default_word(re, span)?;
    let sub_expr = *parse_default_word(sub, span)?;
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
fn try_at_transform(rest: &str, name: &str, span: Span) -> Option<Expr> {
    let at_pos = rest.rfind('@')?;
    let after = &rest[at_pos + 1..];
    if after.len() == 1
        && after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && !rest.contains("[@]")
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
    if let ExprKind::ParameterExpansion(pe) = &mut e.kind {
        pe.taint_off = true;
    }
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
    // DD#65: the `${var:-WORD}` default is a full interpolating word.
    let operand = if op_str == ":-" {
        operand_str
            .map(|s| parse_default_word(&s, span))
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

/// DD#65: the `${var:-WORD}` default is a double-quoted-string body whose
/// terminator is the closing `}` (the brace scanner already stripped it), so
/// the default is *exactly* a `"…"`: `$var`/`${…}`/`$(…)`/`$((…))`/`$'…'`/
/// `` `…` `` interpolate; every other byte — including whitespace (kept
/// verbatim) and quotes (`'`/`"` are literal) — is literal text. Any byte can
/// still be encoded via a `$'…'` C-string (e.g. `$'a\x7db'` → `a}b`). The
/// result is an `ExprKind::DoubleQuoted`, identical to a real double-quoted
/// word. Empty → empty literal.
fn parse_default_word(s: &str, span: Span) -> Result<Box<Expr>, Diagnostic> {
    let mut scanner = Scanner::new(s);
    let parts = scanner.scan_default_body();
    let exprs = string_parts_to_exprs(&parts, span, &scanner)?;
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

/// Parse a sub-script (for command substitution or subshell contents).
pub fn parse_sub_script(content: &str, _span: Span) -> Vec<Statement> {
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
pub fn parse_command_substitution(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    if matches!(scanner.peek().token, Tok::Lt) {
        return parse_slurp_form(scanner, span);
    }

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
            _ => match parse_cmd_sub_step(scanner, span, &tok, &mut stmts)? {
                CmdSubStep::Return(e) => return Ok(e),
                CmdSubStep::Continue => continue,
                CmdSubStep::Break => break,
                CmdSubStep::Pushed => {}
            },
        }
    }

    Ok(Expr {
        kind: ExprKind::CommandSubstitution(stmts),
        span,
    })
}

/// Outcome of one normal-token iteration inside parse_command_substitution.
enum CmdSubStep {
    /// First-word special shortcut consumed the whole $(...); caller returns this Expr.
    Return(Expr),
    /// First-word special shortcut pushed a stmt; caller continues the outer loop.
    Continue,
    /// parse_statement returned None; caller breaks out of the loop.
    Break,
    /// parse_statement returned Some; caller falls through to the next iteration.
    Pushed,
}

fn parse_cmd_sub_step(
    scanner: &mut Scanner,
    span: Span,
    tok: &Tok,
    stmts: &mut Vec<Statement>,
) -> Result<CmdSubStep, Diagnostic> {
    if let Some(step) = try_first_word_shortcut(scanner, span, tok, stmts)? {
        return Ok(step);
    }
    match super::parser::parse_statement(scanner)? {
        Some(stmt) => {
            stmts.push(stmt);
            Ok(CmdSubStep::Pushed)
        }
        None => Ok(CmdSubStep::Break),
    }
}

/// Probe the first word (when no statements have been parsed yet) for one
/// of the bareword shortcuts (status / getcpid). Returns Some(step) when
/// a shortcut fired; None to fall through to parse_statement.
fn try_first_word_shortcut(
    scanner: &mut Scanner,
    span: Span,
    tok: &Tok,
    stmts: &mut Vec<Statement>,
) -> Result<Option<CmdSubStep>, Diagnostic> {
    if !stmts.is_empty() {
        return Ok(None);
    }
    let Tok::Word(w) = tok else {
        return Ok(None);
    };
    match try_special_word(scanner, span, w)? {
        Some(SpecialWord::Returned(e)) => Ok(Some(CmdSubStep::Return(e))),
        Some(SpecialWord::Continued(s)) => {
            stmts.push(s);
            Ok(Some(CmdSubStep::Continue))
        }
        None => Ok(None),
    }
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

/// Outcome of probing the first word of a `$(…)` substitution for
/// one of the bareword shortcuts (`status`, `getcpid`).
enum SpecialWord {
    /// Already consumed the entire substitution; return this `Expr`.
    Returned(Expr),
    /// Pushed a command statement; caller should `continue` the loop.
    Continued(Statement),
}

/// Probe for bareword-shortcut command substitutions:
/// `$(status)` → `$?`, `$(status args…)` → status command,
/// `$(getcpid)` → `$!`, `$(getcpid)` (with no args allowed) →
/// getcpid command. Returns `Ok(None)` when `w` is not one of
/// these.
fn try_special_word(
    scanner: &mut Scanner,
    span: Span,
    w: &str,
) -> Result<Option<SpecialWord>, Diagnostic> {
    match w {
        "status" => parse_status_word(scanner, span).map(Some),
        "getcpid" => parse_getcpid_word(scanner, span).map(Some),
        _ => Ok(None),
    }
}

/// `$(status)` returns the `$?` expansion; `$(status args…)`
/// reconstructs as a regular `status` command (args parsed until
/// the next pipeline / list / close-paren terminator).
fn parse_status_word(scanner: &mut Scanner, span: Span) -> Result<SpecialWord, Diagnostic> {
    scanner.next();
    if matches!(scanner.peek().token, Tok::RParen) {
        scanner.next();
        return Ok(SpecialWord::Returned(Expr {
            kind: ExprKind::SimpleExpansion("?".to_string()),
            span,
        }));
    }
    let mut args = Vec::new();
    while !scanner.at_eof()
        && !matches!(
            scanner.peek().token,
            Tok::RParen | Tok::Newline | Tok::Semi | Tok::Pipe | Tok::AndAnd | Tok::OrOr
        )
    {
        args.push(parse_expr(scanner)?);
    }
    Ok(SpecialWord::Continued(Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("status".to_string()),
                span,
            },
            args,
        }),
        span,
    }))
}

/// `$(getcpid)` returns the `$!` expansion; anything else with the
/// `getcpid` head reconstructs as a no-arg `getcpid` command.
fn parse_getcpid_word(scanner: &mut Scanner, span: Span) -> Result<SpecialWord, Diagnostic> {
    scanner.next();
    if matches!(scanner.peek().token, Tok::RParen) {
        scanner.next();
        return Ok(SpecialWord::Returned(Expr {
            kind: ExprKind::SimpleExpansion("!".to_string()),
            span,
        }));
    }
    Ok(SpecialWord::Continued(Statement {
        kind: StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("getcpid".to_string()),
                span,
            },
            args: vec![],
        }),
        span,
    }))
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
        Tok::Int => "int",
        Tok::Const => "const",
        Tok::Array => "array",
        Tok::Map => "map",
        Tok::Attest => "attest",
        Tok::End => "END",
        _ => "",
    }
}
