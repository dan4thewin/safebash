//! Logos-driven recogniser for the "structural" subset of sash tokens.
//!
//! Integration: `lexer::Scanner::next_token` dispatches on the first byte
//! and delegates the structural-operator family — `\n`, `#`-comment,
//! `;`/`;;`/`;&`/`;;&`, `|`/`||`, `&`/`&&`/`&>`, `(`/`((`, `)`/`))`,
//! `{`, `}`, `>`/`>>`/`>&` — to `scan_via_logos`. Everything else
//! (strings, expansions, words/keywords, heredoc, dash/punct
//! disambiguation, `[[`/`]]`, `<<<`, `+=`, `!`) stays hand-rolled
//! because it needs stateful scanning or word-boundary semantics that
//! the surrounding tokeniser already owns.
//!
//! The keyword variants below are not reached through the dispatcher
//! (their first byte is a letter, which routes to `scan_word`'s
//! keyword promotion table) but are retained because the equivalence
//! tests pin logos' longest-match behaviour for them — useful guard
//! rails if we ever move keyword promotion under logos.
//!
//! Equivalence is verified by `logos_equivalence` tests in `tests.rs`.

use logos::Logos;

/// Structural tokens recognised by logos. One-to-one with the
/// payload-free variants of `lexer::Tok`.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
#[logos(skip r"[ \t]+")]
#[logos(skip r"\\\n")]
pub enum LogosTok {
    // ── Keywords (require word-boundary on the trailing side) ──
    #[token("if")]
    If,
    #[token("then")]
    Then,
    #[token("else")]
    Else,
    #[token("elif")]
    Elif,
    #[token("fi")]
    Fi,
    #[token("while")]
    While,
    #[token("until")]
    Until,
    #[token("do")]
    Do,
    #[token("done")]
    Done,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("case")]
    Case,
    #[token("esac")]
    Esac,
    #[token("fn")]
    Fn,
    #[token("function")]
    Function,
    #[token("local")]
    Local,
    #[token("int")]
    Int,
    #[token("const")]
    Const,
    #[token("array")]
    Array,
    #[token("map")]
    Map,
    #[token("bless")]
    Bless,
    #[token("END")]
    End,

    // ── Multi-char operators (longest match first via priority) ──
    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,
    #[token("&>")]
    AmpGt,
    #[token("((")]
    DblParen,
    #[token("))")]
    DblParenClose,
    #[token("[[")]
    DblBracketOpen,
    #[token("]]")]
    DblBracketClose,
    #[token(";;")]
    DblSemi,
    #[token(">>")]
    DblGt,
    #[token(">&")]
    GtAmp,
    #[token("<<<")]
    HereString,
    #[token("+=")]
    PlusAssign,

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
    #[token("!")]
    Bang,
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
/// `LogosTok` to the corresponding payload-free `lexer::Tok`. Returns
/// `None` for `Comment` (caller extracts the slice).
pub fn to_tok(t: LogosTok) -> Option<super::lexer::Tok> {
    use super::lexer::Tok;
    Some(match t {
        LogosTok::If => Tok::If,
        LogosTok::Then => Tok::Then,
        LogosTok::Else => Tok::Else,
        LogosTok::Elif => Tok::Elif,
        LogosTok::Fi => Tok::Fi,
        LogosTok::While => Tok::While,
        LogosTok::Until => Tok::Until,
        LogosTok::Do => Tok::Do,
        LogosTok::Done => Tok::Done,
        LogosTok::For => Tok::For,
        LogosTok::In => Tok::In,
        LogosTok::Case => Tok::Case,
        LogosTok::Esac => Tok::Esac,
        LogosTok::Fn => Tok::Fn,
        LogosTok::Function => Tok::Function,
        LogosTok::Local => Tok::Local,
        LogosTok::Int => Tok::Int,
        LogosTok::Const => Tok::Const,
        LogosTok::Array => Tok::Array,
        LogosTok::Map => Tok::Map,
        LogosTok::Bless => Tok::Bless,
        LogosTok::End => Tok::End,
        LogosTok::AndAnd => Tok::AndAnd,
        LogosTok::OrOr => Tok::OrOr,
        LogosTok::AmpGt => Tok::AmpGt,
        LogosTok::DblParen => Tok::DblParen,
        LogosTok::DblParenClose => Tok::DblParenClose,
        LogosTok::DblBracketOpen => Tok::DblBracketOpen,
        LogosTok::DblBracketClose => Tok::DblBracketClose,
        LogosTok::DblSemi => Tok::DblSemi,
        LogosTok::DblGt => Tok::DblGt,
        LogosTok::GtAmp => Tok::GtAmp,
        LogosTok::HereString => Tok::HereString,
        LogosTok::PlusAssign => Tok::PlusAssign,
        LogosTok::Pipe => Tok::Pipe,
        LogosTok::Amp => Tok::Amp,
        LogosTok::Semi => Tok::Semi,
        LogosTok::LParen => Tok::LParen,
        LogosTok::RParen => Tok::RParen,
        LogosTok::LBrace => Tok::LBrace,
        LogosTok::RBrace => Tok::RBrace,
        LogosTok::Bang => Tok::Bang,
        LogosTok::Gt => Tok::Gt,
        LogosTok::Newline => Tok::Newline,
        LogosTok::Comment => return None,
    })
}
