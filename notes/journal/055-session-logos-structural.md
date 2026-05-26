title: Session 2026-05-27 — Logos lands the structural-operator subset
date: 2026-05-27
branch: logos-lexer
---

# Logos for structural tokens (DD#59)

DG: "Make a new branch and switch to use logos — if it goes well,
then cement it into a DD." It went well.

## What landed

`sash/src/parse/logos_tokens.rs` is now a regular module wired into
`Scanner::next_token`. Logos owns the structural-operator family
that has no contextual ambiguity:

- newline, `#`-comment
- semicolon family: `;`, `;;`, `;&`, `;;&`
- pipe family: `|`, `||`
- ampersand family: `&`, `&&`, `&>`
- parens: `(`, `((`, `)`, `))`
- braces: `{`, `}`
- gt family: `>`, `>>`, `>&`

Hand-roll keeps everything that needs state or context:

- `$…`, `'…'`, `"…"`, `` `…` ``, ANSI-C `$'…'`
- `<` family including the heredoc sentinel emission
- `-` standalone vs `-flag` prefix
- `!` / `!=`, `=` / `==` / `=~`, `+=`
- `[` / `[[`, `]` / `]]`
- `<<<` (in `scan_lt` alongside `<<` heredoc handling)
- `scan_word` and its keyword-promotion table

`next_token` dispatches by first byte: hand-rolled methods own
their bytes, the structural bytes go to a tiny `scan_via_logos`
helper that runs a `LogosTok::lexer` against the remaining slice
and turns the first matched token back into a `Tok` via a
`to_tok` shim.

## Numbers

- `Scanner::next_token` cyclomatic complexity: **78 → 15**.
- Net diff in `lexer.rs`: −415 / +228 (one file).
- New `logos_tokens.rs`: 187 lines, mostly declarative.
- Spec gates: **285/285** (no change).
- Cargo lib unit tests: **12/12** (including 5 new
  `logos_equivalence` tests that pin longest-match correctness).
- Clippy `-D warnings`: clean across `sash`, `sash-convert`,
  `sash-expand`.

The top-10 cyclo list dropped `next_token` entirely. New top is
still `tokenize_arith` at CC 88, which is the next target in
`r47-reduce-parser-complexity-hotspots`.

## What did not change

`scan_word`'s keyword-promotion lookup is untouched. The keyword
variants in `LogosTok` (If, Then, Else, Fn, Function, …) exist
purely as guard rails — the `logos_equivalence` tests pin
longest-match on them so we can revisit later if we want, but
they are not on a current execution path.

## Caveats

- UTF-8: `scan_via_logos` decodes the byte slice into a `&str`
  with `from_utf8`, falling back to the valid prefix if a
  multi-byte sequence is malformed. The structural-byte dispatch
  guarantees the first byte is ASCII; only `#…` comments can
  carry UTF-8, and the comment payload is re-built from the
  original `&[u8]` source. No payload silently goes through
  lossy decoding.
- Per-token `LogosTok::lexer(slice)` is an allocation-free DFA
  init but it's still a method call per token. Profiling deferred
  until/unless it shows up.

## Process notes for future-me

- The prototype-first approach (build the enum + equivalence
  tests on its own, then integrate) was worth the extra step.
  When integration broke (the `&str` vs `&[u8]` source-type
  mismatch), the prototype's tests were already passing, so the
  bug was clearly mine, not logos's.
- `cargo clippy --release --quiet -- -D warnings` (now in the
  pre-commit hook) caught the temporary `#![allow(dead_code)]`
  the prototype needed before it was wired in. Stripping that
  allow at integration time forces the wiring story to be
  honest.
- Heredoc handling stays where bad ideas go to die — in a
  hand-rolled helper that the validator can rely on. Logos
  literally cannot help here, and pretending it can would have
  bitten us within a sprint.

## Next

- `r47-reduce-parser-complexity-hotspots` rolls on to
  `tokenize_arith` (CC 88), `scan_double_quoted` (CC 68),
  `intern_literals` (CC 70), `cli_main` (CC 69).
- Once `logos-lexer` is reviewed, fast-forward `hand-parser`
  to it.
