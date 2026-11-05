//! Lexer for sash-native syntax.
//!
//! Hand-written scanner with adjacency tracking. Produces `SpannedToken`
//! on demand — no intermediate token array, no lookahead limit.
//!
//! Design:
//!   - Tokens are designed for sash-native (not bash): int, const, array, map,
//!     attest, signal, EXIT, ERR, fn are first-class keywords. (`local fn` was
//!     abandoned by DD#86 — `local` is not a token, so it lexes as an
//!     ordinary word and resolution reports it as an unknown command.)
//!   - Heredocs are rejected at the scanner level (emits __HEREDOC__ sentinel).
//!   - Backticks are rejected (emits `backtick` sentinel; parser raises SB5076).
//!   - Single brackets `[` are NOT operators — they appear only inside words
//!     (array subscripts, case patterns). Only `[[` is a token.
//!   - Adjacency tracking: `next_is_adjacent()` lets the parser decide whether
//!     to concatenate tokens (e.g., `$foo"bar"` → single Concat expression).
//!     It also decides `{`: a brace glued to the previous word is SB4071
//!     (bash would keep it literal), a spaced one opens a block.
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
    Str,
    Int,
    Const,
    Array,
    Map,
    Attest,
    End,
    Exit,
    Err,

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
///
/// An `Expansion` or `CommandSub` is parsed again from its raw text, so it
/// carries the [`Origin`] that text starts at; positions inside it then name
/// the program rather than the substring.
#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Literal(String),
    Variable(String),           // $var
    Expansion(String, Origin),  // ${...} raw content
    CommandSub(String, Origin), // $(...) raw content
    ArithSub(String),           // $((...)) raw content
}

// ── Source origin ────────────────────────────────────────────────────

/// Where a scanner's text sits in the program, so that a nested scanner
/// reports positions in the program rather than in its own text.
///
/// A construct inside another one — the statements of a `$( … )` in a `"…"`
/// string, a `${a[…]}` index, a sed field — is parsed by re-lexing its text
/// with a fresh [`Scanner`]. Numbered from 1:1, everything such a scanner
/// reported was relative to the substring: `echo "$(strlen $n)"` on line 3
/// put its diagnostic at 1:1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Origin {
    /// The text is a verbatim copy of the source from this 1-based
    /// (line, column), so a position inside it maps back exactly.
    At(usize, usize),
    /// The text was rebuilt rather than copied — the arithmetic lexer
    /// re-serialises its tokens — so nothing inside it maps back. Every
    /// position reports the enclosing construct's, and so does every scanner
    /// nested inside this one.
    Within(usize, usize),
}

impl Origin {
    /// The origin of a whole program's source.
    pub const START: Origin = Origin::At(1, 1);

    /// The origin of the text that follows `prefix`, when `prefix` itself
    /// starts at `self`.
    pub fn after(self, prefix: &str) -> Origin {
        match self {
            Origin::At(line, col) => {
                let (line, col) = advance_line_col((line, col), prefix.as_bytes());
                Origin::At(line, col)
            }
            within @ Origin::Within(..) => within,
        }
    }
}

