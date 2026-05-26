//! Lexer for sash-native syntax.
//!
//! Hand-written scanner with adjacency tracking. Produces `SpannedToken`
//! on demand — no intermediate token array, no lookahead limit.
//!
//! Design:
//!   - Tokens are designed for sash-native (not bash): int, const, array, map,
//!     bless, signal, END, fn, local fn are first-class keywords.
//!   - Heredocs are rejected at the scanner level (emits __HEREDOC__ sentinel).
//!   - Backticks are rejected (emits `backtick` sentinel for error reporting).
//!   - Single brackets `[` are NOT operators — they appear only inside words
//!     (array subscripts, case patterns). Only `[[` is a token.
//!   - Adjacency tracking: `next_is_adjacent()` lets the parser decide whether
//!     to concatenate tokens (e.g., `$foo"bar"` → single Concat expression).
//!
//! SECURITY: The scanner never silently drops input. Every byte becomes part
//! of a token. Unknown sequences become Word tokens for the parser to reject.

// ── Source spans ─────────────────────────────────────────────────────

/// Source span for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LexSpan {
    pub start: usize, // byte offset
    pub end: usize,   // byte offset
}

/// A token with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Tok,
    pub span: LexSpan,
}

// ── Token types ──────────────────────────────────────────────────────

/// High-level token type. The scanner produces these directly from source bytes.
/// Grouped by category: keywords, operators, brackets, arithmetic, assignment,
/// redirects, case terminators, literals, expansions.
#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    // Keywords
    If,
    Then,
    Else,
    Elif,
    Fi,
    While,
    Until,
    Do,
    Done,
    For,
    In,
    Case,
    Esac,
    Fn,
    Function,
    Local,
    Int,
    Const,
    Array,
    Map,
    Bless,
    End,

    // Operators
    Pipe,
    AndAnd,
    OrOr,
    Amp,
    Semi,
    Newline,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Bang,
    Dash,

    // Brackets
    DblBracketOpen,
    DblBracketClose,

    // Arithmetic
    DollarDblParen, // $((
    DblParen,       // ((
    DblParenClose,  // ))

    // Assignment
    Eq,         // =
    PlusAssign, // +=

    // Redirects
    HereString, // <<<
    DblGt,      // >>
    GtAmp,      // >&
    LtAmp,      // <&
    LtGt,       // <>
    AmpGt,      // &>
    Gt,         // >
    Lt,         // <

    // Case
    DblSemiAmp, // ;;&
    SemiAmp,    // ;&
    DblSemi,    // ;;

    // Literals / values
    Word(String),
    SingleQuoted(String),          // content without quotes
    DoubleQuoted(Vec<StringPart>), // interpolated parts
    AnsiCQuoted(String),           // $'...' → expanded string

    // Expansions (produced by scanner)
    Dollar(String),      // $var, $1, $@, $#, $?, $$, $!
    DollarBrace(String), // ${...} raw content for sub-parsing
    DollarParen,         // $( — start of command substitution

    // Comments
    Comment(String),

    Eof,
}

impl Tok {
    /// Could this token be a word in command position?
    pub fn is_word_like(&self) -> bool {
        matches!(
            self,
            Tok::Word(_)
                | Tok::SingleQuoted(_)
                | Tok::DoubleQuoted(_)
                | Tok::AnsiCQuoted(_)
                | Tok::Dollar(_)
                | Tok::DollarBrace(_)
                | Tok::DollarParen
                | Tok::DollarDblParen
        )
    }
}

// ── String interpolation ─────────────────────────────────────────────

/// Part of an interpolated double-quoted string.
/// The scanner splits `"hello $name"` into [Literal("hello "), Variable("name")].
#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Literal(String),
    Variable(String),   // $var
    Expansion(String),  // ${...} raw content
    CommandSub(String), // $(...) raw content
    ArithSub(String),   // $((...)) raw content
}

// ── Scanner ──────────────────────────────────────────────────────────

