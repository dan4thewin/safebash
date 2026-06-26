# 123 — Fix: `<<` / `<<=` left-shift mis-lexed as heredoc

Continues the arithmetic-correctness thread (journals 121, 122). Closes
`bug-arith-left-shift-heredoc`, the last *lexing* gap in `(( ))`.

## Bug

`(( y = x << 2 ))` and `$(( x << 2 ))` and `(( x <<= 2 ))` all gave the
wrong result (the `<< n` was dropped). `>>` worked fine.

## Diagnosis (confirmed by instrumenting `tokenize_arith`)

The arith string reaching the byte-lexer was `x __HEREDOC__ 2` — the shell
scanner's `scan_lt` (lexer.rs) tokenizes `<<` as a **heredoc** sentinel
(`__HEREDOC__`) and skips to end of the delimiter word, with no arith
context. So `collect_arith_content` reassembled the mangled text. `>>`
survived because it scans to `Tok::DblGt` (">>"), which the arith re-lexer
recombines into Shr.

The arith side was already complete: `match_arith_op2(b"<<")` → `LtLt`,
`match_arith_op3(b"<<=")` → `LtLtEq`, both mapped to `Shl`. The defect was
purely the shell scanner mangling `<<` before arith saw it.

## Fix

- `Scanner` gains `arith_depth: usize` + `enter_arith()` / `exit_arith()`.
- `collect_arith_content` brackets its pull loop with
  `enter_arith()`/`exit_arith()` (the single chokepoint for all three
  arith entry points — `(( ))`, `$(( ))`, C-style for).
- `scan_lt`: when `arith_depth > 0`, emit a verbatim `Word("<<")`
  (advancing 2 bytes, no heredoc skip) instead of the sentinel.
  `tok_to_text(Word(w)) = w`, so the reassembled string contains `<<`,
  which `tokenize_arith` re-lexes to `LtLt` → Shl. `<<=` works for free:
  `scan_lt` emits `Word("<<")` then `Tok::Eq`, and the
  `needs_separator_space` `<op>=` rule (journal 121) keeps them adjacent →
  `<<=` → `LtLtEq` → Shl compound-assign.
- `<<<` (here-string) is checked first, unaffected. Heredoc rejection
  outside arith is unchanged.

No peek-buffer race in practice: `collect_arith_content` is the caller's
first action after `((`/`$((` is consumed, so `<<` is tokenized in arith
mode. Verified `$(( ))` (not just `(( ))`) works.

## Spec / verification

`spec/arithmetic.sh` 31.1j (`<<`), 31.1k (`$(( << ))`), 31.1l (`<<=`),
31.1m (`>>=` regression guard). All three backends green. Full suite:
native **0 failing** (535 passing); c=58/janet=106 unchanged — zero
regressions. Heredoc rejection, `>>`, `<=`/`>=`, redirects all verified
intact.

## DD#25 status

With `<<` now evaluating, the only remaining DD#25 item is a **decision**:
should left-shift *overflow* (`1 << 63` into the sign bit, or shift ≥ 64)
trap? Shift-overflow semantics are murkier than `+`/`-`/`*`; left for DG
(noted in `bug-runtime-arith-overflow-wraps`). Runtime `+ - * Pow Div Neg`
all trap; `Mod` is correct.
