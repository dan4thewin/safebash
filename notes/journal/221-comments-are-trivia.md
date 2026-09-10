# 221 — Comments are lexical trivia (trailing comments fixed)

DG: "trailing comments seem to be broken." Then, concretely: `int foo=1 # bar`
broken too.

## The breakage

`echo hi # c`, `int/const/array/map foo=… # c`, `for x in a b # c`,
`if true # c`, `echo hi | cat # c`, `return 0 # c` — all **SB4014 "expected
expression"**. Only a full-line comment (`# c` at statement start) and a couple
of constructs that end in a hard delimiter (`[[ … ]] # c`, `bless … # c`)
worked. `x=5 # c` worked by luck (its value parse stops cleanly).

Root cause: the hand-parser modeled `#` as a token that becomes a
`StmtKind::Comment` at statement *start* (`parse_comment_stmt`). Every
value-collecting loop (command args `parse_command_element`, declaration values,
for-in words, dispatch patterns) recognized only `Newline | Semi | Eof` as a
terminator, so a **trailing** `Tok::Comment` fell into the value parser and blew
up. And comments could also sit at structural positions (after a `for`/`if`
header before `{`), each its own latent break.

## The design call (DG)

I laid out two fixes:

- **A — keep comments as AST statements**: add `Tok::Comment` as a terminator to
  every value loop *and* teach every structural position to skip a trailing
  comment. Many sites, fragile, each new position a latent re-break.
- **B — comments as lexical trivia**: drop `Tok::Comment` in the Scanner, remove
  the now-dead `StmtKind::Comment` + `parse_comment_stmt`. One chokepoint, kills
  the whole class.

The fact that flipped it: **`.sash_source` embeds the source verbatim** (DD#45),
so `--extract` round-trip does not depend on the AST carrying comments at all.
Comments were already no-ops in native/c (filtered in transform, `Comment →
block` in lowering), and `--emit-source`/`--annotate` are already rejected by
the main compiler. The only loss is the non-gate janet backend rendering
comments into its output. DG chose **B**. → DD#85.

## Implementation

- `parse/lexer.rs`: new `next_token_skip_comments` loops over `next_token`,
  skipping any `Tok::Comment`; `peek`/`peek_n`/`next` call it. `next_token` (the
  cc-15 byte-dispatcher) is left byte-identical to HEAD so the complexity gate
  keeps grandfathering it — the filter is a separate tiny wrapper.
- Removed `StmtKind::Comment` (ir.rs) + its two `child_bodies` arms, the
  `Tok::Comment` dispatch arm + `parse_comment_stmt` (parser.rs), and the
  consumers: two `.filter(Comment)` calls (transform/expr_handlers.rs, now
  no-ops), the `Comment => block` lower arm, two `validate.rs` arms, the
  `emit_janet` arm + dead `emit_comment`, and two `emit_janet_map` arms.
  `dispatch_statement` cc 23→22 (pure dispatcher, delta −21, gate-exempt).

## Behavior (matches bash)

- `echo a # ; echo b` → runs only `echo a` (the `;` is inside the comment).
- Same-line `f() { return 0 # c }` → the `# c }` comment eats the `}`; bash
  needs the `}` on its own line too. Verified against bash.
- `#!/usr/bin/env sash` shebang is a comment → dropped (janet already skipped it).
- `#` inside `$(( ))` stays a Word (DD#72) — arithmetic untouched.

## Verified

spec `spec/comments.sh` cmt.1–11 (trailing comment after command / pipeline /
typed decl / array decl / for-header / if-cond; full-line; shebang; comment runs
to EOL; comment between statements; inline comment in a `$( )` body). cmt.11
gated native,c (janet doesn't inline a `$(user_fn)` capture — pre-existing,
unrelated). cov.90's stale `parse_comment_stmt` reference refreshed.

Full 3-mode sweep vs HEAD `da02f3a`: **0 regressions, 0 new failing, +32
passing** (cmt.1–11 × 3 − 1 janet skip). Lib unit tests green (16;
`comment_to_eol` tests the logos layer, unaffected). Rust cargo suites (benched)
have comment-bearing tests (`brush_cmdsub_comment`, oracle `comment_preserved`)
that still pass — they assert output, and output is unchanged.