/// Step a 1-based (line, column) over `bytes`. A column counts bytes, not
/// characters — the unit [`Scanner::line_col`] has always reported.
fn advance_line_col((mut line, mut col): (usize, usize), bytes: &[u8]) -> (usize, usize) {
    for &b in bytes {
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
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
    /// Nonzero while parsing the statements inside a `$( … )` body. There an
    /// unquoted `)` is the substitution's own closer, so it must terminate a
    /// DD#71 absorb-mode word rather than join it — the same rule `]]`
    /// already enforces for a `[[ ]]` operand. Without it `$(int x=1)` ate
    /// its own `)` and was reported as an unterminated `$(`.
    cmd_sub_depth: usize,
    /// Nonzero while parsing a control-flow **header condition** (`if`,
    /// `elif`, `while`, `until`). There a `{` closes the condition and opens
    /// the body (DD#68), so it must terminate a command's word list even when
    /// it is glued to the previous word — `if /bin/true a{echo y}` is legal.
    /// Outside a header an adjacent `{` is instead SB4071, because it means
    /// the author wrote a brace *inside a word* and bash would have kept it
    /// literal (`echo a{echo z}`).
    header_depth: usize,
    /// First construct whose closing delimiter ran off the end of the source:
    /// `(offset of the opener, what it was)`. The scan-to-closer helpers all
    /// stopped at end-of-input and silently pretended they had found the
    /// closer, so `/bin/echo "hello` and `echo ${x` compiled clean. First one
    /// wins — later ones are consequences of the first.
    unterminated: Option<(usize, &'static str)>,
    /// Offset of the first `#` that sits immediately after an operator with no
    /// space. bash would start a comment there; sash does not (see
    /// [`attached_hash_word`]). Reported as SB4058 so the author is not left
    /// guessing why the rest of the line became code.
    attached_hash: Option<usize>,
    /// First `$'…'` escape this scan rejected: `(offset of the `\`, why)`.
    /// The escape is resolved here, so nothing downstream can see it; without
    /// this channel a dropped bash escape could only fail silently. First one
    /// wins, as with `unterminated`.
    bad_escape: Option<(usize, String)>,
    /// Where `source` sits in the program (see [`Origin`]). Every position
    /// this scanner reports — token spans and its own fatal diagnostics alike
    /// — is counted from here.
    origin: Origin,
    /// Set once the parser has skipped ahead after an error (see
    /// [`Self::mark_recovered`]). From there on it no longer knows which
    /// blocks are open, so it cannot tell a stray block keyword (`fi`,
    /// `done`, …) from the closer of a block whose opener it skipped.
    recovered: bool,
}

impl Scanner {
    pub fn new(source: &str) -> Self {
        Self::with_origin(source, Origin::START)
    }

    /// A scanner over text that starts at `origin` in the program: the
    /// re-lex of a nested construct.
    pub fn with_origin(source: &str, origin: Origin) -> Self {
        Self {
            source: source.as_bytes().to_vec(),
            pos: 0,
            peeked: Vec::new(),
            last_end: 0,
            arith_depth: 0,
            cmd_sub_depth: 0,
            header_depth: 0,
            unterminated: None,
            attached_hash: None,
            bad_escape: None,
            origin,
            recovered: false,
        }
    }

    /// Step past the closing delimiter of a construct opened at `open_at`, or
    /// record it as unterminated when the scan ran off the end of the source.
    fn require_closer(&mut self, open_at: usize, what: &'static str) {
        if self.pos < self.source.len() {
            self.pos += 1;
            return;
        }
        self.record_unterminated(open_at, what);
    }

    fn record_unterminated(&mut self, open_at: usize, what: &'static str) {
        if self.unterminated.is_none() {
            self.unterminated = Some((open_at, what));
        }
    }

    /// The first unterminated construct the scan hit, if any. `parse` reports
    /// it ahead of everything else: an unterminated quote or `${` swallows the
    /// rest of the program, so any other diagnostic is downstream noise.
    pub fn unterminated(&self) -> Option<(usize, &'static str)> {
        self.unterminated
    }

    /// The first `$'…'` escape the scan rejected, if any.
    pub fn bad_escape(&self) -> Option<(usize, &str)> {
        self.bad_escape
            .as_ref()
            .map(|(at, why)| (*at, why.as_str()))
    }

    /// Offset of the first operator-adjacent `#` (see [`attached_hash_word`]),
    /// if the scan hit one.
    pub fn attached_hash(&self) -> Option<usize> {
        self.attached_hash
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

    /// Enter a control-flow header condition (`if`/`elif`/`while`/`until`).
    /// Must be paired with [`exit_header`]. While active, a `{` terminates a
    /// command's word list even when adjacent to the previous word, because
    /// there it opens the block body (DD#68).
    pub fn enter_header(&mut self) {
        self.header_depth += 1;
    }

    /// Leave a control-flow header condition (see [`enter_header`]).
    pub fn exit_header(&mut self) {
        self.header_depth = self.header_depth.saturating_sub(1);
    }

    /// True while parsing a control-flow header condition.
    pub fn in_header(&self) -> bool {
        self.header_depth > 0
    }

    /// Enter a `$( … )` body. Must be paired with [`exit_cmd_sub`].
    pub fn enter_cmd_sub(&mut self) {
        self.cmd_sub_depth += 1;
    }

    /// Leave a `$( … )` body (see [`enter_cmd_sub`]).
    pub fn exit_cmd_sub(&mut self) {
        self.cmd_sub_depth = self.cmd_sub_depth.saturating_sub(1);
    }

    /// Whether the cursor is inside a `$( … )` body, where `)` is a closer
    /// rather than an absorbable metacharacter.
    pub fn in_cmd_sub(&self) -> bool {
        self.cmd_sub_depth > 0
    }

    /// Record that the parser skipped ahead after an error.
    pub fn mark_recovered(&mut self) {
        self.recovered = true;
    }

    /// Whether the parser has skipped ahead after an error, so that the
    /// blocks it believes open may not be the source's.
    pub fn has_recovered(&self) -> bool {
        self.recovered
    }

    /// Compute (line, col) from byte offset. Both 1-based, and counted from
    /// this scanner's [`Origin`], so a nested scanner names program positions.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        match self.origin {
            Origin::At(line, col) => {
                advance_line_col((line, col), &self.source[..offset.min(self.source.len())])
            }
            Origin::Within(line, col) => (line, col),
        }
    }

    /// The origin of this scanner's text from `offset` on, for the nested
    /// scanner that re-lexes it. Exact while this text is a verbatim copy;
    /// once an enclosing text was rebuilt, the whole nest reports that
    /// construct's position.
    pub fn origin_of(&self, offset: usize) -> Origin {
        match self.origin {
            Origin::At(..) => {
                let (line, col) = self.line_col(offset);
                Origin::At(line, col)
            }
            within @ Origin::Within(..) => within,
        }
    }

    /// The source byte at `offset`, if there is one.
    pub fn byte_at(&self, offset: usize) -> Option<u8> {
        self.source.get(offset).copied()
    }

    /// Source text between two byte offsets. Used by the assignment-LHS
    /// subscript parser to recover the raw index text from the token spans, so
    /// it can re-parse it exactly as the read path does.
    pub fn source_slice(&self, start: usize, end: usize) -> &str {
        std::str::from_utf8(&self.source[start..end]).unwrap_or("")
    }

    /// Consumed source only: lookahead and following trivia are excluded.
    pub fn consumed_source_since(&self, start: usize) -> &str {
        self.source_slice(start, self.last_end)
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
                b'\\' => {
                    if !self.take_line_continuation() {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    /// Consume a line continuation at the cursor, reporting whether there was
    /// one. Per DD#64 the shell reads a backslash in exactly two places, and
    /// one of them is `\` at end-of-line — where *both* bytes vanish.
    ///
    /// End-of-*input* counts: the last line of a file is at end-of-line
    /// whether or not it carries a newline, and bash discards a trailing
    /// backslash the same way (`echo a\` with no final newline prints `a`).
    ///
    /// One definition for every context. `skip_whitespace` and
    /// `scan_word_chars` had separate copies and the second one was missing,
    /// so a word split across a continuation silently became two statements.
    fn take_line_continuation(&mut self) -> bool {
        if self.source[self.pos] != b'\\' {
            return false;
        }
        match self.source.get(self.pos + 1) {
            None => self.pos += 1,
            Some(b'\n') => self.pos += 2,
            Some(_) => return false,
        }
        true
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
        use super::logos_tokens::LogosTok;
        use logos::Logos;

        if let Some(tok) = self.arith_hash_word(start) {
            return tok;
        }
        if let Some(tok) = self.attached_hash_word(start) {
            return tok;
        }
        let str_slice = logos_input(&self.source[self.pos..]);
        let mut lex = LogosTok::lexer(str_slice);
        let matched = lex.next();
        // Read the match length while `lex` is still alive — this releases its
        // borrow of `self.source` before the `&mut self` call below.
        let len = lex.span().end;
        match matched {
            Some(Ok(lt)) => self.emit_logos_token(lt, len, start),
            // The caller dispatched one of ten structural bytes, every one of
            // which the DFA matches (pinned by
            // `logos_matches_every_dispatched_structural_byte`). A miss is a
            // compiler bug, not bad input; the old silent fall-back to
            // `scan_word` produced `Tok::Eof` and truncated the program
            // mid-source.
            other => panic!(
                "logos failed to match dispatched structural byte {:?}: {:?}",
                self.source.get(self.pos),
                other
            ),
        }
    }

    /// A `#` that is **not** preceded by whitespace is an ordinary word
    /// character, not a comment introducer. Without this the scanner truncated
    /// every unquoted word at an interior `#` and ate the rest of the line as a
    /// comment — `echo a#b` printed `a`, `echo 16#ff` printed `16`. Silently:
    /// no diagnostic, a binary that disagrees with bash.
    ///
    /// DD#72 fixed exactly this collision inside `$(( ))` (see
    /// [`arith_hash_word`]) and stopped there; the same hole outside arith
    /// survived.
    ///
    /// **The rule is sash's, not bash's.** A comment opens at start of input or
    /// after a blank. bash also opens one after an *operator*, so `echo a;# c`
    /// comments in bash and does not here. That divergence is deliberate —
    /// requiring the space is the spelling every readable script already uses,
    /// and one rule covers both the word case and the expansion case
    /// (`$s#y`, `${s}#y`, `$(p)#z`) that a byte-level bash emulation would not.
    /// It is recorded as [`attached_hash`] so it is reported (SB4058) rather
    /// than left to surface as a baffling "command '#' not found".
    ///
    /// `)` and `}` are deliberately NOT operators here: they far more often
    /// close an expansion (`${s}#y`, `$(p)#z`), and bash agrees that a `#`
    /// after them continues the word.
    ///
    /// `None` when the byte is not `#`, or when it opens a genuine comment
    /// (start of input, or preceded by a blank or a newline).
    fn attached_hash_word(&mut self, start: usize) -> Option<SpannedToken> {
        if self.source[self.pos] != b'#' || self.hash_starts_comment() {
            return None;
        }
        if matches!(
            self.source[..self.pos].last(),
            Some(b';' | b'&' | b'|' | b'<' | b'>' | b'(')
        ) {
            self.attached_hash.get_or_insert(self.pos);
        }
        Some(self.scan_word(start))
    }

    fn hash_starts_comment(&self) -> bool {
        matches!(
            self.source[..self.pos].last(),
            Some(b' ' | b'\t' | b'\n') | None
        )
    }

    /// Turn a matched `logos` token of `len` bytes into a `SpannedToken`.
    fn emit_logos_token(
        &mut self,
        lt: super::logos_tokens::LogosTok,
        len: usize,
        start: usize,
    ) -> SpannedToken {
        use super::logos_tokens::{to_tok, LogosTok};

        let end = self.pos + len;
        let token = if matches!(lt, LogosTok::Comment) {
            Tok::Comment(String::from_utf8_lossy(&self.source[self.pos..end]).to_string())
        } else {
            to_tok(lt)
        };
        self.pos = end;
        SpannedToken {
            token,
            span: LexSpan { start, end },
        }
    }

    /// DD#72/DD#94: inside `$(( ))` (`arith_depth > 0`), a `#` **immediately
    /// after an alnum** is the base-literal marker — emit it as a single-char
    /// word so the arith re-lexer sees (and rejects) `base#digits` instead of
    /// the scanner truncating at the `#`. A `#` that is *not* alnum-adjacent is
    /// an ordinary comment (DD#94 package A): it falls through to the normal
    /// comment path, so `(( a = 1  # note` works like a comment anywhere else.
    /// `None` when not in arith mode or the byte is not `#`.
    fn arith_hash_word(&mut self, start: usize) -> Option<SpannedToken> {
        if self.arith_depth == 0 || self.source[self.pos] != b'#' {
            return None;
        }
        // `16#ff` — the loud DD#72 reject. Without this the `#` would start a
        // comment and silently truncate the literal to `16`.
        let prev_is_alnum = self.pos > 0 && self.source[self.pos - 1].is_ascii_alphanumeric();
        if !prev_is_alnum {
            return None;
        }
        self.pos += 1;
        Some(SpannedToken {
            token: Tok::Word("#".to_string()),
            span: LexSpan {
                start,
                end: self.pos,
            },
        })
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

    /// DD#67 (DG, 2026-09-14): backticks are **dropped**, not sugar. The
    /// mapping to `$(cmd)` is not 1-to-1 — nesting needs `\``, which DD#64
    /// removes — so the form fails DD#67's own keep-as-sugar test. Every
    /// backtick scans to the `` `backtick` `` sentinel; the parser rejects it
    /// with SB5076 and `sash-convert` rewrites the source to `$( … )`.
    fn scan_backtick(&mut self, start: usize) -> SpannedToken {
        self.pos += 1; // opening `
        self.skip_backtick_body();
        if self.pos < self.source.len() && self.source[self.pos] == b'`' {
            self.pos += 1; // closing `
        }
        SpannedToken {
            token: Tok::Word("`backtick`".to_string()),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Advance past a backtick body to the closing `` ` `` / newline / EOF.
    /// The content is discarded — the whole construct is rejected — but a
    /// `\`` is still stepped over so an escaped inner backtick does not end
    /// the scan early and strand the rest of the line as garbage tokens.
    fn skip_backtick_body(&mut self) {
        while self.pos < self.source.len()
            && self.source[self.pos] != b'`'
            && self.source[self.pos] != b'\n'
        {
            if self.is_escaped_backtick() {
                self.pos += 2;
            } else {
                self.pos += 1;
            }
        }
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
                let value = self.consume_ansi_c_quoted(start);
                SpannedToken {
                    token: Tok::AnsiCQuoted(value),
                    span: LexSpan {
                        start,
                        end: self.pos,
                    },
                }
            }
            b'(' => self.scan_dollar_paren(start),
            b'{' => {
                // ${...}
                self.pos += 1; // skip {
                self.scan_brace_expansion(start)
            }
            b'@' | b'*' | b'#' | b'?' | b'!' | b'$' | b'0' => {
                let var = (self.source[self.pos] as char).to_string();
                self.pos += 1;
                self.dollar_name_token(start, var, false)
            }
            b'1'..=b'9' => {
                // Positional param — could be multi-digit in rare cases but bash only does single
                let var = (self.source[self.pos] as char).to_string();
                self.pos += 1;
                self.dollar_name_token(start, var, true)
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let name_end = scan_ident_end(&self.source, self.pos).unwrap_or(self.pos);
                let name = String::from_utf8_lossy(&self.source[self.pos..name_end]).to_string();
                self.pos = name_end;
                self.dollar_name_token(start, name, true)
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

    /// `$(` / `$((` — command / arithmetic substitution opener. `self.pos` is
    /// at the `(`.
    fn scan_dollar_paren(&mut self, start: usize) -> SpannedToken {
        let dbl = self.source.get(self.pos + 1) == Some(&b'(');
        self.pos += if dbl { 2 } else { 1 };
        let token = if dbl {
            Tok::DollarDblParen
        } else {
            Tok::DollarParen
        };
        SpannedToken {
            token,
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// A plain `$var` token (no brace-less disposition), spanning `start..pos`.
    fn plain_dollar(&self, start: usize, var: String) -> SpannedToken {
        SpannedToken {
            token: Tok::Dollar(var),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Finish a `$name` whose name is already scanned (`self.pos` is just past
    /// it). A following `[` makes it a brace-less subscript; otherwise the
    /// spelling falls through to a plain `$name`, taking a brace-less
    /// disposition (DD#90) only where one is in scope — `$@` and friends are
    /// not, which is the whole of `dispositions`.
    fn dollar_name_token(
        &mut self,
        start: usize,
        name: String,
        dispositions: bool,
    ) -> SpannedToken {
        if let Some(tok) = self.braceless_subscript(start, &name, dispositions) {
            return tok;
        }
        if dispositions {
            self.dollar_or_braceless_disposition(start, name)
        } else {
            self.plain_dollar(start, name)
        }
    }

    /// After a bare `$name` / `$N` / `$#` (with `self.pos` just past the name),
    /// a `[` opens a **brace-less subscript**: synthesize the braced spelling
    /// `DollarBrace("name[idx]")` and consume through the matching `]`.
    ///
    /// This is the same desugar [`dollar_or_braceless_disposition`] performs for
    /// `$x:-foo`, and it exists for the same reason: downstream there is then
    /// exactly **one** subscript grammar — the braced one — and the brace-less
    /// spelling cannot drift away from it (DG, 2026-10-03: "all paths must
    /// agree"). What the three hand-rolled token-level paths this replaces got
    /// wrong was not one rule but five crashes and a whole index grammar:
    /// `$a[$(( i + 1 ))]` / `$a[$(f)]` / `$a["k"]` never parsed as subscripts at
    /// all, and `$#[0]`, `$?[0]`, `$$[0]`, `$![0]`, `$1[0]` each reached codegen
    /// as a subscript on an array named `#` and panicked (`ICE: undefined
    /// array`) — the braced forms rejecting all of them cleanly the whole time.
    ///
    /// **Inside `(( … ))` too** (DG, 2026-10-04). It did not used to be: the
    /// arith body is re-serialised through [`tok_to_text`] and re-lexed, and
    /// when `$a[$i]` reached that round trip as *separate* tokens the index was
    /// re-spaced to `$a[ $i ]` and silently re-read as `a[0]` — so SB4035
    /// rejected the whole spelling rather than risk a wrong answer. Fusing the
    /// subscript into one token here is what removed that hazard: the index now
    /// survives the round trip verbatim, and every spelling lands on the braced
    /// grammar with the braced rejects (`$a[i+1]` is SB4041 exactly as
    /// `${a[i+1]}` is). The old carve-out was a workaround for a defect this
    /// function had already fixed. Specs `dsx.1`-`dsx.5`.
    ///
    /// A trailing disposition is folded in as well, so `$a[0]:-z` desugars to
    /// `${a[0]:-z}` — "a string cell behaves like a string variable" (DG), and
    /// that settles every operator rather than just the one that was asked
    /// about. It needs no cell-specific legibility limit: the operand boundary
    /// is [`scan_braceless_operand`]'s word boundary, the same one a scalar
    /// uses, and the `:` must be tight against the `]` exactly as it must be
    /// tight against a scalar's name. Where that adjacency collides with the
    /// arithmetic ternary the result is a loud SB4020, never a silent
    /// reinterpretation. `dispositions` is false for `$@` and friends, and the
    /// double-quote scanner never reaches here at all, so interpolation keeps
    /// its mandatory braces (DD#90, DG: "err on the side of clarity").
    ///
    /// Returns `None` when no `[` follows or the brackets are unbalanced, so
    /// `echo $a[` keeps its literal reading.
    fn braceless_subscript(
        &mut self,
        start: usize,
        name: &str,
        dispositions: bool,
    ) -> Option<SpannedToken> {
        let close = super::expr::balanced_subscript_close(&self.source, self.pos)?;
        let mut text = String::from_utf8_lossy(&self.source[self.pos..=close]).to_string();
        self.pos = close + 1;
        if dispositions {
            if let Some((op, operand, end)) = self.try_braceless_disposition(self.pos) {
                self.pos = end;
                text.push_str(&format!(":{op}{operand}"));
            }
        }
        Some(SpannedToken {
            token: Tok::DollarBrace(format!("{name}{text}")),
            span: LexSpan {
                start,
                end: self.pos,
            },
        })
    }

    /// After a bare `$name` / `$N` (with `self.pos` just past the name), emit
    /// either a plain `Dollar(name)` or — if a brace-less disposition follows —
    /// a synthesized `DollarBrace("name:OP…")` so the whole `$x:-foo` / `$x:~`
    /// desugars to the braced form and reuses every downstream path
    /// (DD#65/#69/#84; DG 2026-09-04). Only `:-`, `:?`, `:~` immediately after
    /// the name open a disposition; any other `:X` stays literal concatenation
    /// (so `$x:/bin`, `$x:3` are unaffected). Inside `"…"` this path is not
    /// reached (the double-quote scanner keeps braces mandatory — clarity).
    fn dollar_or_braceless_disposition(&mut self, start: usize, name: String) -> SpannedToken {
        if let Some((op, operand, end)) = self.try_braceless_disposition(self.pos) {
            self.pos = end;
            return SpannedToken {
                token: Tok::DollarBrace(format!("{}:{}{}", name, op, operand)),
                span: LexSpan {
                    start,
                    end: self.pos,
                },
            };
        }
        self.plain_dollar(start, name)
    }

    /// If `src[pos..]` opens a brace-less disposition, return `(op, operand,
    /// end)` where `op` is the operator text and `end` is just past the
    /// consumed text. `:~` (elide) takes no operand. `:-`/`:?` scan the operand
    /// to the word boundary; returns `None` (→ caller falls back to the plain
    /// form) when the operand would contain a `$( )` / `$(( ))` / `` `…` ``
    /// substitution, which must use braces.
    ///
    /// DD#91 addendum: a leading `!` is the taint-off MODIFIER, so it may lead
    /// a disposition (`$x:!~`, `$x:!-W`) or stand alone (`$x:!`, which is the
    /// value trusted). The op text is re-serialized into the braced spelling by
    /// both callers, so `!` simply rides along and the braced parser does the
    /// real work — one grammar, not two.
    fn try_braceless_disposition(&self, pos: usize) -> Option<(String, String, usize)> {
        if self.source.get(pos) != Some(&b':') {
            return None;
        }
        let trusted = self.source.get(pos + 1) == Some(&b'!');
        let at = if trusted { pos + 2 } else { pos + 1 };
        match (self.braceless_disposition_body(at), trusted) {
            (Some((op, operand, end)), true) => Some((format!("!{op}"), operand, end)),
            (Some(found), false) => Some(found),
            // `$x:!` — the modifier with no disposition.
            (None, true) => Some(("!".to_string(), String::new(), pos + 2)),
            (None, false) => None,
        }
    }

    /// The disposition itself, without the `:` or any `!` modifier.
    fn braceless_disposition_body(&self, at: usize) -> Option<(String, String, usize)> {
        match self.source.get(at) {
            Some(b'~') => Some(("~".to_string(), String::new(), at + 1)),
            Some(b'-') => self
                .scan_braceless_operand(at + 1)
                .map(|(o, e)| ("-".to_string(), o, e)),
            Some(b'?') => self
                .scan_braceless_operand(at + 1)
                .map(|(o, e)| ("?".to_string(), o, e)),
            _ => None,
        }
    }

    /// Scan a brace-less disposition operand from `from` to the word boundary.
    /// Literal bytes and `$name` / `${…}` interpolation are consumed (quotes are
    /// ordinary literal bytes, DD#65); an unnested whitespace or word-terminator
    /// (`;|&()<>{}#`) ends it. A `$( )` / `$(( ))` / `` `…` `` is consumed whole
    /// as part of the operand (DD#91 follow-up, DG 2026-09-04 — DD#90's original
    /// rule 4 required braces for these; see specs vm.bl13–vm.bl18). `None` only
    /// when such a run is *unterminated*, which makes the operand scan fail and
    /// the word fall back to the pre-DD#90 tokenization.
    fn scan_braceless_operand(&self, from: usize) -> Option<(String, usize)> {
        let s = &self.source;
        let mut p = from;
        while p < s.len() {
            match s[p] {
                b' ' | b'\t' | b'\n' | b';' | b'|' | b'&' | b'(' | b')' | b'<' | b'>' | b'{'
                | b'}' | b'#' => break,
                b'`' => p = self.skip_backtick_to_close(p)?,
                b'$' => p = self.braceless_operand_after_dollar(p)?,
                _ => p += 1,
            }
        }
        Some((String::from_utf8_lossy(&s[from..p]).to_string(), p))
    }

    /// Consume the `$…` at `p` inside a brace-less operand: `${…}` (to its
    /// close), a `$( )` / `$(( ))` command / arithmetic substitution (to the
    /// matching close), a `$name` / `$N` / `$@`-style var, or a bare `$`.
    /// Returns the position just past the run (`None` if a substitution is
    /// unterminated → operand scan fails, use braces).
    fn braceless_operand_after_dollar(&self, p: usize) -> Option<usize> {
        match self.source.get(p + 1) {
            Some(b'{') => self.skip_braced_to_close(p + 2),
            Some(b'(') => self.skip_paren_to_close(p + 1),
            _ => {
                let end = self.skip_dollar_var(p + 1);
                Some(if end > p + 1 { end } else { p + 1 })
            }
        }
    }

    /// Skip a `$( … )` / `$(( … ))` from the opening `(` at `from` to just past
    /// the matching close, by paren depth (quoted regions are skipped so a `)`
    /// inside `'…'`/`"…"` does not miscount). `None` if unterminated.
    fn skip_paren_to_close(&self, from: usize) -> Option<usize> {
        let s = &self.source;
        let mut p = from;
        let mut depth = 0i32;
        while p < s.len() {
            match s[p] {
                b'(' => {
                    depth += 1;
                    p += 1;
                }
                b')' => {
                    depth -= 1;
                    p += 1;
                    if depth == 0 {
                        return Some(p);
                    }
                }
                b'\'' | b'"' => p = self.skip_simple_quote(p)?,
                _ => p += 1,
            }
        }
        None
    }

    /// Skip a `'…'` / `"…"` from the opening quote at `p` to just past the
    /// close (no escape processing — matches DD#65's literal-byte view enough
    /// for the paren scan). `None` if unterminated.
    fn skip_simple_quote(&self, p: usize) -> Option<usize> {
        let s = &self.source;
        let q = s[p];
        let mut i = p + 1;
        while i < s.len() {
            if s[i] == q {
                return Some(i + 1);
            }
            i += 1;
        }
        None
    }

    /// Skip a `` `…` `` backtick substitution from the opening backtick at `p`
    /// to just past the close. `None` if unterminated.
    fn skip_backtick_to_close(&self, p: usize) -> Option<usize> {
        let s = &self.source;
        let mut i = p + 1;
        while i < s.len() {
            if s[i] == b'`' {
                return Some(i + 1);
            }
            i += 1;
        }
        None
    }

    /// Skip a `${…}` from `p` (just past the opening `{`) to just past the
    /// matching `}`. `None` if unterminated (→ operand scan fails, braces).
    fn skip_braced_to_close(&self, mut p: usize) -> Option<usize> {
        let s = &self.source;
        let mut depth = 1i32;
        while p < s.len() {
            match s[p] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(p + 1);
                    }
                }
                _ => {}
            }
            p += 1;
        }
        None
    }

    /// Skip a `$`-suffix variable at `p` (the byte after `$`): a single-char
    /// special (`@ * # ? ! $` or a positional digit) or an identifier run.
    /// Returns the position just past it (or `p` if there is no variable).
    fn skip_dollar_var(&self, p: usize) -> usize {
        let s = &self.source;
        match s.get(p) {
            Some(&c)
                if matches!(c, b'@' | b'*' | b'#' | b'?' | b'!' | b'$') || c.is_ascii_digit() =>
            {
                p + 1
            }
            Some(&c) if c.is_ascii_alphabetic() || c == b'_' => {
                let mut q = p + 1;
                while q < s.len() && (s[q].is_ascii_alphanumeric() || s[q] == b'_') {
                    q += 1;
                }
                q
            }
            _ => p,
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
        self.advance_to_unnested_close();
        let content = String::from_utf8_lossy(&self.source[inner_start..self.pos]).to_string();
        // Shared with the DD#69 `@{a:-WORD}` splat disposition, so name the
        // opener the source actually used: an unclosed `@{` reported
        // "unterminated `${`", pointing the reader at a sigil they had not
        // written.
        let open_at = inner_start.saturating_sub(2);
        let what = match self.source.get(open_at) {
            Some(b'@') => "`@{`",
            _ => "`${`",
        };
        self.require_closer(open_at, what);
        content
    }

    /// Advance to the `}` that closes the brace opened just before
    /// `self.pos`, counting nested `{`/`}` pairs. Stops *at* that byte, or at
    /// end-of-source if it never arrives (the caller diagnoses that).
    fn advance_to_unnested_close(&mut self) {
        let mut depth = 1i32;
        while let Some(&b) = self.source.get(self.pos) {
            depth += match b {
                b'{' => 1,
                b'}' => -1,
                _ => 0,
            };
            if depth == 0 {
                return;
            }
            self.pos += 1;
        }
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

    /// DD#63 Path B for the *statement* form `v =~ s/RE/SUB/[flags]`: take the
    /// payload verbatim from the source instead of stitching it back from
    /// tokens. Token reassembly broke on anything the accumulator did not
    /// enumerate — `$'…'`, and every metacharacter that lexes as an operator
    /// (`=` `#` `&` `<` `>` `;`) — producing SB4032 on a well-formed payload.
    /// The `${v:s/…}` form has always scanned raw source (`scan_sed_payload`);
    /// this is the same thing for the statement form, so the two agree.
    ///
    /// Rewinds to just past the last consumed token (dropping any peeked
    /// lookahead), skips blanks, and requires `s/` at that point. Returns the
    /// payload text with the [`Origin`] it starts at, and leaves the scanner
    /// just past it. `None` if this is not an `s/` payload or a delimiter is
    /// missing — the caller raises SB4032.
    pub fn take_sed_stmt_payload(&mut self) -> Option<(String, Origin)> {
        self.peeked.clear();
        self.pos = self.last_end;
        self.skip_blanks_only();
        let start = self.pos;
        let end = self.scan_sed_chain(start)?;
        let raw = String::from_utf8_lossy(&self.source[start..end]).to_string();
        self.pos = end;
        self.last_end = end;
        Some((raw, self.origin_of(start)))
    }

    /// Blanks only — a newline ends the statement, so it must not be skipped
    /// while looking for the payload's start.
    fn skip_blanks_only(&mut self) {
        while self.pos < self.source.len()
            && (self.source[self.pos] == b' ' || self.source[self.pos] == b'\t')
        {
            self.pos += 1;
        }
    }

    /// One or more `,`-separated `s/…/…/[flags]` edits (DD#96). Returns the
    /// index just past the last edit's flags.
    fn scan_sed_chain(&self, start: usize) -> Option<usize> {
        let mut end = self.scan_one_sed_edit(start)?;
        loop {
            // A chain continues only while an edit is owed — the next
            // non-blank byte is the `,` separator.
            let comma = self.skip_bytes(end, b" \t");
            if self.source.get(comma) != Some(&b',') {
                // The payload must end HERE. DD#63 is explicit that a
                // malformed payload is a compile error, not a silent fallback
                // (`s/-/x/y/` and `s/-/x/zz9` were SB4032 before this scan
                // existed). Without this the leftover bytes lex as a fresh
                // statement, so `s =~ s/-/x/y/` built a program that tried to
                // execute `/`.
                return self.sed_payload_ends_cleanly(comma).then_some(end);
            }
            // Blanks and a newline may follow it (DD#96 continuation).
            end = self.scan_one_sed_edit(self.skip_bytes(comma + 1, b" \t\n\r"))?;
        }
    }

    /// First index at or after `from` whose byte is not in `set`.
    fn skip_bytes(&self, from: usize, set: &[u8]) -> usize {
        let mut j = from;
        while j < self.source.len() && set.contains(&self.source[j]) {
            j += 1;
        }
        j
    }

    /// Index just past the delimiter closing SUB, scanning from just past the
    /// one that opened RE. `None` if a newline or EOF arrives first.
    fn scan_sed_fields(&self, from: usize) -> Option<usize> {
        let mut j = from;
        let mut slashes = 0u8;
        while let Some(&b) = self.source.get(j) {
            j += 1;
            match b {
                b'\n' | b'\r' => return None, // unterminated payload
                b'/' if slashes == 1 => return Some(j),
                b'/' => slashes = 1,
                _ => {}
            }
        }
        None
    }

    /// First index at or after `from` whose byte fails `pred`.
    fn skip_while(&self, from: usize, pred: fn(&u8) -> bool) -> usize {
        let mut j = from;
        while self.source.get(j).is_some_and(pred) {
            j += 1;
        }
        j
    }

    /// True when `j` is a legitimate end for a sed payload: end of input, or a
    /// byte that ends the statement. A `)` closes an enclosing substitution.
    fn sed_payload_ends_cleanly(&self, j: usize) -> bool {
        matches!(
            self.source.get(j),
            None | Some(b'\n' | b'\r' | b';' | b'|' | b'&' | b')')
        )
    }

    /// One `s/RE/SUB/[flags]` edit from `start`. `None` when the text does not
    /// open with `s/`, or a newline/EOF arrives before both delimiters.
    fn scan_one_sed_edit(&self, start: usize) -> Option<usize> {
        let src = &self.source;
        if start + 2 > src.len() || &src[start..start + 2] != b"s/" {
            return None;
        }
        let after_fields = self.scan_sed_fields(start + 2)?;
        // Flags are the run of letters after the closing delimiter.
        Some(self.skip_while(after_fields, u8::is_ascii_alphabetic))
    }

    /// Scan '...' single-quoted string.
    fn scan_single_quoted(&mut self, start: usize) -> SpannedToken {
        self.pos += 1; // skip opening '
        let content_start = self.pos;
        while self.pos < self.source.len() && self.source[self.pos] != b'\'' {
            self.pos += 1;
        }
        let content = String::from_utf8_lossy(&self.source[content_start..self.pos]).to_string();
        self.require_closer(start, "`'` string");
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
        self.require_closer(start, "`\"` string");
        SpannedToken {
            token: Tok::DoubleQuoted(parts),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Append the character at `self.pos` to `out`, advancing past its full
    /// UTF-8 sequence. `out.push(byte as char)` reads each byte as a Latin-1
    /// code point and re-encodes it (`0xC3` -> `U+00C3` -> `C3 83`), which
    /// double-encodes every non-ASCII literal in the source. Slicing the whole
    /// sequence and decoding it once matches `scan_single_quoted`.
    ///
    /// Source is always valid UTF-8 — a file goes through `read_to_string` and
    /// `-c` arrives as a `String` — and every scanner step advances by whole
    /// characters, so `self.pos` is always a lead byte. A continuation byte
    /// (`0x80..=0xBF`) or an invalid lead (`0xF8..=0xFF`) can only appear if
    /// that invariant is broken, and the old `_ => 1` answered it by splitting
    /// the character silently.
    fn push_source_char(&mut self, out: &mut String) {
        let b = self.source[self.pos];
        let len = match b {
            0x00..=0x7F => 1,
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            0x80..=0xBF | 0xF8..=0xFF => unreachable!(
                "source is valid UTF-8 and the scanner stands on a lead byte, \
                 so {b:#04x} cannot start a character here"
            ),
        };
        let end = (self.pos + len).min(self.source.len());
        out.push_str(&String::from_utf8_lossy(&self.source[self.pos..end]));
        self.pos = end;
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
                    self.push_source_char(&mut literal);
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
                let origin = self.origin_of(self.pos + 1);
                let content = self.consume_dq_brace_expansion();
                parts.push(StringPart::Expansion(content, origin));
            }
            b'(' => self.scan_dq_paren(parts),
            b'\'' => self.scan_dq_cstring(literal),
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let name = self.consume_dq_var_name();
                parts.push(self.dq_subscript_or_variable(name));
            }
            b'@' | b'*' | b'#' | b'?' | b'!' | b'$' | b'0'..=b'9' => {
                let var = (self.source[self.pos] as char).to_string();
                self.pos += 1;
                parts.push(self.dq_subscript_or_variable(var));
            }
            _ => {
                // Bare `$` followed by something unexpected — keep as
                // literal; don't consume the next byte.
                literal.push('$');
            }
        }
    }

    /// The in-string half of [`braceless_subscript`]: `"$a[K]"` becomes the
    /// `Expansion("a[K]")` part that `${a[K]}` would have produced, so the
    /// quoted and unquoted spellings meet the same grammar.
    ///
    /// Before this, the in-string reader required the index to arrive as a
    /// single trailing literal, which only an int literal or a bare key ever
    /// does. `echo $a[$i]` printed the element and `echo "$a[$i]"` was SB5064 —
    /// a hole inside the brace-less read's own stated scope, not a narrowing
    /// anyone chose (DD#33 read side, decision B, DG 2026-06-21).
    fn dq_subscript_or_variable(&mut self, name: String) -> StringPart {
        match super::expr::balanced_subscript_close(&self.source, self.pos) {
            Some(close) => {
                // `name` is the verbatim source just before the `[`.
                let origin = self.origin_of(self.pos - name.len());
                let text = String::from_utf8_lossy(&self.source[self.pos..=close]).to_string();
                self.pos = close + 1;
                StringPart::Expansion(format!("{name}{text}"), origin)
            }
            None => StringPart::Variable(name),
        }
    }

    /// `$(…)` or `$((…))` — `(` is at `self.pos`. Pushes the appropriate
    /// part and advances past the closing paren(s).
    fn scan_dq_paren(&mut self, parts: &mut Vec<StringPart>) {
        if self.pos + 1 < self.source.len() && self.source[self.pos + 1] == b'(' {
            let content = self.consume_dq_arith_sub();
            parts.push(StringPart::ArithSub(content));
        } else {
            let origin = self.origin_of(self.pos + 1);
            let content = self.consume_dq_cmd_sub();
            parts.push(StringPart::CommandSub(content, origin));
        }
    }

    /// DD#64 addendum: `$'…'` inside `"…"` is an escape-aware,
    /// compile-time-constant C-string. The opening `'` is at `self.pos`.
    /// Its resolved bytes become literal text in the concat (they merge
    /// with adjacent literal runs). `$'` claims its sequence, like `${`/`$(`.
    fn scan_dq_cstring(&mut self, literal: &mut String) {
        let dollar = self.pos - 1;
        self.pos += 1; // skip opening '
        literal.push_str(&self.consume_ansi_c_quoted(dollar));
    }

    /// `${…}` — opening `{` is at `self.pos`. Returns the inner text
    /// and advances past the closing `}`.
    fn consume_dq_brace_expansion(&mut self) -> String {
        self.pos += 1; // skip {
        self.scan_braced_content()
    }

    /// `$((…))` — caller has not yet advanced past `((`. Returns the
    /// inner arithmetic text using the same collector as an unquoted expansion.
    fn consume_dq_arith_sub(&mut self) -> String {
        self.pos += 2; // skip ((
        let open_at = self.pos - 3;
        // This can run while peek_n is building a token beyond older lookahead.
        // The nested token stream must not consume that queue or its adjacency.
        let peeked = std::mem::take(&mut self.peeked);
        let last_end = self.last_end;
        let unterminated = self.unterminated;
        let collected = super::arith::scan_arith_content(self);
        self.peeked = peeked;
        self.last_end = last_end;
        if !collected.closed {
            // Without `))`, the enclosing quote can look like an unclosed
            // inner string. That consequence must not replace the arith opener.
            if matches!(self.unterminated, Some((_, "`\"` string"))) {
                self.unterminated = unterminated;
            }
            self.record_unterminated(open_at, "`$((`");
        }
        collected.content
    }

    /// `$(…)` — caller has not yet advanced past `(`. Returns the
    /// inner command text and advances past the closing `)`.
    fn consume_dq_cmd_sub(&mut self) -> String {
        self.pos += 1; // skip (
        let inner_start = self.pos;
        let mut depth = 1i32;
        while self.pos < self.source.len() && depth > 0 {
            self.step_dq_cmd_sub(&mut depth);
        }
        let content = String::from_utf8_lossy(&self.source[inner_start..self.pos]).to_string();
        self.require_closer(inner_start.saturating_sub(2), "`$(`");
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

    /// One byte of a `"…$(…)…"` body: track paren depth, and skip nested
    /// quoted runs and comments whole so their `)` does not close the substitution.
    fn step_dq_cmd_sub(&mut self, depth: &mut i32) {
        match self.source[self.pos] {
            b'(' => {
                *depth += 1;
                self.pos += 1;
            }
            b')' => {
                *depth -= 1;
                if *depth > 0 {
                    self.pos += 1;
                }
            }
            b'\'' => self.skip_balanced_squote(),
            b'"' => {
                self.scan_double_quoted(self.pos);
            }
            b'$' if self.source.get(self.pos + 1) == Some(&b'\'') => {
                let start = self.pos;
                self.pos += 2;
                self.consume_ansi_c_quoted(start);
            }
            b'#' if self.hash_starts_comment() => {
                self.scan_via_logos(self.pos);
            }
            _ => {
                self.pos += 1;
            }
        }
    }

    /// Skip a single-quoted run starting at `self.pos`. Single quotes
    /// have no escapes; just advance until the closing `'`.
    fn skip_balanced_squote(&mut self) {
        let open_at = self.pos;
        self.pos += 1; // opening '
        while self.pos < self.source.len() && self.source[self.pos] != b'\'' {
            self.pos += 1;
        }
        self.require_closer(open_at, "`'` string");
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
        self.require_closer(inner_start.saturating_sub(1), "`` ` `` span");
        parts.push(StringPart::Literal(format!("`{}`", content)));
    }

    /// Scan $'...' ANSI-C quoted string (opening $' already consumed).
    fn consume_ansi_c_quoted(&mut self, start: usize) -> String {
        let bytes = self.scan_ansi_c_bytes();
        self.require_closer(start, "`$'` string");
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// True when `self.pos` starts a `\<byte>` escape with a byte to follow.
    fn at_escape_lead(&self) -> bool {
        self.source[self.pos] == b'\\' && self.pos + 1 < self.source.len()
    }

    /// Body of `$'…'` from `self.pos` to the closing `'`, which is left
    /// unconsumed.
    ///
    /// Accumulates raw bytes rather than `char`s: the body mixes escapes with
    /// literal source text, which is UTF-8 and may be multi-byte, so a byte
    /// buffer is the only representation both fit. Decoding the finished byte
    /// string once is what `scan_single_quoted` does too.
    ///
    /// Two escapes can no longer combine into one character — `\xNN` is capped
    /// at `\x7f`, and `\uHHHH` emits a whole character — so the buffer is
    /// always valid UTF-8 by construction.
    fn scan_ansi_c_bytes(&mut self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        while self.pos < self.source.len() && self.source[self.pos] != b'\'' {
            if self.at_escape_lead() {
                self.pos += 1;
                self.consume_ansi_c_escape(&mut bytes);
                continue;
            }
            bytes.push(self.source[self.pos]);
            self.pos += 1;
        }
        bytes
    }

    /// `\…` body inside `$'…'`. `self.pos` points at the byte after `\`.
    ///
    /// The escape set follows DD#72's model for authored constants: a clean
    /// modern set, and a **loud** rejection of the archaic bash spellings
    /// rather than a silent difference. Accepted: the named escapes
    /// (`simple_escape`), `\xNN` for an ASCII byte, and `\uHHHH`/`\UHHHHHHHH`
    /// for a character. Rejected: bash's octal (`\101`, `\0101`) and `\cX`.
    /// Anything bash does not read as an escape either (`\q`) stays a literal
    /// `\<byte>`, exactly as bash leaves it.
    fn consume_ansi_c_escape(&mut self, out: &mut Vec<u8>) {
        let c = self.source[self.pos];
        if let Some(ch) = simple_escape(c) {
            push_utf8(out, ch);
            self.pos += 1;
            return;
        }
        let at = self.pos - 1;
        match c {
            b'x' => {
                self.pos += 1;
                self.consume_ansi_c_hex(at, out);
            }
            b'u' => {
                self.pos += 1;
                self.consume_ansi_c_unicode(at, 4, out);
            }
            b'U' => {
                self.pos += 1;
                self.consume_ansi_c_unicode(at, 8, out);
            }
            b'0'..=b'7' => self.reject_escape(
                at,
                "octal escapes are not supported (`\\0`, `\\101`, `\\0101`) — write \
                 the byte as `\\xNN` hex (NUL is `\\x00`), or the character as \
                 `\\uHHHH`"
                    .to_string(),
            ),
            b'c' => self.reject_escape(
                at,
                "`\\cX` control escapes are not supported — write the byte as \
                 `\\xNN` hex (`\\cA` is `\\x01`)"
                    .to_string(),
            ),
            other => {
                out.push(b'\\');
                out.push(other);
                self.pos += 1;
            }
        }
    }

    /// Record the first rejected escape. `at` is the offset of its `\`.
    ///
    /// Deliberately does **not** move `self.pos`. The digit scanners have
    /// already consumed what they read, and an extra step there ate the
    /// closing `'` — which made a rejected `$'\xff'` report SB4057
    /// *unterminated* instead of the real reason. Where nothing was consumed
    /// the byte is left for `scan_ansi_c_bytes` to push as a literal, so the
    /// scan still makes progress and the string still closes; `parse` reports
    /// before any of that content is used.
    fn reject_escape(&mut self, at: usize, why: String) {
        if self.bad_escape.is_none() {
            self.bad_escape = Some((at, why));
        }
    }

    /// Up to `max` hex digits from `self.pos`, as a value. `None` when there
    /// is not even one — the prefix is then meaningless.
    fn take_hex_digits(&mut self, max: usize) -> Option<u32> {
        let mut val: u32 = 0;
        let mut seen = 0;
        while seen < max && self.pos < self.source.len() {
            let d = match self.source[self.pos] {
                b @ b'0'..=b'9' => b - b'0',
                b @ b'a'..=b'f' => b - b'a' + 10,
                b @ b'A'..=b'F' => b - b'A' + 10,
                _ => break,
            };
            val = val * 16 + u32::from(d);
            self.pos += 1;
            seen += 1;
        }
        (seen > 0).then_some(val)
    }

    /// `\xNN` — up to two hex digits, naming one **byte**. Capped at `\x7f`:
    /// a sash string is UTF-8 text, and a lone byte above ASCII is not text.
    /// A character above ASCII is `\uHHHH`, which encodes as valid UTF-8.
    fn consume_ansi_c_hex(&mut self, at: usize, out: &mut Vec<u8>) {
        let Some(val) = self.take_hex_digits(2) else {
            return self.reject_escape(at, "`\\x` needs at least one hex digit".to_string());
        };
        if val > 0x7f {
            return self.reject_escape(
                at,
                format!(
                    "`\\x{val:02x}` is above `\\x7f` — a lone byte above ASCII is not \
                     UTF-8 text; write the character as `\\uHHHH` instead"
                ),
            );
        }
        out.push(val as u8);
    }

    /// `\uHHHH` / `\UHHHHHHHH` — up to `max` hex digits naming a **character**,
    /// emitted as UTF-8. bash encodes surrogates and out-of-range values
    /// anyway, producing invalid UTF-8; those are rejected here instead.
    fn consume_ansi_c_unicode(&mut self, at: usize, max: usize, out: &mut Vec<u8>) {
        let Some(val) = self.take_hex_digits(max) else {
            return self.reject_escape(at, "`\\u`/`\\U` needs at least one hex digit".to_string());
        };
        let Some(ch) = char::from_u32(val) else {
            return self.reject_escape(
                at,
                format!(
                    "`U+{val:04X}` is not a character — a surrogate \
                     (`D800`-`DFFF`) or above `10FFFF` has no UTF-8 encoding"
                ),
            );
        };
        push_utf8(out, ch);
    }

    // ── Word scanning ─────────────────────────────────────────────────

    /// Scan a bare word token.
    /// Accumulates bytes until a word-breaking character, then checks
    /// the result against the keyword table.
    fn scan_word(&mut self, start: usize) -> SpannedToken {
        // A leading `@{…}` (DD#69 braced splat) or `@name:-/?/~` (DD#90
        // brace-less splat disposition) is scanned whole into an `@{…}` Word so
        // `parse_word_arm` decodes it.
        if self.source.get(self.pos) == Some(&b'@') {
            if let Some(tok) = self.try_scan_at_word(start) {
                return tok;
            }
        }
        let mut word = String::new();
        self.scan_word_chars(&mut word);

        assert!(
            !word.is_empty(),
            "scan_word at a word-break byte {:?} — the byte dispatcher owns \
             every break byte, so this is a dispatch bug",
            self.source.get(start)
        );

        SpannedToken {
            token: classify_word_token(word),
            span: LexSpan {
                start,
                end: self.pos,
            },
        }
    }

    /// Scan an `@`-led word form at `self.pos` (the `@`): `@{…}` (braced splat)
    /// or `@name:-/?/~` (brace-less disposition). Returns the synthesized
    /// `@{…}` Word, or `None` when this `@` is neither (caller does the normal
    /// word scan — bare `@a`, `@a.b`, etc.).
    fn try_scan_at_word(&mut self, start: usize) -> Option<SpannedToken> {
        if self.source.get(self.pos + 1) == Some(&b'{') {
            return Some(self.scan_at_brace(start));
        }
        self.try_scan_braceless_at_disposition(start)
    }

    /// `@name:-foo` / `@name:?msg` / `@name:~` (brace-less array-splat
    /// disposition) → a synthesized `Word("@{name:…}")` identical to what
    /// `scan_at_brace` produces for the braced spelling. `self.pos` is at the
    /// `@`. Returns `None` (caller falls back to the normal word scan) when the
    /// `@name` is not immediately followed by a `:-`/`:?`/`:~` disposition.
    fn try_scan_braceless_at_disposition(&mut self, start: usize) -> Option<SpannedToken> {
        let name_start = self.pos + 1;
        let name_end = scan_ident_end(&self.source, name_start)?;
        let (op, operand, end) = self.try_braceless_disposition(name_end)?;
        let name = String::from_utf8_lossy(&self.source[name_start..name_end]).to_string();
        self.pos = end;
        Some(SpannedToken {
            token: Tok::Word(format!("@{{{}:{}{}}}", name, op, operand)),
            span: LexSpan {
                start,
                end: self.pos,
            },
        })
    }

    /// Does the word end at byte `b`? Every break in one place, so the scan
    /// loop itself stays a loop and not a decision table.
    ///
    /// `=` ends a word; a *leading* `=` never reaches here, because the byte
    /// dispatcher routes `b'='` to `scan_eq_punct` and no caller of
    /// `scan_word` advances past its first byte. `+=` ends one too
    /// (compound-assign). A doubled `[[` / `]]` at subscript-depth 0 delimits
    /// a test/dispatch, so it breaks and the byte dispatcher tokenizes it —
    /// that is what makes `[[ … ]]` whitespace-optional like `(( … ))`.
    /// `#` ends a word only inside `$(( ))` (DD#72's base-prefix reject);
    /// elsewhere DD#110 makes it an ordinary word byte.
    ///
    /// `@` ends a word only when `{` follows, so a *mid*-word `@{…}` splat
    /// reaches the same diagnostic as a trailing one. Without the break,
    /// `y@{a:-x}` scanned `y@` as a word and let the already-breaking `{`
    /// open a brace group, so the splat was reported as `SB5011 command
    /// 'a:-x' not found` while `@{a:-x}y` correctly said SB5083. Breaking
    /// here makes the prefix form lex as `y` + `@{a:-x}`, which the adjacency
    /// pass joins into the same `Concat` the suffix form produces — so both
    /// spellings reach one check rather than two. A word-*leading* `@{`
    /// never arrives here: `scan_word` routes it to `try_scan_at_word` first,
    /// which always claims the braced form.
    fn word_ends_here(&self, b: u8, depth: i32) -> bool {
        match b {
            b if always_ends_bare_word(b) => true,
            b'#' => self.arith_depth > 0,
            b'+' => self.source.get(self.pos + 1) == Some(&b'='),
            b'@' => self.source.get(self.pos + 1) == Some(&b'{'),
            b'[' | b']' => depth == 0 && self.source.get(self.pos + 1) == Some(&b),
            _ => false,
        }
    }

    /// Scan word body bytes into `word`, stopping at a word break or
    /// end-of-input.
    fn scan_word_chars(&mut self, word: &mut String) {
        // Subscript-bracket depth within this word. A doubled `[[`/`]]` delimits
        // a `[[ … ]]` test/dispatch only at depth 0; inside a subscript (depth >
        // 0) a `]]` is two subscript closes — e.g. the bare nested cell read
        // `a[a[0]]` (arith model B) ends in `]]` and must stay one word.
        let mut depth = 0i32;
        while self.pos < self.source.len() {
            let b = self.source[self.pos];
            if self.word_ends_here(b, depth) {
                break;
            }
            // A continuation joins the next line into this word; without it a
            // word split across lines silently became two statements.
            if self.take_line_continuation() {
                continue;
            }
            Self::track_subscript_depth(b, &mut depth);
            self.push_source_char(word);
        }
    }

    /// A bracket that did not end the word is a subscript character: track the
    /// depth so a nested subscript's closing `]]` stays inside the word. A `]`
    /// at depth 0 is ordinary text, not an unbalanced close.
    fn track_subscript_depth(b: u8, depth: &mut i32) {
        match b {
            b'[' => *depth += 1,
            b']' if *depth > 0 => *depth -= 1,
            _ => {}
        }
    }
}

/// Decode `bytes` to `&str` for the `logos` DFA (which needs `&str`, while the
/// scanner holds bytes).
///
/// Source is always valid UTF-8 (a file goes through `read_to_string`, `-c`
/// arrives as a `String`) and the scanner only ever stands at a character
/// boundary, so the decode cannot fail. It used to fall back to the valid
/// prefix on error, which is the *silent* answer to a broken invariant: logos
/// would match against truncated input and the tail would be re-dispatched as
/// if it were a separate token. DD#116 — that is not a backstop, it is a
/// second answer to a question `read_to_string` already settled.
fn logos_input(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes)
        .expect("source is valid UTF-8 and the scanner stands at a character boundary")
}

/// Bytes that end a bare word in *every* context.
///
/// `Scanner::word_ends_here` layers the context-dependent breaks on top of
/// this set (`#` inside `$(( ))`, `+=`, `@{`, a doubled `[[`/`]]`). It is
/// shared rather than duplicated because `has_bare_spelling` asks the same
/// question from outside the scanner, and a private second copy of a word
/// rule is how a fix lands on one reader and silently misses the other.
pub(crate) fn always_ends_bare_word(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b';'
            | b'|'
            | b'&'
            | b'('
            | b')'
            | b'{'
            | b'}'
            | b'<'
            | b'>'
            | b'"'
            | b'\''
            | b'`'
            | b'$'
            | b'='
    )
}

/// Could `s` be written as a single *bare* word — no quotes at all?
///
/// This is the test behind SB5096: a single-quoted command head is allowed
/// only when quoting is the one way to say the name (DG, 2026-10-10 — "should
/// only be used when there is no unquoted way to say the command name"), so
/// `'/opt/my tool'` is legal and `'echo'` is not.
///
/// Biased deliberately towards "needs quoting". Every context-dependent break
/// `word_ends_here` can take is counted here as a break, even where this
/// particular position would not take it, because a false *accept* costs a
/// redundant pair of quotes while a false *reject* would make a real program
/// unsayable. The one exception is `#`, which DD#110 makes an ordinary word
/// byte everywhere but the start of a word.
///
/// `unit tests cross-check this against the real Scanner` — see
/// `bare_spelling_agrees_with_the_scanner`.
pub(crate) fn has_bare_spelling(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    !b.iter().enumerate().any(|(i, &c)| {
        let next = b.get(i + 1).copied();
        match c {
            c if always_ends_bare_word(c) => true,
            b'#' => i == 0,
            b'+' => next == Some(b'='),
            b'@' => next == Some(b'{'),
            b'[' | b']' => next == Some(c),
            _ => false,
        }
    })
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
        "str" => Tok::Str,
        "int" => Tok::Int,
        "const" => Tok::Const,
        "array" => Tok::Array,
        "map" => Tok::Map,
        "attest" => Tok::Attest,
        "signal" => Tok::Word("signal".to_string()),
        "END" => Tok::End,
        "EXIT" => Tok::Exit,
        "ERR" => Tok::Err,
        "return" => Tok::Word("return".to_string()),
        "break" => Tok::Word("break".to_string()),
        "continue" => Tok::Word("continue".to_string()),
        _ => Tok::Word(word),
    }
}

/// Append `ch`'s UTF-8 encoding. The `$'…'` body is accumulated as bytes so
/// that `\xC3\xA9` stays one character rather than two (see
/// `scan_ansi_c_bytes`), so a `char` has to be encoded rather than pushed.
fn push_utf8(out: &mut Vec<u8>, ch: char) {
    let mut buf = [0u8; 4];
    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
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

/// The two invariant panics in this module guard paths that the byte
/// dispatcher makes unreachable (journal 312). DG's rule for the coverage
/// campaign: an unreachable should-never-run assertion earns a unit test
/// proving it catches what it claims. These reach past the dispatcher and
/// call the scan paths directly with the input the dispatcher would never
/// hand them.
#[cfg(test)]
mod invariant_tests {
    use super::{logos_input, Scanner};

    #[test]
    #[should_panic(expected = "scanner stands on a lead byte")]
    fn push_source_char_rejects_a_continuation_byte_position() {
        let mut sc = Scanner::new("\u{e9}");
        sc.pos = 1;
        sc.push_source_char(&mut String::new());
    }

    #[test]
    #[should_panic(expected = "the byte dispatcher owns")]
    fn scan_word_rejects_a_word_break_byte() {
        // A space is a word-break byte; `skip_whitespace` means `scan_word`
        // never sees one. Reaching past that, the word body is empty.
        let mut sc = Scanner::new(" ");
        let _ = sc.scan_word(0);
    }

    #[test]
    #[should_panic(expected = "logos failed to match dispatched structural byte")]
    fn scan_via_logos_rejects_an_unmatched_byte() {
        // A backslash is never dispatched to logos; the DFA has no rule for
        // it, so the invariant fires instead of silently truncating.
        let mut sc = Scanner::new("\\");
        let _ = sc.scan_via_logos(0);
    }

    /// DD#116: `logos_input` used to fall back to the valid UTF-8 prefix,
    /// which handed logos truncated input and re-dispatched the tail as if it
    /// were a separate token. Source is valid UTF-8 by construction
    /// (`read_to_string` for a file, a `String` for `-c`), so the fallback
    /// answered a question already settled — and answered it silently. Only a
    /// direct call can reach the decode failure, because `Scanner::new` takes
    /// a `&str`.
    #[test]
    #[should_panic(expected = "source is valid UTF-8")]
    fn logos_input_rejects_invalid_utf8() {
        let _ = logos_input(&[0xC3]);
    }

    #[test]
    fn logos_input_decodes_valid_utf8() {
        assert_eq!(logos_input("if é".as_bytes()), "if é");
    }

    /// `has_bare_spelling` answers a question the Scanner also answers, from
    /// outside it. A prose claim that the two agree is the kind of thing that
    /// rots silently, so drive the real scanner: a string has a bare spelling
    /// exactly when scanning that text bare yields one token holding it whole.
    ///
    /// The predicate is deliberately biased towards "needs quoting", so the
    /// assertion is one-directional: whenever it claims a bare spelling
    /// exists, the scanner must agree. The converse is allowed to be
    /// conservative (it costs a redundant pair of quotes, never a program
    /// that cannot be written).
    #[test]
    fn bare_spelling_agrees_with_the_scanner() {
        use super::has_bare_spelling;
        let cases = [
            "echo",
            "/bin/echo",
            "/usr/bin/env",
            "ech*",
            "ec?o",
            "[e]cho",
            "a#b",
            "my tool",
            "/opt/my tool",
            "a;b",
            "a|b",
            "a=b",
            "a+=b",
            "a@{b}",
            "a[[b",
            "#lead",
            "a$b",
        ];
        for text in cases {
            if !has_bare_spelling(text) {
                continue;
            }
            let mut sc = Scanner::new(text);
            let tok = sc.next_token();
            assert!(
                sc.at_eof(),
                "{text:?} was claimed bare-spellable but did not scan as one token"
            );
            assert_eq!(
                sc.source_slice(tok.span.start, tok.span.end),
                text,
                "{text:?} was claimed bare-spellable but the token text differs"
            );
        }
    }

    /// The half that matters for SB5096: these have no unquoted spelling, so
    /// the rule must let a single-quoted head carry them.
    #[test]
    fn a_name_needing_quotes_has_no_bare_spelling() {
        use super::has_bare_spelling;
        for text in ["my tool", "/opt/my tool", "a;b", "a|b", "a$b", "", "#lead"] {
            assert!(
                !has_bare_spelling(text),
                "{text:?} must be reported as needing quotes"
            );
        }
    }
}