/// Scanner: wraps the source and produces SpannedToken on demand.
///
/// Stateful: tracks byte position and last-consumed-end for adjacency.
/// Peek buffer (FIFO) allows unbounded lookahead without rewinding.
/// Handles strings, expansions, $(, $((, ((, and other multi-char tokens
/// that logos can't handle well due to overlapping prefixes.
pub struct Scanner {
    source: Vec<u8>,
    pos: usize,
    /// Peeked tokens (FIFO)
    peeked: Vec<SpannedToken>,
    /// Position after last consumed token (for adjacency checking)
    last_end: usize,
}

impl Scanner {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.as_bytes().to_vec(),
            pos: 0,
            peeked: Vec::new(),
            last_end: 0,
        }
    }

    /// Compute (line, col) from byte offset. Both 1-based.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for (i, &b) in self.source.iter().enumerate() {
            if i >= offset {
                break;
            }
            if b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Peek at the next token without consuming it.
    pub fn peek(&mut self) -> &SpannedToken {
        if self.peeked.is_empty() {
            let tok = self.next_token();
            self.peeked.push(tok);
        }
        &self.peeked[0]
    }

    /// Peek at the Nth token ahead (0-based).
    pub fn peek_n(&mut self, n: usize) -> &SpannedToken {
        while self.peeked.len() <= n {
            let tok = self.next_token();
            self.peeked.push(tok);
        }
        &self.peeked[n]
    }

    /// Consume and return the next token.
    pub fn next(&mut self) -> SpannedToken {
        let tok = if !self.peeked.is_empty() {
            self.peeked.remove(0)
        } else {
            self.next_token()
        };
        self.last_end = tok.span.end;
        tok
    }

    /// Check if the next token starts immediately after the last consumed token
    /// (no whitespace gap). Used to decide whether to concatenate.
    pub fn next_is_adjacent(&mut self) -> bool {
        let next_start = self.peek().span.start;
        next_start == self.last_end
    }

    /// Check if current position is at end of input.
    pub fn at_eof(&mut self) -> bool {
        self.peek().token == Tok::Eof
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.source.len() {
            match self.source[self.pos] {
                b' ' | b'\t' => self.pos += 1,
                b'\\' if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'\n' => {
                    // Line continuation
                    self.pos += 2;
                }
                _ => break,
            }
        }
    }

    // ── Token dispatch ───────────────────────────────────────────────
    //
    // Main tokenization: dispatch on first byte to produce the next token.
    // Longest-match: multi-char operators checked before single-char.

    fn next_token(&mut self) -> SpannedToken {
        self.skip_whitespace();
        let start = self.pos;

        if self.pos >= self.source.len() {
            return SpannedToken {
                token: Tok::Eof,
                span: LexSpan { start, end: start },
            };
        }

        let b = self.source[self.pos];

        match b {
            b'\n' => {
                self.pos += 1;
                SpannedToken {
                    token: Tok::Newline,
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }

            b'#' => {
                // Comment — consume until end of line
                let end = self.source[self.pos..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map(|p| self.pos + p)
                    .unwrap_or(self.source.len());
                let text = String::from_utf8_lossy(&self.source[self.pos..end]).to_string();
                self.pos = end;
                SpannedToken {
                    token: Tok::Comment(text),
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }

            b';' => {
                if self.pos + 2 < self.source.len()
                    && self.source[self.pos + 1] == b';'
                    && self.source[self.pos + 2] == b'&'
                {
                    self.pos += 3;
                    SpannedToken {
                        token: Tok::DblSemiAmp,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'&' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::SemiAmp,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b';' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::DblSemi,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::Semi,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }

            b'|' => {
                if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'|' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::OrOr,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::Pipe,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }

            b'&' => {
                if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'&' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::AndAnd,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'>' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::AmpGt,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::Amp,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }

            b'(' => {
                if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'(' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::DblParen,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::LParen,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }

            b')' => {
                if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b')' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::DblParenClose,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::RParen,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }

            b'{' => {
                self.pos += 1;
                SpannedToken {
                    token: Tok::LBrace,
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }

            b'}' => {
                self.pos += 1;
                SpannedToken {
                    token: Tok::RBrace,
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }

            b'!' => {
                if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'=' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::Word("!=".to_string()),
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::Bang,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }

            b'<' => {
                if self.starts_with(b"<<<") {
                    self.pos += 3;
                    SpannedToken {
                        token: Tok::HereString,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else if self.starts_with(b"<<") {
                    // Heredocs are not supported in sash
                    // Skip to end of line to avoid cascading errors
                    self.pos += 2;
                    if self.pos < self.source.len() && self.source[self.pos] == b'-' {
                        self.pos += 1;
                    }
                    // Skip delimiter word
                    while self.pos < self.source.len()
                        && !matches!(self.source[self.pos], b'\n' | b' ' | b'\t')
                    {
                        self.pos += 1;
                    }
                    // Emit as a special word that the validator catches
                    SpannedToken {
                        token: Tok::Word("__HEREDOC__".to_string()),
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else if self.starts_with(b"<&") {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::LtAmp,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else if self.starts_with(b"<>") {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::LtGt,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::Lt,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }

            b'>' => {
                if self.starts_with(b">&") {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::GtAmp,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else if self.starts_with(b">>") {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::DblGt,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::Gt,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }

            b'$' => self.scan_dollar(start),

            b'\'' => self.scan_single_quoted(start),

            b'"' => self.scan_double_quoted(start),

            b'`' => {
                // Backtick — rejected (SB5020)
                // Consume until matching backtick or EOL for better error
                self.pos += 1;
                while self.pos < self.source.len()
                    && self.source[self.pos] != b'`'
                    && self.source[self.pos] != b'\n'
                {
                    self.pos += 1;
                }
                if self.pos < self.source.len() && self.source[self.pos] == b'`' {
                    self.pos += 1;
                }
                SpannedToken {
                    token: Tok::Word("`backtick`".to_string()),
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }

            b'[' => {
                if self.starts_with(b"[[") {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::DblBracketOpen,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    // [ is part of a word (array subscripts, case patterns)
                    self.scan_word(start)
                }
            }

            b']' => {
                if self.starts_with(b"]]") {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::DblBracketClose,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.scan_word(start)
                }
            }

            b'=' => {
                if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'~' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::Word("=~".to_string()),
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'=' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::Word("==".to_string()),
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::Eq,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }

            b'+' => {
                if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'=' {
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::PlusAssign,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.scan_word(start)
                }
            }

            b'-' => {
                // Standalone dash (followed by whitespace/terminator) → Tok::Dash
                // Adjacent to alphanumeric (e.g. -n, -f) → part of word
                let next = if self.pos + 1 < self.source.len() {
                    self.source[self.pos + 1]
                } else {
                    b' ' // EOF counts as whitespace
                };
                if next == b' '
                    || next == b'\t'
                    || next == b'\n'
                    || next == b';'
                    || next == b'|'
                    || next == b'&'
                    || next == b')'
                    || next == b'}'
                    || next == b'#'
                {
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::Dash,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    self.scan_word(start)
                }
            }

            _ => self.scan_word(start),
        }
    }

    fn starts_with(&self, prefix: &[u8]) -> bool {
        self.source[self.pos..].starts_with(prefix)
    }

    // ── Dollar expansions ─────────────────────────────────────────────

    /// Scan $...: $var, ${...}, $((, $(, $1, $@, $#, $?, $$, $!, $'...', $0
    fn scan_dollar(&mut self, start: usize) -> SpannedToken {
        self.pos += 1; // skip $

        if self.pos >= self.source.len() {
            return SpannedToken {
                token: Tok::Word("$".to_string()),
                span: LexSpan {
                    start,
                    end: self.pos,
                },
            };
        }

        match self.source[self.pos] {
            b'\'' => {
                // $'...' ANSI-C quoting
                self.pos += 1; // skip '
                self.scan_ansi_c_quoted(start)
            }
            b'(' => {
                if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'(' {
                    // $((
                    self.pos += 2;
                    SpannedToken {
                        token: Tok::DollarDblParen,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                } else {
                    // $(
                    self.pos += 1;
                    SpannedToken {
                        token: Tok::DollarParen,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    }
                }
            }
            b'{' => {
                // ${...}
                self.pos += 1; // skip {
                self.scan_brace_expansion(start)
            }
            b'@' | b'*' | b'#' | b'?' | b'!' | b'$' | b'0' => {
                let var = (self.source[self.pos] as char).to_string();
                self.pos += 1;
                SpannedToken {
                    token: Tok::Dollar(var),
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }
            b'1'..=b'9' => {
                // Positional param — could be multi-digit in rare cases but bash only does single
                let var = (self.source[self.pos] as char).to_string();
                self.pos += 1;
                SpannedToken {
                    token: Tok::Dollar(var),
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let name_start = self.pos;
                while self.pos < self.source.len()
                    && (self.source[self.pos].is_ascii_alphanumeric()
                        || self.source[self.pos] == b'_')
                {
                    self.pos += 1;
                }
                let name = String::from_utf8_lossy(&self.source[name_start..self.pos]).to_string();
                SpannedToken {
                    token: Tok::Dollar(name),
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }
            _ => {
                // Bare $ — treat as literal
                SpannedToken {
                    token: Tok::Word("$".to_string()),
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }
        }
    }

    // ── String scanning ───────────────────────────────────────────────

    /// Scan ${...} — capture everything between { and matching }.
    /// Handles nested braces, quoted strings, and backslash escapes.
    fn scan_brace_expansion(&mut self, start: usize) -> SpannedToken {
        let content_start = self.pos;
        let mut depth = 1i32;
        while self.pos < self.source.len() && depth > 0 {
            match self.source[self.pos] {
                b'{' => {
                    depth += 1;
                    self.pos += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    self.pos += 1;
                }
                b'\'' => {
                    // Skip single-quoted string inside ${...}
                    self.pos += 1;
                    while self.pos < self.source.len() && self.source[self.pos] != b'\'' {
                        self.pos += 1;
                    }
                    if self.pos < self.source.len() {
                        self.pos += 1;
                    }
                }
                b'"' => {
                    // Skip double-quoted string inside ${...}
                    self.pos += 1;
                    while self.pos < self.source.len() && self.source[self.pos] != b'"' {
                        if self.source[self.pos] == b'\\' {
                            self.pos += 1;
                        }
                        self.pos += 1;
                    }
                    if self.pos < self.source.len() {
                        self.pos += 1;
                    }
                }
                b'\\' => {
                    self.pos += 2;
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
        let content = String::from_utf8_lossy(&self.source[content_start..self.pos]).to_string();
        if self.pos < self.source.len() {
            self.pos += 1; // skip closing }
        }
        SpannedToken {
            token: Tok::DollarBrace(content),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Scan '...' single-quoted string.
    fn scan_single_quoted(&mut self, start: usize) -> SpannedToken {
        self.pos += 1; // skip opening '
        let content_start = self.pos;
        while self.pos < self.source.len() && self.source[self.pos] != b'\'' {
            self.pos += 1;
        }
        let content = String::from_utf8_lossy(&self.source[content_start..self.pos]).to_string();
        if self.pos < self.source.len() {
            self.pos += 1; // skip closing '
        }
        SpannedToken {
            token: Tok::SingleQuoted(content),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Scan "..." double-quoted string with interpolation.
    fn scan_double_quoted(&mut self, start: usize) -> SpannedToken {
        self.pos += 1; // skip opening "
        let mut parts = Vec::new();
        let mut literal = String::new();

        while self.pos < self.source.len() && self.source[self.pos] != b'"' {
            match self.source[self.pos] {
                b'\\' => {
                    if self.pos + 1 < self.source.len() {
                        let next = self.source[self.pos + 1];
                        match next {
                            b'"' | b'\\' | b'$' | b'`' => {
                                literal.push(next as char);
                                self.pos += 2;
                            }
                            b'\n' => {
                                // Line continuation — skip both
                                self.pos += 2;
                            }
                            _ => {
                                literal.push('\\');
                                self.pos += 1;
                            }
                        }
                    } else {
                        literal.push('\\');
                        self.pos += 1;
                    }
                }
                b'$' => {
                    // Flush literal
                    if !literal.is_empty() {
                        parts.push(StringPart::Literal(std::mem::take(&mut literal)));
                    }
                    self.pos += 1;
                    if self.pos >= self.source.len() {
                        literal.push('$');
                        continue;
                    }
                    match self.source[self.pos] {
                        b'{' => {
                            // ${...}
                            self.pos += 1;
                            let inner_start = self.pos;
                            let mut depth = 1i32;
                            while self.pos < self.source.len() && depth > 0 {
                                match self.source[self.pos] {
                                    b'{' => {
                                        depth += 1;
                                        self.pos += 1;
                                    }
                                    b'}' => {
                                        depth -= 1;
                                        if depth > 0 {
                                            self.pos += 1;
                                        }
                                    }
                                    b'\'' => {
                                        self.pos += 1;
                                        while self.pos < self.source.len()
                                            && self.source[self.pos] != b'\''
                                        {
                                            self.pos += 1;
                                        }
                                        if self.pos < self.source.len() {
                                            self.pos += 1;
                                        }
                                    }
                                    b'"' => {
                                        // Nested " inside ${} — just skip
                                        self.pos += 1;
                                    }
                                    b'\\' => {
                                        self.pos += 2;
                                    }
                                    _ => {
                                        self.pos += 1;
                                    }
                                }
                            }
                            let content =
                                String::from_utf8_lossy(&self.source[inner_start..self.pos])
                                    .to_string();
                            if self.pos < self.source.len() {
                                self.pos += 1; // skip }
                            }
                            parts.push(StringPart::Expansion(content));
                        }
                        b'(' => {
                            if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'('
                            {
                                // $((...))
                                self.pos += 2;
                                let inner_start = self.pos;
                                let mut depth = 1i32;
                                while self.pos + 1 < self.source.len() && depth > 0 {
                                    if self.source[self.pos] == b'('
                                        && self.source[self.pos + 1] == b'('
                                    {
                                        depth += 1;
                                        self.pos += 2;
                                    } else if self.source[self.pos] == b')'
                                        && self.source[self.pos + 1] == b')'
                                    {
                                        depth -= 1;
                                        if depth == 0 {
                                            break;
                                        }
                                        self.pos += 2;
                                    } else {
                                        self.pos += 1;
                                    }
                                }
                                let content =
                                    String::from_utf8_lossy(&self.source[inner_start..self.pos])
                                        .to_string();
                                if self.pos + 1 < self.source.len() {
                                    self.pos += 2; // skip ))
                                }
                                parts.push(StringPart::ArithSub(content));
                            } else {
                                // $(...)
                                self.pos += 1;
                                let inner_start = self.pos;
                                let mut depth = 1i32;
                                while self.pos < self.source.len() && depth > 0 {
                                    match self.source[self.pos] {
                                        b'(' => {
                                            depth += 1;
                                            self.pos += 1;
                                        }
                                        b')' => {
                                            depth -= 1;
                                            if depth > 0 {
                                                self.pos += 1;
                                            }
                                        }
                                        b'\'' => {
                                            self.pos += 1;
                                            while self.pos < self.source.len()
                                                && self.source[self.pos] != b'\''
                                            {
                                                self.pos += 1;
                                            }
                                            if self.pos < self.source.len() {
                                                self.pos += 1;
                                            }
                                        }
                                        b'"' => {
                                            self.pos += 1;
                                            while self.pos < self.source.len()
                                                && self.source[self.pos] != b'"'
                                            {
                                                if self.source[self.pos] == b'\\' {
                                                    self.pos += 1;
                                                }
                                                self.pos += 1;
                                            }
                                            if self.pos < self.source.len() {
                                                self.pos += 1;
                                            }
                                        }
                                        _ => {
                                            self.pos += 1;
                                        }
                                    }
                                }
                                let content =
                                    String::from_utf8_lossy(&self.source[inner_start..self.pos])
                                        .to_string();
                                if self.pos < self.source.len() {
                                    self.pos += 1; // skip )
                                }
                                parts.push(StringPart::CommandSub(content));
                            }
                        }
                        c if c.is_ascii_alphabetic() || c == b'_' => {
                            let name_start = self.pos;
                            while self.pos < self.source.len()
                                && (self.source[self.pos].is_ascii_alphanumeric()
                                    || self.source[self.pos] == b'_')
                            {
                                self.pos += 1;
                            }
                            let name = String::from_utf8_lossy(&self.source[name_start..self.pos])
                                .to_string();
                            parts.push(StringPart::Variable(name));
                        }
                        b'@' | b'*' | b'#' | b'?' | b'!' | b'$' | b'0'..=b'9' => {
                            let var = (self.source[self.pos] as char).to_string();
                            self.pos += 1;
                            parts.push(StringPart::Variable(var));
                        }
                        _ => {
                            literal.push('$');
                        }
                    }
                }
                b'`' => {
                    // Backtick inside string — capture for rejection
                    if !literal.is_empty() {
                        parts.push(StringPart::Literal(std::mem::take(&mut literal)));
                    }
                    self.pos += 1;
                    let inner_start = self.pos;
                    while self.pos < self.source.len() && self.source[self.pos] != b'`' {
                        self.pos += 1;
                    }
                    let content =
                        String::from_utf8_lossy(&self.source[inner_start..self.pos]).to_string();
                    if self.pos < self.source.len() {
                        self.pos += 1; // skip closing `
                    }
                    parts.push(StringPart::CommandSub(format!("`{}`", content)));
                }
                _ => {
                    literal.push(self.source[self.pos] as char);
                    self.pos += 1;
                }
            }
        }

        if !literal.is_empty() {
            parts.push(StringPart::Literal(literal));
        }

        if self.pos < self.source.len() {
            self.pos += 1; // skip closing "
        }

        SpannedToken {
            token: Tok::DoubleQuoted(parts),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Scan $'...' ANSI-C quoted string (opening $' already consumed).
    fn scan_ansi_c_quoted(&mut self, start: usize) -> SpannedToken {
        let mut result = String::new();
        while self.pos < self.source.len() && self.source[self.pos] != b'\'' {
            if self.source[self.pos] == b'\\' && self.pos + 1 < self.source.len() {
                self.pos += 1;
                match self.source[self.pos] {
                    b'n' => {
                        result.push('\n');
                        self.pos += 1;
                    }
                    b't' => {
                        result.push('\t');
                        self.pos += 1;
                    }
                    b'r' => {
                        result.push('\r');
                        self.pos += 1;
                    }
                    b'a' => {
                        result.push('\x07');
                        self.pos += 1;
                    }
                    b'b' => {
                        result.push('\x08');
                        self.pos += 1;
                    }
                    b'e' | b'E' => {
                        result.push('\x1b');
                        self.pos += 1;
                    }
                    b'f' => {
                        result.push('\x0c');
                        self.pos += 1;
                    }
                    b'v' => {
                        result.push('\x0b');
                        self.pos += 1;
                    }
                    b'\\' => {
                        result.push('\\');
                        self.pos += 1;
                    }
                    b'\'' => {
                        result.push('\'');
                        self.pos += 1;
                    }
                    b'"' => {
                        result.push('"');
                        self.pos += 1;
                    }
                    b'0' => {
                        self.pos += 1;
                        let mut val = 0u8;
                        for _ in 0..3 {
                            if self.pos < self.source.len()
                                && self.source[self.pos] >= b'0'
                                && self.source[self.pos] <= b'7'
                            {
                                val = val * 8 + (self.source[self.pos] - b'0');
                                self.pos += 1;
                            } else {
                                break;
                            }
                        }
                        result.push(val as char);
                    }
                    b'x' => {
                        self.pos += 1;
                        let mut val = 0u8;
                        for _ in 0..2 {
                            if self.pos < self.source.len() {
                                let d = match self.source[self.pos] {
                                    b'0'..=b'9' => self.source[self.pos] - b'0',
                                    b'a'..=b'f' => self.source[self.pos] - b'a' + 10,
                                    b'A'..=b'F' => self.source[self.pos] - b'A' + 10,
                                    _ => break,
                                };
                                val = val * 16 + d;
                                self.pos += 1;
                            }
                        }
                        result.push(val as char);
                    }
                    other => {
                        result.push('\\');
                        result.push(other as char);
                        self.pos += 1;
                    }
                }
            } else {
                result.push(self.source[self.pos] as char);
                self.pos += 1;
            }
        }
        if self.pos < self.source.len() {
            self.pos += 1; // skip closing '
        }
        SpannedToken {
            token: Tok::AnsiCQuoted(result),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    // ── Word scanning ─────────────────────────────────────────────────

    /// Scan a bare word token.
    /// Accumulates bytes until a word-breaking character, then checks
    /// the result against the keyword table.
    fn scan_word(&mut self, start: usize) -> SpannedToken {
        let mut word = String::new();

        while self.pos < self.source.len() {
            let b = self.source[self.pos];
            match b {
                // Word-breaking characters
                b' ' | b'\t' | b'\n' | b';' | b'|' | b'&' | b'(' | b')' | b'{' | b'}' | b'<'
                | b'>' | b'#' | b'"' | b'\'' | b'`' | b'$' => break,
                b'=' => {
                    // = breaks a word IFF we're at word start scanning an assignment
                    // We need to emit the word accumulated so far, then emit = separately
                    if word.is_empty() {
                        self.pos += 1;
                        return SpannedToken {
                            token: Tok::Eq,
                            span: LexSpan {
                                start,
                                end: self.pos,
                            },
                        };
                    }
                    // Check if next char is = (== operator in test context)
                    if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'=' {
                        break; // Let the parser handle ==
                    }
                    // Otherwise this = is part of assignment — break here so parser
                    // can see: Word("name") Eq Word("value")
                    break;
                }
                b'+' => {
                    if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'=' {
                        break; // += assignment operator
                    }
                    word.push('+');
                    self.pos += 1;
                }
                b'[' | b']' => {
                    // Include brackets in words (array subscripts, case patterns)
                    word.push(b as char);
                    self.pos += 1;
                }
                _ => {
                    word.push(b as char);
                    self.pos += 1;
                }
            }
        }

        if word.is_empty() {
            // Shouldn't happen but safety
            return SpannedToken {
                token: Tok::Eof,
                span: LexSpan { start, end: start },
            };
        }

        // Check for keyword match
        let token = match word.as_str() {
            "if" => Tok::If,
            "then" => Tok::Then,
            "else" => Tok::Else,
            "elif" => Tok::Elif,
            "fi" => Tok::Fi,
            "while" => Tok::While,
            "until" => Tok::Until,
            "do" => Tok::Do,
            "done" => Tok::Done,
            "for" => Tok::For,
            "in" => Tok::In,
            "case" => Tok::Case,
            "esac" => Tok::Esac,
            "fn" => Tok::Fn,
            "function" => Tok::Function,
            "local" => Tok::Local,
            "int" => Tok::Int,
            "const" => Tok::Const,
            "array" => Tok::Array,
            "map" => Tok::Map,
            "bless" => Tok::Bless,
            "signal" => Tok::Word("signal".to_string()),
            "END" => Tok::End,
            "return" => Tok::Word("return".to_string()),
            "break" => Tok::Word("break".to_string()),
            "continue" => Tok::Word("continue".to_string()),
            "eval" => Tok::Word("eval".to_string()), // keep as word for error reporting
            "source" => Tok::Word("source".to_string()),
            "export" => Tok::Word("export".to_string()),
            "declare" => Tok::Word("declare".to_string()),
            "typeset" => Tok::Word("typeset".to_string()),
            "readonly" => Tok::Word("readonly".to_string()),
            "select" => Tok::Word("select".to_string()),
            _ => Tok::Word(word),
        };

        SpannedToken {
            token,
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }
}
