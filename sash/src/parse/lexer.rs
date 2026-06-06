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
    // Strategy: a small dispatch on the first byte separates the
    // context-sensitive scan paths (strings, expansions, words,
    // heredoc detection, dash/punct vs word) from the structural
    // operators that go through a `logos`-generated DFA. The DFA
    // handles longest-match for the operator and keyword families
    // that have no contextual ambiguity (see logos_tokens.rs).
    //
    // r47-decide-logos-vs-handroll: kept hand-roll for `<` (heredoc),
    // `-` (dash vs word), and the punct/word boundary cases.

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
            // Context-sensitive: stateful scan paths
            b'$' => self.scan_dollar(start),
            b'\'' => self.scan_single_quoted(start),
            b'"' => self.scan_double_quoted(start),
            b'`' => self.scan_backtick(start),
            b'<' => self.scan_lt(start),
            b'-' => self.scan_dash_or_word(start),

            // Word-boundary punctuation
            b'!' => self.scan_bang_or_neq(start),
            b'=' => self.scan_eq_punct(start),
            b'+' => self.scan_plus_or_word(start),
            b'[' => self.scan_lbracket(start),
            b']' => self.scan_rbracket(start),

            // Structural operators / comments / newline → logos
            b'\n' | b'#' | b';' | b'|' | b'&' | b'(' | b')' | b'{' | b'}' | b'>' => {
                self.scan_via_logos(start)
            }

            // Default: word scan (carries keyword promotion)
            _ => self.scan_word(start),
        }
    }

    /// Run the `logos` DFA at the current position and emit the matched
    /// token. The caller has already established that the first byte is
    /// one of the "structural" bytes (no contextual ambiguity).
    fn scan_via_logos(&mut self, start: usize) -> SpannedToken {
        use super::logos_tokens::{to_tok, LogosTok};
        use logos::Logos;

        // Logos defaults to &str source; we hold bytes. The slice is
        // ASCII for every structural-byte dispatch (newline, `#`, `;`,
        // `|`, `&`, `(`, `)`, `{`, `}`, `>`) but comments may contain
        // UTF-8 — we accept that by using lossy decoding here. The
        // matched byte length is what counts; the comment payload is
        // re-read from the original byte slice below.
        let bytes_slice = &self.source[self.pos..];
        let str_slice = match std::str::from_utf8(bytes_slice) {
            Ok(s) => s,
            Err(e) => {
                // Truncate at the first invalid byte so logos sees a
                // valid prefix. The remainder will be handled by the
                // next dispatch (likely scan_word).
                std::str::from_utf8(&bytes_slice[..e.valid_up_to()])
                    .expect("checked prefix is valid UTF-8")
            }
        };
        let mut lex = LogosTok::lexer(str_slice);
        match lex.next() {
            Some(Ok(lt)) => {
                let len = lex.span().end;
                let end = self.pos + len;
                let token = if matches!(lt, LogosTok::Comment) {
                    let text = String::from_utf8_lossy(&self.source[self.pos..end]).to_string();
                    Tok::Comment(text)
                } else {
                    to_tok(lt).expect("logos token without payload mapping")
                };
                self.pos = end;
                SpannedToken {
                    token,
                    span: LexSpan { start, end },
                }
            }
            _ => {
                // Logos didn't match the dispatched byte. Fall back to
                // word scan so the byte still becomes part of a token.
                self.scan_word(start)
            }
        }
    }

    /// Scan `<<<` (HereString) / `<<` (heredoc sentinel, rejected) /
    /// `<&` (LtAmp) / `<>` (LtGt) / `<` (Lt).
    fn scan_lt(&mut self, start: usize) -> SpannedToken {
        let token = if self.starts_with(b"<<<") {
            self.pos += 3;
            Tok::HereString
        } else if self.starts_with(b"<<") {
            // Heredocs are not supported in sash. Skip to end of
            // delimiter word so we emit one sentinel for the validator.
            self.pos += 2;
            if self.pos < self.source.len() && self.source[self.pos] == b'-' {
                self.pos += 1;
            }
            while self.pos < self.source.len()
                && !matches!(self.source[self.pos], b'\n' | b' ' | b'\t')
            {
                self.pos += 1;
            }
            Tok::Word("__HEREDOC__".to_string())
        } else if self.starts_with(b"<&") {
            self.pos += 2;
            Tok::LtAmp
        } else if self.starts_with(b"<>") {
            self.pos += 2;
            Tok::LtGt
        } else {
            self.pos += 1;
            Tok::Lt
        };
        SpannedToken {
            token,
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Scan `-` standalone (Dash) vs `-flag` (word).
    fn scan_dash_or_word(&mut self, start: usize) -> SpannedToken {
        let next = self.source.get(self.pos + 1).copied().unwrap_or(b' ');
        let is_terminator = matches!(
            next,
            b' ' | b'\t' | b'\n' | b';' | b'|' | b'&' | b')' | b'}' | b'#'
        );
        if is_terminator {
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

    /// Scan `!` (Bang) vs `!=` (Word).
    fn scan_bang_or_neq(&mut self, start: usize) -> SpannedToken {
        if self.source.get(self.pos + 1) == Some(&b'=') {
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

    /// Scan `=` (Eq) / `==` / `=~` (both Word).
    fn scan_eq_punct(&mut self, start: usize) -> SpannedToken {
        let token = match self.source.get(self.pos + 1) {
            Some(&b'~') => {
                self.pos += 2;
                Tok::Word("=~".to_string())
            }
            Some(&b'=') => {
                self.pos += 2;
                Tok::Word("==".to_string())
            }
            _ => {
                self.pos += 1;
                Tok::Eq
            }
        };
        SpannedToken {
            token,
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Scan `+=` (PlusAssign) vs `+` (word character).
    fn scan_plus_or_word(&mut self, start: usize) -> SpannedToken {
        if self.source.get(self.pos + 1) == Some(&b'=') {
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

    /// `[[` is an operator, bare `[` is a word character.
    fn scan_lbracket(&mut self, start: usize) -> SpannedToken {
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
            self.scan_word(start)
        }
    }

    /// `]]` is an operator, bare `]` is a word character.
    fn scan_rbracket(&mut self, start: usize) -> SpannedToken {
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

    /// Backticks are rejected (SB5020). Consume until matching backtick
    /// or end-of-line so error reporting can show the offending span.
    fn scan_backtick(&mut self, start: usize) -> SpannedToken {
        self.pos += 1; // opening `
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
                b'\\' => self.scan_dq_escape(&mut literal),
                b'$' => self.scan_dq_dollar(&mut parts, &mut literal),
                b'`' => self.scan_dq_backtick(&mut parts, &mut literal),
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

    /// Handle a `\` inside a `"…"` string. Per POSIX, `\` is literal
    /// except before `"`, `\`, `$`, `` ` ``, or newline (line continuation).
    fn scan_dq_escape(&mut self, literal: &mut String) {
        if self.pos + 1 >= self.source.len() {
            literal.push('\\');
            self.pos += 1;
            return;
        }
        let next = self.source[self.pos + 1];
        match next {
            b'"' | b'\\' | b'$' | b'`' => {
                literal.push(next as char);
                self.pos += 2;
            }
            b'\n' => {
                // Line continuation — both bytes vanish.
                self.pos += 2;
            }
            _ => {
                literal.push('\\');
                self.pos += 1;
            }
        }
    }

    /// Handle a `$` inside a `"…"` string. Dispatches to one of the
    /// six expansion forms (`${…}`, `$((…))`, `$(…)`, `$name`, special
    /// var, or bare literal `$`). Flushes the current literal first
    /// when an expansion is recognised.
    fn scan_dq_dollar(&mut self, parts: &mut Vec<StringPart>, literal: &mut String) {
        if !literal.is_empty() {
            parts.push(StringPart::Literal(std::mem::take(literal)));
        }
        self.pos += 1; // skip $
        if self.pos >= self.source.len() {
            literal.push('$');
            return;
        }
        match self.source[self.pos] {
            b'{' => {
                let content = self.consume_dq_brace_expansion();
                parts.push(StringPart::Expansion(content));
            }
            b'(' => {
                if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'(' {
                    let content = self.consume_dq_arith_sub();
                    parts.push(StringPart::ArithSub(content));
                } else {
                    let content = self.consume_dq_cmd_sub();
                    parts.push(StringPart::CommandSub(content));
                }
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let name = self.consume_dq_var_name();
                parts.push(StringPart::Variable(name));
            }
            b'@' | b'*' | b'#' | b'?' | b'!' | b'$' | b'0'..=b'9' => {
                let var = (self.source[self.pos] as char).to_string();
                self.pos += 1;
                parts.push(StringPart::Variable(var));
            }
            _ => {
                // Bare `$` followed by something unexpected — keep as
                // literal; don't consume the next byte.
                literal.push('$');
            }
        }
    }

    /// `${…}` — opening `{` is at `self.pos`. Returns the inner text
    /// and advances past the closing `}`.
    fn consume_dq_brace_expansion(&mut self) -> String {
        self.pos += 1; // skip {
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
                b'\'' => self.skip_balanced_squote(),
                b'"' => {
                    // Nested " inside ${} — skip without re-scanning.
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
        let content = String::from_utf8_lossy(&self.source[inner_start..self.pos]).to_string();
        if self.pos < self.source.len() {
            self.pos += 1; // skip closing }
        }
        content
    }

    /// `$((…))` — caller has not yet advanced past `((`. Returns the
    /// inner arithmetic text and advances past the closing `))`.
    fn consume_dq_arith_sub(&mut self) -> String {
        self.pos += 2; // skip ((
        let inner_start = self.pos;
        let mut depth = 1i32;
        while self.pos + 1 < self.source.len() && depth > 0 {
            if self.source[self.pos] == b'(' && self.source[self.pos + 1] == b'(' {
                depth += 1;
                self.pos += 2;
            } else if self.source[self.pos] == b')' && self.source[self.pos + 1] == b')' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                self.pos += 2;
            } else {
                self.pos += 1;
            }
        }
        let content = String::from_utf8_lossy(&self.source[inner_start..self.pos]).to_string();
        if self.pos + 1 < self.source.len() {
            self.pos += 2; // skip ))
        }
        content
    }

    /// `$(…)` — caller has not yet advanced past `(`. Returns the
    /// inner command text and advances past the closing `)`.
    fn consume_dq_cmd_sub(&mut self) -> String {
        self.pos += 1; // skip (
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
                b'\'' => self.skip_balanced_squote(),
                b'"' => self.skip_balanced_dquote(),
                _ => {
                    self.pos += 1;
                }
            }
        }
        let content = String::from_utf8_lossy(&self.source[inner_start..self.pos]).to_string();
        if self.pos < self.source.len() {
            self.pos += 1; // skip closing )
        }
        content
    }

    /// `$NAME` — caller is at the first name byte (alpha/`_`). Returns
    /// the name and advances past its last byte.
    fn consume_dq_var_name(&mut self) -> String {
        let name_start = self.pos;
        while self.pos < self.source.len()
            && (self.source[self.pos].is_ascii_alphanumeric() || self.source[self.pos] == b'_')
        {
            self.pos += 1;
        }
        String::from_utf8_lossy(&self.source[name_start..self.pos]).to_string()
    }

    /// Skip a single-quoted run starting at `self.pos`. Single quotes
    /// have no escapes; just advance until the closing `'`.
    fn skip_balanced_squote(&mut self) {
        self.pos += 1; // opening '
        while self.pos < self.source.len() && self.source[self.pos] != b'\'' {
            self.pos += 1;
        }
        if self.pos < self.source.len() {
            self.pos += 1;
        }
    }

    /// Skip a double-quoted run starting at `self.pos`. Honours `\`
    /// escapes (one extra byte past the backslash).
    fn skip_balanced_dquote(&mut self) {
        self.pos += 1; // opening "
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

    /// Handle a `` ` `` inside a `"…"` string. Captures the backtick
    /// span as a CommandSub so the validator can reject it (SB5020).
    fn scan_dq_backtick(&mut self, parts: &mut Vec<StringPart>, literal: &mut String) {
        if !literal.is_empty() {
            parts.push(StringPart::Literal(std::mem::take(literal)));
        }
        self.pos += 1; // opening `
        let inner_start = self.pos;
        while self.pos < self.source.len() && self.source[self.pos] != b'`' {
            self.pos += 1;
        }
        let content = String::from_utf8_lossy(&self.source[inner_start..self.pos]).to_string();
        if self.pos < self.source.len() {
            self.pos += 1; // closing `
        }
        parts.push(StringPart::CommandSub(format!("`{}`", content)));
    }

    /// Scan $'...' ANSI-C quoted string (opening $' already consumed).
    fn scan_ansi_c_quoted(&mut self, start: usize) -> SpannedToken {
        let mut result = String::new();
        while self.pos < self.source.len() && self.source[self.pos] != b'\'' {
            if self.source[self.pos] == b'\\' && self.pos + 1 < self.source.len() {
                self.pos += 1;
                self.consume_ansi_c_escape(&mut result);
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

    /// `\…` body inside `$'…'`. `self.pos` points at the byte after `\`.
    /// Tries `simple_escape` first, then `\0NNN` octal and `\xHH` hex,
    /// then falls back to literal `\<byte>` passthrough.
    fn consume_ansi_c_escape(&mut self, out: &mut String) {
        let c = self.source[self.pos];
        if let Some(ch) = simple_escape(c) {
            out.push(ch);
            self.pos += 1;
            return;
        }
        match c {
            b'0' => {
                self.pos += 1;
                out.push(self.scan_ansi_c_octal() as char);
            }
            b'x' => {
                self.pos += 1;
                out.push(self.scan_ansi_c_hex() as char);
            }
            other => {
                out.push('\\');
                out.push(other as char);
                self.pos += 1;
            }
        }
    }

    /// Up to three octal digits after `\0`. Stops at the first non-octal
    /// byte or EOI.
    fn scan_ansi_c_octal(&mut self) -> u8 {
        let mut val = 0u8;
        for _ in 0..3 {
            if self.pos >= self.source.len() {
                break;
            }
            let b = self.source[self.pos];
            if !(b'0'..=b'7').contains(&b) {
                break;
            }
            val = val.wrapping_mul(8).wrapping_add(b - b'0');
            self.pos += 1;
        }
        val
    }

    /// Up to two hex digits after `\x`. Stops at the first non-hex byte
    /// or EOI.
    fn scan_ansi_c_hex(&mut self) -> u8 {
        let mut val = 0u8;
        for _ in 0..2 {
            if self.pos >= self.source.len() {
                break;
            }
            let d = match self.source[self.pos] {
                b'0'..=b'9' => self.source[self.pos] - b'0',
                b'a'..=b'f' => self.source[self.pos] - b'a' + 10,
                b'A'..=b'F' => self.source[self.pos] - b'A' + 10,
                _ => break,
            };
            val = val.wrapping_mul(16).wrapping_add(d);
            self.pos += 1;
        }
        val
    }

    // ── Word scanning ─────────────────────────────────────────────────

    /// Scan a bare word token.
    /// Accumulates bytes until a word-breaking character, then checks
    /// the result against the keyword table.
    fn scan_word(&mut self, start: usize) -> SpannedToken {
        let mut word = String::new();

        if let Some(eq_tok) = self.scan_word_chars(start, &mut word) {
            return eq_tok;
        }

        if word.is_empty() {
            return SpannedToken {
                token: Tok::Eof,
                span: LexSpan { start, end: start },
            };
        }

        SpannedToken {
            token: classify_word_token(word),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Scan word body bytes into `word`. Returns `Some(token)` if a bare `=`
    /// at word start was consumed (caller emits it directly); returns `None`
    /// after a normal break / end-of-input.
    fn scan_word_chars(&mut self, start: usize, word: &mut String) -> Option<SpannedToken> {
        while self.pos < self.source.len() {
            let b = self.source[self.pos];
            match b {
                b' ' | b'\t' | b'\n' | b';' | b'|' | b'&' | b'(' | b')' | b'{' | b'}' | b'<'
                | b'>' | b'#' | b'"' | b'\'' | b'`' | b'$' => break,
                b'=' => {
                    if word.is_empty() {
                        self.pos += 1;
                        return Some(SpannedToken {
                            token: Tok::Eq,
                            span: LexSpan {
                                start,
                                end: self.pos,
                            },
                        });
                    }
                    if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'=' {
                        break;
                    }
                    break;
                }
                b'+' => {
                    if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'=' {
                        break;
                    }
                    word.push('+');
                    self.pos += 1;
                }
                b'[' | b']' => {
                    word.push(b as char);
                    self.pos += 1;
                }
                _ => {
                    word.push(b as char);
                    self.pos += 1;
                }
            }
        }
        None
    }
}

/// Map a scanned word to its keyword token, falling back to `Tok::Word`.
fn classify_word_token(word: String) -> Tok {
    match word.as_str() {
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
        "eval" => Tok::Word("eval".to_string()),
        "source" => Tok::Word("source".to_string()),
        "export" => Tok::Word("export".to_string()),
        "declare" => Tok::Word("declare".to_string()),
        "typeset" => Tok::Word("typeset".to_string()),
        "readonly" => Tok::Word("readonly".to_string()),
        "select" => Tok::Word("select".to_string()),
        _ => Tok::Word(word),
    }
}

/// Named-escape table for `$'…'`: `\n`, `\t`, `\r`, `\a`, `\b`, `\e`/`\E`,
/// `\f`, `\v`, `\\`, `\'`, `\"`. Returns `None` for anything else (octal
/// `\0NNN`, hex `\xHH`, or unknown — caller handles).
fn simple_escape(b: u8) -> Option<char> {
    Some(match b {
        b'n' => '\n',
        b't' => '\t',
        b'r' => '\r',
        b'a' => '\x07',
        b'b' => '\x08',
        b'e' | b'E' => '\x1b',
        b'f' => '\x0c',
        b'v' => '\x0b',
        b'\\' => '\\',
        b'\'' => '\'',
        b'"' => '"',
        _ => return None,
    })
}
