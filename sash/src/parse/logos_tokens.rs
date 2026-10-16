//! Logos-driven recogniser for the "structural" subset of sash tokens.
//!
//! Integration: `lexer::Scanner::next_token` dispatches on the first byte
//! and delegates the structural-operator family — `\n`, `#`-comment,
//! `;`/`;;`, `|`/`||`, `&`/`&&`/`&>`, `(`/`((`, `)`/`))`, `{`, `}`,
//! `>`/`>>`/`>&` — to `scan_via_logos`.
//!
//! `LOGOS_DISPATCH_INVARIANT` states the contract exactly: logos is only
//! ever handed a slice whose first byte is one of ten, and it matches
//! from position 0 — so every variant below begins with one of those
//! bytes, and a pattern beginning with anything else could never fire.
//! Everything else — strings, expansions, words/keywords, heredoc,
//! dash/punct disambiguation, `[[`/`]]`, `<<<`, `+=`, `!` — stays
//! hand-rolled because it needs stateful scanning or word-boundary
//! semantics the surrounding tokeniser already owns, and so never
//! reaches this module at all.
//!
//! This file once carried 27 further variants: the 22 keywords plus
//! `[[`, `]]`, `<<<`, `+=` and `!`. They were retained as guard rails
//! against a possible future migration of keyword promotion under
//! logos, but none of them could ever match — a keyword starts with a
//! letter, which the dispatcher routes to `scan_word` and thence to
//! `classify_word_token`, and the five operators start with bytes that
//! have their own hand-rolled scanners. That made the keyword list a
//! second, silent answer to "what is a keyword?" sitting beside
//! `classify_word_token`'s real one and free to drift out of step with
//! it. Deleted 2026-09-16 (journal 346); `spec/coverage.sh` cov.284-298
//! pin that the hand-rolled paths still lex every one of them.
//!
//! Equivalence is verified by `logos_equivalence` tests in `tests.rs`.

use logos::Logos;

/// Why every `LogosTok` variant below starts with one of ten bytes.
///
/// Cited by `to_tok`'s `Comment` arm, which is unreachable for the
/// narrower reason recorded there.
pub const LOGOS_DISPATCH_INVARIANT: &str =
    "logos is only ever handed a slice beginning with one of the ten bytes \
     Scanner::next_token dispatches to it (newline # ; | & ( ) { } >), and it \
     matches from position 0, so a pattern beginning with any other byte can \
     never fire";

/// Structural tokens recognised by logos. One-to-one with the
/// payload-free variants of `lexer::Tok`.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
#[logos(skip r"[ \t]+")]
#[logos(skip r"\\\n")]
pub enum LogosTok {
    // ── Multi-char operators (longest match first via priority) ──
    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,
    #[token("&>")]
    AmpGt,
    #[token("((")]
    DblParen,
    #[token(";;")]
    DblSemi,
    #[token(">>")]
    DblGt,
    #[token(">&")]
    GtAmp,

    // ── Single-char operators ──
    #[token("|")]
    Pipe,
    #[token("&")]
    Amp,
    #[token(";")]
    Semi,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(">")]
    Gt,

    // ── Statement terminator ──
    #[token("\n")]
    Newline,

    // ── Comments: # to end-of-line ──
    #[regex(r"#[^\n]*", allow_greedy = true)]
    Comment,
}

/// Helper for the integration shim: convert a successfully-matched
/// `LogosTok` to the corresponding payload-free `lexer::Tok`.
///
/// Total, because every variant that reaches here carries no payload.
/// `Comment` is the one variant that does, and `emit_logos_token`
/// intercepts it *before* calling this — it needs the matched slice, so
/// it cannot go through a payload-free mapping. This function therefore
/// never sees one, which is why the arm is an ICE and why the return
/// type is `Tok` rather than `Option<Tok>`. It returned an `Option` for
/// exactly that arm until 2026-09-16, and the sole caller answered it
/// with `.expect()`.
///
/// **Total over `LogosTok` on purpose — do not add a wildcard.**
pub fn to_tok(t: LogosTok) -> super::lexer::Tok {
    use super::lexer::Tok;
    match t {
        LogosTok::AndAnd => Tok::AndAnd,
        LogosTok::OrOr => Tok::OrOr,
        LogosTok::AmpGt => Tok::AmpGt,
        LogosTok::DblParen => Tok::DblParen,
        LogosTok::DblSemi => Tok::DblSemi,
        LogosTok::DblGt => Tok::DblGt,
        LogosTok::GtAmp => Tok::GtAmp,
        LogosTok::Pipe => Tok::Pipe,
        LogosTok::Amp => Tok::Amp,
        LogosTok::Semi => Tok::Semi,
        LogosTok::LParen => Tok::LParen,
        LogosTok::RParen => Tok::RParen,
        LogosTok::LBrace => Tok::LBrace,
        LogosTok::RBrace => Tok::RBrace,
        LogosTok::Gt => Tok::Gt,
        LogosTok::Newline => Tok::Newline,
        LogosTok::Comment => unreachable!(
            "emit_logos_token intercepts LogosTok::Comment before to_tok to \
             extract its slice; reaching here means that interception was \
             removed and a comment is about to lose its payload ({})",
            LOGOS_DISPATCH_INVARIANT
        ),
    }
}
