---
title: "`(( ))` internals: multi-line separator semantics + comments inside arithmetic"
status: deferred
priority: P3
kind: decision
depends: []
gates:
  - "DECIDE: is a multi-line `(( ))` one expression (bash model) or a sequence block (newline ≡ comma)?"
  - "DECIDE: are `;` and `,` equivalent sequence separators in a standalone `(( ))`?"
  - "DECIDE: allow whitespace-preceded `#…EOL` comments inside `(( ))` / `$(( ))` (super-bash), or keep the bash-parity rejection?"
  - "If any 'allow': implement in the arith re-lexer + comma loop (and/or arith_hash_word), add specs, note the divergence in DD#72 / a new DD"
---

# `(( ))` internals — separators and comments (needs a DG decision)

Surfaced by DG (2026-08-31) while reviewing DD#85 (comments-are-trivia). Two
linked questions about what is allowed *inside* `(( ))` / `$(( ))`. Both are
**deliberate-divergence-from-bash** design calls, not bugs — sash currently
matches bash on all of them. Deferred for a decision later.

## Background — current behavior (all verified, all match bash)

| inside `(( ))` | standalone | C-style for-header |
|---|---|---|
| `,` (comma operator) | works — sequence, value = last (`(( a=1, b=2 ))` → a=1 b=2) | sequences within a slot |
| `;` | **SB4035** (rejected; confusing "invalid numeric literal ';'") | slot delimiter (structural) |
| newline after a *complete* sub-expr (`(( a=1 ⏎ b=2 ))`) | **SB4020** "unexpected token after arithmetic expression" | — |
| newline after a *pending* operator (`(( 1 + ⏎ 2 ))`) | works → 3 (continuation) | — |
| `#…EOL` (comment) | **SB4035** (DD#72: `#` lexes as a Word so `base#digits` is a loud reject, not a silent truncation) | same |

bash rejects `;`, newline-as-separator, and `#`-comments inside `(( ))` too, so
today's sash is bash-consistent. The questions below are whether sash should be
**more permissive** than bash for readability, consistent with its "safe subset
that reads clearly" ethos and the DD#85 "comments/trivia are uniform" direction.

Key architectural facts:
- The C-style for-header `;` is **not** an arith token: `parse_c_style_for`
  (`parse/parser.rs:496`) splits the raw header on `;` into exactly three slots
  *before* the arith parser runs; within each slot `,` sequences. So `;` already
  has a **grouping** role there that `,` does not.
- A `(( ))` body is collected as **raw text** (`collect_arith_content_raw`),
  then re-tokenized by a **second** lexer `tokenize_arith` and parsed by
  `ArithParser`; the top-level comma fold is at `parse/arith.rs:214`
  (`ArithExpr::Comma`). The main scanner's `arith_hash_word`
  (`parse/lexer.rs:383`) only exists so the raw *collector* doesn't mis-detect
  the closing `))` when a `#` appears (without it, `16#ff ))` would look like a
  comment eating `))`). DD#72 removed `base#digits`, so `#` now has **no valid
  meaning** anywhere in sash arithmetic — it is always an error today.

## Decision 1 — multi-line `(( ))`: one expression vs sequence block

- **(1a) Keep single-expression (bash model).** A newline is only valid
  mid-expression (continuation). Optionally just improve the two confusing error
  messages (`;`/newline → SB4035/SB4020 both read as numeric-literal noise).
  Zero semantic change.
- **(1b) Sequence block: newline ≡ comma.** `(( a=1 ⏎ b=2 ⏎ c=b*3 ))` becomes a
  little let-block (value = last), instead of forcing `(( a=1, b=2, c=b*3 ))` on
  one line.
  - **Required rule:** a newline is a *separator* only when a **complete**
    sub-expression has just been parsed, and a *continuation* when a binary
    operator is still pending — otherwise `(( 1 + ⏎ 2 ))` (works today = 3)
    breaks. This is standard significant-newline handling (Go / JS ASI).

Recommendation: **(1b)** — it's the readability win, and it fits the "newline
separates" intuition used everywhere else in sash.

## Decision 2 — is `;` ≡ `,` in a standalone `(( ))`?

- **(2a) No** — `;` stays rejected in standalone `(( ))` (bash parity). newline
  and `,` are the only sequencers (if 1b).
- **(2b) Yes** — `;` is a sequence separator equal to `,`/newline in a
  standalone `(( ))`. Removes the confusing SB4035-on-`;`.
  - **Nuance:** `;` keeps its distinct **grouping** role in the for-header (the
    for-parser splits on `;` into 3 slots first). So the honest rule is: inside
    `(( ))`, `,` / `;` / newline all sequence, **except** the for-header reserves
    `;` to delimit its three slots (with `,`/newline sequencing within each).
    `;` and `,` are thus interchangeable only in a standalone `(( ))`.

Recommendation: lean **(2b)** for a unified separator story, but (2a) is fine if
the dual role of `;` feels too subtle.

## Decision 3 — comments inside `(( ))` / `$(( ))`

Because DD#72 freed `#` (no valid arithmetic use), sash *could* allow comments
inside arithmetic — making DD#85 "comments are trivia everywhere" truly
uniform (arith is the lone exception today).

- **(3a) Keep rejecting** (bash parity). `#` anywhere in arith → error. Simplest.
- **(3b) Allow whitespace-preceded `#…EOL` as a comment** inside `(( ))`.
  - **Caveat / required rule:** treat `#` as a base-literal marker (→ Word →
    SB4035) **only when immediately preceded by an alnum/digit** (`16#ff` stays
    a loud reject — do NOT reintroduce the DD#72 silent-truncation); otherwise
    (whitespace/newline before it) it's a comment. Ties naturally to 1b: an
    end-of-line comment on a spanned arith line only makes sense if newline is a
    separator.

Recommendation: **(3b)** *iff* 1b is chosen (comments at the ends of spanned
lines are the motivating case); otherwise (3a).

## Implementation pointers (once decided)

- Separators (1/2): `tokenize_arith` (emit a separator token for newline and/or
  `;`) + the top-level comma fold at `parse/arith.rs:214`
  (generalize `ArithExpr::Comma` to accept newline/`;`); preserve the
  pending-operator continuation rule. Leave `parse_c_style_for`
  (`parse/parser.rs:496`) splitting on `;` first, so the for-header is unaffected.
- Comments (3): `arith_hash_word` (`parse/lexer.rs:383`) — gate the Word branch
  on "preceded by alnum"; let a non-adjacent `#` fall through so the raw
  collector / re-lexer drops it (mirrors `next_token_skip_comments`).
- Specs: extend an arith spec (e.g. `spec/arithmetic.sh`) with multi-line
  sequence, `;`-as-comma, and (if 3b) end-of-line arith comments. Record the
  bash divergence in DD#72 (addendum) or a new DD.

## Also fix regardless (low-stakes)

Even under (1a)/(2a)/(3a), the error messages for `;`/newline/`#` inside `(( ))`
are misleading — they surface as "invalid numeric literal" (SB4035) or
"unexpected token" (SB4020). A dedicated diagnostic ("`;` / comments are not
allowed inside `(( ))` — use `,`" etc.) would help. Optional, decide alongside.
