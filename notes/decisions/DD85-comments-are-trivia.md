title: DD#85 — Comments are lexical trivia (dropped by the scanner)
status: accepted

1. Recognized comments are scanner trivia, absent from the AST and ignored
   uniformly in every grammatical position. They run through end-of-line,
   including any apparent delimiters on that line.
2. Comment recognition follows DD110's boundary rule and DD94's arithmetic
   rules; comment removal must not silently truncate a word or expression.
3. Source extraction preserves comments through the verbatim embedded source,
   independently of the AST.

---

Settled with DG (2026-08-31). A `#`-comment is **lexical trivia**: the Scanner
drops `# … EOL` tokens entirely, so a comment never reaches the parser. Comments
have no representation in the AST.

## The thread that produced this

DG: "trailing comments seem to be broken." They were — `echo hi # c`,
`int foo=1 # c`, `for x in a b # c`, `if true # c`, `echo hi | cat # c`,
`return 0 # c` all failed with **SB4014 "expected expression"**. Only a
**full-line** comment worked.

Root cause: the hand-parser treated `#` as a token that becomes a
`StmtKind::Comment` statement, dispatched at statement *start*
(`parse_comment_stmt`). But the value-collecting loops (command args,
declaration values, for-in words, dispatch patterns, …) only recognized
`Newline | Semi | Eof` as terminators — a trailing `Tok::Comment` fell through
to the value parser (`parse_expr`) → "expected expression". Comments could also
appear at structural positions (after a `for`/`if`/`while` header before `{`),
each a separate latent break.

## The decision

Two ways to fix it were on the table:

- **A — keep comments as AST statements.** Add `Tok::Comment` as a terminator to
  every value loop *and* make every structural position tolerate a trailing
  comment. Fragile (many sites; each new syntactic position is a latent
  re-break); preserves janet's comment rendering.
- **B — treat comments as lexical trivia (chosen).** Drop `Tok::Comment` in the
  Scanner (`peek`/`peek_n`/`next` skip them via `next_token_skip_comments`), and
  remove the now-dead `StmtKind::Comment` + `parse_comment_stmt`. One chokepoint;
  permanently eliminates the whole bug class; matches how most compilers handle
  comments.

DG chose **B**.

## Why B is safe

- **Round-trip is unaffected.** `.sash_source` embeds the source **verbatim**
  (DD#45; the raw bytes), so `--extract` returns the original text, comments and
  all — independent of the AST.
- **Comments were already semantic no-ops** in the gate backends: the transform
  filtered them out (`$( )` bodies) and `lower_one` mapped `Comment → block`
  (nothing). Native/c never emitted anything for a comment.
- **`--emit-source` / `--annotate` are already rejected** by the main compiler
  ("use sash-convert for source transformations"), so no compiler mode
  reconstructs source from the AST.
- The only visible change: the **janet** backend (non-gate) stops rendering
  comments into its output — accepted by DG.
- A shebang (`#!/usr/bin/env sash`) is a comment, so it too is dropped
  (previously the janet emitter special-cased and skipped it anyway).

## Consequences

- ~~`#` is still not a comment inside `$(( ))` (DD#72 — it lexes as a Word via
  `arith_hash_word`), so arithmetic is untouched.~~ DD94 admits properly
  separated arithmetic comments; DD110 governs the boundary.
- A comment still runs to end-of-line: `echo a # ; echo b` runs only `echo a`
  (the `;` is inside the comment), and a same-line `# c }` eats the `}` — both
  match bash.
- `Tok::Comment` (the lexer token) still exists; it is produced by `next_token`
  and filtered by `next_token_skip_comments`. `LogosTok::Comment` is unchanged.

## Implementation

`parse/lexer.rs` (`next_token_skip_comments` wrapping `next_token`; the three
`Scanner` accessors call it), `parse/parser.rs` (removed the `Tok::Comment`
dispatch arm + `parse_comment_stmt`), `ir.rs` (removed `StmtKind::Comment` +
its `child_bodies` arms), and the consumers in `validate.rs`,
`transform/{statements,expr_handlers}.rs`, `emit_janet*.rs`. spec
`spec/comments.sh` (cmt.1–11), all three modes. Journal 221.
