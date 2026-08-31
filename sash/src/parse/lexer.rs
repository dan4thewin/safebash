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

    // Terminators
    DblSemi, // ;;

    // Literals / values
    Word(String),
    SingleQuoted(String),          // content without quotes
    DoubleQuoted(Vec<StringPart>), // interpolated parts
    AnsiCQuoted(String),           // $'...' → expanded string

    // Expansions (produced by scanner)
    Dollar(String),      // $var, $1, $@, $#, $?, $$, $!
    DollarBrace(String), // ${...} raw content for sub-parsing
    DollarParen,         // $( — start of command substitution
    Backtick(String),    // DD#67: flat `cmd` → $(cmd); raw inner content

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
    /// Nonzero while scanning the body of `(( ))` / `$(( ))`. In this mode
    /// `<<` is the left-shift operator, not a heredoc
    /// (bug-arith-left-shift-heredoc).
    arith_depth: usize,
}

impl Scanner {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.as_bytes().to_vec(),
            pos: 0,
            peeked: Vec::new(),
            last_end: 0,
            arith_depth: 0,
        }
    }

    /// Enter arithmetic-scanning mode (`(( … ))` body). Must be paired with
    /// [`exit_arith`]. While active, `<<` lexes as left-shift, not heredoc.
    pub fn enter_arith(&mut self) {
        self.arith_depth += 1;
    }

    /// Leave arithmetic-scanning mode (see [`enter_arith`]).
    pub fn exit_arith(&mut self) {
        self.arith_depth = self.arith_depth.saturating_sub(1);
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
            let tok = self.next_token_skip_comments();
            self.peeked.push(tok);
        }
        &self.peeked[0]
    }

    /// Peek at the Nth token ahead (0-based).
    pub fn peek_n(&mut self, n: usize) -> &SpannedToken {
        while self.peeked.len() <= n {
            let tok = self.next_token_skip_comments();
            self.peeked.push(tok);
        }
        &self.peeked[n]
    }

    /// Consume and return the next token.
    pub fn next(&mut self) -> SpannedToken {
        let tok = if !self.peeked.is_empty() {
            self.peeked.remove(0)
        } else {
            self.next_token_skip_comments()
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

    /// Fetch the next non-trivia token. Comments are lexical trivia (DG
    /// 2026-08-31): the scanner drops `# … EOL` tokens entirely so a trailing
    /// comment never reaches the parser (it would otherwise be an "expected
    /// expression" at every value-collecting loop). The source is preserved
    /// verbatim in the `.sash_source` ELF section, so `--extract` round-trip is
    /// unaffected. (Inside `$(( ))` a `#` is a Word, not a Comment — see
    /// `arith_hash_word` — so arithmetic is untouched.)
    fn next_token_skip_comments(&mut self) -> SpannedToken {
        loop {
            let tok = self.next_token();
            if matches!(tok.token, Tok::Comment(_)) {
                continue;
            }
            return tok;
        }
    }

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

        if let Some(tok) = self.arith_hash_word(start) {
            return tok;
        }
        let str_slice = logos_input(&self.source[self.pos..]);
        let mut lex = LogosTok::lexer(str_slice);
        match lex.next() {
            Some(Ok(lt)) => {
                // Read the match length now — this is the last use of `lex`, so
                // its borrow of `self.source` is released before the `&mut self`
                // calls below.
                let len = lex.span().end;
                if let Some(tok) = self.split_dbl_paren_close(&lt, start) {
                    return tok;
                }
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

    /// DD#72: inside `$(( ))` (`arith_depth > 0`), `#` is NOT a comment — emit
    /// it as a single-char word so the arith re-lexer sees (and rejects)
    /// `base#digits` instead of the scanner truncating at the `#`. `None` when
    /// not in arith mode or the byte is not `#`.
    fn arith_hash_word(&mut self, start: usize) -> Option<SpannedToken> {
        if self.arith_depth > 0 && self.source[self.pos] == b'#' {
            self.pos += 1;
            return Some(SpannedToken {
                token: Tok::Word("#".to_string()),
                span: LexSpan {
                    start,
                    end: self.pos,
                },
            });
        }
        None
    }

    /// `))` lexes as one `DblParenClose`, but that token is only meaningful
    /// inside arithmetic (`(( ))` / `$(( ))`). Outside arith mode, a `))` is two
    /// separate `)` — e.g. two adjacent command-substitution closes in
    /// `$(f $(g))` (bug-bare-nested-cmdsub-arg). Split it: emit a single
    /// `RParen`, consuming one byte, so the second `)` re-lexes next call
    /// (`)))` splits into three closes). `None` leaves the token as-is.
    fn split_dbl_paren_close(
        &mut self,
        lt: &super::logos_tokens::LogosTok,
        start: usize,
    ) -> Option<SpannedToken> {
        use super::logos_tokens::LogosTok;
        if matches!(lt, LogosTok::DblParenClose) && self.arith_depth == 0 {
            self.pos = start + 1;
            return Some(SpannedToken {
                token: Tok::RParen,
                span: LexSpan {
                    start,
                    end: start + 1,
                },
            });
        }
        None
    }

    /// Scan `<<<` (HereString) / `<<` (heredoc sentinel, rejected) /
    /// `<&` (LtAmp) / `<>` (LtGt) / `<` (Lt).
    fn scan_lt(&mut self, start: usize) -> SpannedToken {
        let token = if self.starts_with(b"<<<") {
            self.pos += 3;
            Tok::HereString
        } else if self.arith_depth > 0 && self.starts_with(b"<<") {
            // Inside `(( ))` / `$(( ))`, `<<` is the left-shift operator,
            // not a heredoc (the scanner has no arith context otherwise).
            // Emit it as a verbatim word so the arith re-lexer recombines
            // it into `<<` / `<<=` (Shl). bug-arith-left-shift-heredoc.
            self.pos += 2;
            Tok::Word("<<".to_string())
        } else if self.starts_with(b"<<") {
            self.scan_heredoc_sentinel()
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

    /// `<<` / `<<-` heredoc (outside arith context). Heredocs are not
    /// supported in sash — skip to the end of the delimiter word and emit a
    /// single `__HEREDOC__` sentinel for the validator to reject.
    fn scan_heredoc_sentinel(&mut self) -> Tok {
        self.pos += 2;
        if self.pos < self.source.len() && self.source[self.pos] == b'-' {
            self.pos += 1;
        }
        while self.pos < self.source.len() && !matches!(self.source[self.pos], b'\n' | b' ' | b'\t')
        {
            self.pos += 1;
        }
        Tok::Word("__HEREDOC__".to_string())
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

    /// DD#67: flat `` `cmd` `` is sugar for `$(cmd)` → `Tok::Backtick`
    /// carrying the raw inner content. A `\`` inside (the bash nesting
    /// escape, which DD#64 removes) can't map 1-to-1, so it falls back to
    /// the `` `backtick` `` sentinel the parser rejects with a diagnostic.
    fn scan_backtick(&mut self, start: usize) -> SpannedToken {
        self.pos += 1; // opening `
        let (content, nested) = self.scan_backtick_body();
        let terminated = self.pos < self.source.len() && self.source[self.pos] == b'`';
        if terminated {
            self.pos += 1; // closing `
        }
        let token = if nested || !terminated {
            Tok::Word("`backtick`".to_string())
        } else {
            Tok::Backtick(content)
        };
        SpannedToken {
            token,
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Scan a backtick body up to the closing `` ` `` / newline / EOF.
    /// Returns the raw content and whether a `\`` nesting attempt was seen.
    fn scan_backtick_body(&mut self) -> (String, bool) {
        let content_start = self.pos;
        let mut nested = false;
        while self.pos < self.source.len()
            && self.source[self.pos] != b'`'
            && self.source[self.pos] != b'\n'
        {
            if self.is_escaped_backtick() {
                nested = true;
                self.pos += 2;
            } else {
                self.pos += 1;
            }
        }
        let content = String::from_utf8_lossy(&self.source[content_start..self.pos]).into_owned();
        (content, nested)
    }

    /// `\`` at the cursor — the bash nesting escape DD#64 removes.
    fn is_escaped_backtick(&self) -> bool {
        self.source[self.pos] == b'\\'
            && self.pos + 1 < self.source.len()
            && self.source[self.pos + 1] == b'`'
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
        let content = self.scan_braced_content();
        SpannedToken {
            token: Tok::DollarBrace(content),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// DD#69: `@{a:-WORD}` / `@{a:?WORD}` external-splat disposition. `self.pos`
    /// is at the `@`. Scans the whole `@{…}` (brace-depth / DD#65 semantics,
    /// like `${…}`) into a single `Word` so `parse_word_arm` can decode it.
    fn scan_at_brace(&mut self, start: usize) -> SpannedToken {
        self.pos += 2; // skip `@{`
        let content = self.scan_braced_content();
        SpannedToken {
            token: Tok::Word(format!("@{{{}}}", content)),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Scan `${…}` inner content by brace depth. `self.pos` is just past the
    /// opening `{`; on return it is past the matching `}` and the result is
    /// the text between them. DD#65: quotes and `\` are ordinary bytes, so the
    /// first unnested `}` closes (embed a `}` via a `$'\x7d'` C-string). Shared
    /// by the bare (`scan_brace_expansion`) and in-`"…"`
    /// (`consume_dq_brace_expansion`) scanners.
    fn scan_braced_content(&mut self) -> String {
        if let Some(content) = self.try_scan_sed_braced_content() {
            return content;
        }
        self.scan_by_brace_depth()
    }

    /// The default `${…}` scan: the first unnested `}` closes (DD#65 — quotes
    /// and `\` are ordinary bytes). `self.pos` is just past the opening `{`.
    fn scan_by_brace_depth(&mut self) -> String {
        let inner_start = self.pos;
        let mut depth = 1i32;
        while self.pos < self.source.len() && depth > 0 {
            match self.source[self.pos] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            if depth > 0 {
                self.pos += 1;
            }
        }
        let content = String::from_utf8_lossy(&self.source[inner_start..self.pos]).to_string();
        if self.pos < self.source.len() {
            self.pos += 1; // skip closing }
        }
        content
    }

    /// DD#63 Path B: `${name:s/RE/SUB/flags}` — the three `/`-delimited
    /// fields make a bare `{`/`}` inside RE/SUB literal, so scan by slash
    /// count instead of brace depth (the default `}` closer would otherwise
    /// truncate `${v:s/a}b/z/}` at the first `}`). On a well-formed payload,
    /// returns `name:s/RE/SUB/flags` and advances past the closing `}`. On an
    /// unterminated payload (a `"`/newline/EOF arrives before the third `/`
    /// and its `}`), returns the best-effort content and stops *at* the
    /// boundary byte — the parser's `try_sed_op` then flags SB4032. Returns
    /// `None` when this is not a `:s/` expansion (caller falls back to the
    /// brace-depth scan).
    fn try_scan_sed_braced_content(&mut self) -> Option<String> {
        let start = self.pos;
        let body_start = self.sed_prefix_end(start)?;
        let (end, closed) = self.scan_sed_payload(body_start);
        let content = String::from_utf8_lossy(&self.source[start..end]).to_string();
        self.pos = if closed { end + 1 } else { end };
        Some(content)
    }

    /// If the `${…}` body opens with `NAME:s/`, return the byte index just
    /// past the `:s/`; otherwise `None`. `start` is the first content byte.
    fn sed_prefix_end(&self, start: usize) -> Option<usize> {
        let src = &self.source;
        let i = scan_ident_end(src, start)?;
        if i + 3 <= src.len() && &src[i..i + 3] == b":s/" {
            Some(i + 3)
        } else {
            None
        }
    }

    /// Scan an `s/…` payload from `from` (just past the first `/`). Two more
    /// `/` delimiters end RE and SUB; then flags run to the closing `}`.
    /// Returns `(end, closed)`: `end` is the closing `}` when `closed`, else
    /// the boundary byte (`"`/newline/EOF) of an unterminated payload.
    fn scan_sed_payload(&self, from: usize) -> (usize, bool) {
        let src = &self.source;
        let mut j = from;
        let mut slashes = 0u8;
        while j < src.len() {
            match src[j] {
                b'"' | b'\n' => return (j, false),
                b'/' if slashes < 2 => slashes += 1,
                b'}' if slashes >= 2 => return (j, true),
                _ => {}
            }
            j += 1;
        }
        (j, false)
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
        let parts = self.scan_interp_body(true);
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

    /// Walk a double-quoted-string body from `self.pos`, splitting it into
    /// literal runs and `$…`/`` `…` `` expansion parts. When `stop_at_dquote`
    /// the body ends at the next `"` (an ordinary `"…"` string); otherwise a
    /// `"` is an ordinary literal byte and the body runs to end-of-input.
    fn scan_interp_body(&mut self, stop_at_dquote: bool) -> Vec<StringPart> {
        let mut parts = Vec::new();
        let mut literal = String::new();
        while self.pos < self.source.len() && !(stop_at_dquote && self.source[self.pos] == b'"') {
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
        parts
    }

    /// DD#65: scan the entire remaining input as a double-quoted-string body
    /// (no surrounding quotes; runs to end-of-input; a `"` is a literal byte).
    /// Used for the `${var:-…}` default, which *is* a `"…"` body whose
    /// terminator is the `}` the brace scanner already stripped.
    pub(crate) fn scan_default_body(&mut self) -> Vec<StringPart> {
        self.scan_interp_body(false)
    }

    /// Handle a `\` inside a `"…"` string. Per DD#64 the shell does **no**
    /// backslash processing in `"…"` — `\` is a verbatim literal byte. The
    /// one exception is `\<newline>` (line continuation), where both bytes
    /// vanish. `\"`, `\\`, `\$`, `` \` `` no longer collapse; a literal `"`,
    /// `$`, etc. is produced by quoting + concatenation (`'"'`, `'$'`).
    fn scan_dq_escape(&mut self, literal: &mut String) {
        if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'\n' {
            // Line continuation — both bytes vanish.
            self.pos += 2;
        } else {
            literal.push('\\');
            self.pos += 1;
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
            b'(' => self.scan_dq_paren(parts),
            b'\'' => self.scan_dq_cstring(literal),
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

    /// `$(…)` or `$((…))` — `(` is at `self.pos`. Pushes the appropriate
    /// part and advances past the closing paren(s).
    fn scan_dq_paren(&mut self, parts: &mut Vec<StringPart>) {
        if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'(' {
            let content = self.consume_dq_arith_sub();
            parts.push(StringPart::ArithSub(content));
        } else {
            let content = self.consume_dq_cmd_sub();
            parts.push(StringPart::CommandSub(content));
        }
    }

    /// DD#64 addendum: `$'…'` inside `"…"` is an escape-aware,
    /// compile-time-constant C-string. The opening `'` is at `self.pos`.
    /// Its resolved bytes become literal text in the concat (they merge
    /// with adjacent literal runs). `$'` claims its sequence, like `${`/`$(`.
    fn scan_dq_cstring(&mut self, literal: &mut String) {
        let dollar = self.pos - 1;
        self.pos += 1; // skip opening '
        if let Tok::AnsiCQuoted(s) = self.scan_ansi_c_quoted(dollar).token {
            literal.push_str(&s);
        }
    }

    /// `${…}` — opening `{` is at `self.pos`. Returns the inner text
    /// and advances past the closing `}`.
    fn consume_dq_brace_expansion(&mut self) -> String {
        self.pos += 1; // skip {
        self.scan_braced_content()
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

    /// A `` ` `` inside a `"…"` string is a **literal** backtick character:
    /// backticks are only top-level `$( )` sugar and have no special meaning in
    /// quotes (DG 2026-08-28). Emit the `` `…` `` span as a plain Literal (never
    /// a CommandSub — nothing in quotes should be executable via backticks).
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
        parts.push(StringPart::Literal(format!("`{}`", content)));
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
        // DD#69: a leading `@{…}` is a splat-disposition — scan it whole so
        // `parse_word_arm` decodes it (a bare `@{` never reaches here except
        // as `@` + `{`, which the byte dispatcher already routes).
        if self.source.get(self.pos) == Some(&b'@') && self.source.get(self.pos + 1) == Some(&b'{')
        {
            return self.scan_at_brace(start);
        }
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
        // Subscript-bracket depth within this word. A doubled `[[`/`]]` delimits
        // a `[[ … ]]` test/dispatch only at depth 0; inside a subscript (depth >
        // 0) a `]]` is two subscript closes — e.g. the bare nested cell read
        // `a[a[0]]` (arith model B) ends in `]]` and must stay one word.
        let mut depth = 0i32;
        while self.pos < self.source.len() {
            let b = self.source[self.pos];
            match b {
                b' ' | b'\t' | b'\n' | b';' | b'|' | b'&' | b'(' | b')' | b'{' | b'}' | b'<'
                | b'>' | b'#' | b'"' | b'\'' | b'`' | b'$' => break,
                // A bare leading `=` is its own token; otherwise `=` ends the word.
                b'=' if word.is_empty() => {
                    self.pos += 1;
                    return Some(SpannedToken {
                        token: Tok::Eq,
                        span: LexSpan {
                            start,
                            end: self.pos,
                        },
                    });
                }
                b'=' => break,
                // `+=` breaks the word (compound-assign operator).
                b'+' if self.source.get(self.pos + 1) == Some(&b'=') => break,
                // A doubled `[[` / `]]` at subscript-depth 0 delimits a
                // test/dispatch — break so the byte dispatcher tokenizes it
                // (`scan_lbracket`/`scan_rbracket`), making `[[ … ]]`
                // whitespace-optional like `(( … ))`.
                b'[' | b']' if depth == 0 && self.source.get(self.pos + 1) == Some(&b) => break,
                // Otherwise a bracket is a subscript char: track depth so a
                // nested subscript's closing `]]` stays within the word.
                b'[' => {
                    depth += 1;
                    word.push('[');
                    self.pos += 1;
                }
                b']' if depth > 0 => {
                    depth -= 1;
                    word.push(']');
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

/// Decode `bytes` to `&str` for the `logos` DFA (which needs `&str`, while the
/// scanner holds bytes). The dispatched structural byte is ASCII, but a trailing
/// comment may contain UTF-8; on invalid UTF-8 we hand logos the valid prefix
/// (enough to match the leading operator) and the remainder is re-dispatched
/// (likely `scan_word`). The matched byte length is what the caller uses.
fn logos_input(bytes: &[u8]) -> &str {
    match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            std::str::from_utf8(&bytes[..e.valid_up_to()]).expect("checked prefix is valid UTF-8")
        }
    }
}

/// Byte index just past the `[A-Za-z_][A-Za-z0-9_]*` identifier at `start`,
/// or `None` if there is no identifier there.
fn scan_ident_end(src: &[u8], start: usize) -> Option<usize> {
    let first = *src.get(start)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut i = start + 1;
    while i < src.len() && (src[i].is_ascii_alphanumeric() || src[i] == b'_') {
        i += 1;
    }
    Some(i)
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
