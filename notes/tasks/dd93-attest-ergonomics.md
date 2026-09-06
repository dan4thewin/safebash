status: in-progress
depends: DD93
gates:

# DD#93 attest ergonomics — live tracker

Bundle from the 2026-09-06 bless-ergonomics thread. See
`notes/decisions/DD93-attest-ergonomics.md`.

## Stages

- [x] **S1 rename** `bless` → `attest` (hard, no alias). Lexer keyword,
  identifiers/strings, diagnostics, converter emit, specs, Rust tests,
  current-state docs. 0 regressions (native/c/janet). Journal 247.
- [ ] **S2 shape tests** `i?`/`p?`/`g?`. Runtime `is_print`/`is_graph`;
  `-i`→`i?` rename; parser `?`-suffix recognizer; native/c/janet dispatch;
  migrate `types.sh` 77.x + add p?/g? specs. General operators.
- [ ] **S3 unary shorthand** `attest VAR OP` (scalar + attest-all).
- [ ] **S4 arith-assign form** `attest numstr (( myint = numstr ))`. Both
  clean, distinct names, scalar only, SB4030 exemption.

## Follow-ups (not this bundle)

- UTF-8-aware shape test (distinct spelling).
- Remaining POSIX-class shape tests (`a?` alnum, `d?` digit, `l?` alpha,
  `s?` space, …) — token/dispatch designed to make each a one-liner.
- Array analogue of the arith-assign form: `attest e in a (( … ))`.
- Same-name in-place retype for the arith form (`attest s (( s = s ))`).
