---
title: "DECISION: bare `{` in a regex — literal (bash-compatible) or require `\\{`?"
status: open
priority: P2
kind: decision
depends: []
gates:
  - "DG rules: does a bare `{` that does not open a valid `{n,m}` interval match a literal `{` (bash/grep ERE behavior), or must a literal brace be written `\\{` (the vendored engine's current behavior)?"
---

# Bare `{` in a regex: literal vs. require `\{`

## The conflict

The vendored Plan 9 regex engine (`sash-rt/csrc/regcomp.c`) **rejects** a
bare `{` that does not open a valid `{n,m}` interval, with a deliberate
diagnostic:

```
regexp error: invalid '{' — use \{ for literal or {n,m} for repetition
```

So today a literal brace must be written `\{`:

```
match "a{b" "a\{b"          # → matches
gsub "x{y{z" "\{" "_"       # → x_y_z
echo "${var:s/a\{b/z/}"     # var=a{b → z
```

Bash / POSIX ERE (and `grep -E`) instead treat a `{` that is not part of
a valid interval as a **literal** `{`:

```
$ printf 'a{b\n' | grep -E 'a{b'
a{b                          # matches
```

`}` is already literal in both (only `{` starts an interval), so
`${var:s/a}b/z/}` → `z` works today (spec 63.g1 / 63.g2).

## Why this surfaced

Implementing `regex-interpolation-delimiters` (DD#63 Path B). The Path B
lexer correctly keeps a `{`/`}` byte inside the `${…:s/…}` fields from
closing the expansion (verified: 63.g1/g2/g3 all lex correctly). The
open question is purely the **regex semantics** of a bare `{`, which is
independent of Path B. Spec 63.g3 was therefore written to the engine's
current `\{` convention (guarded native,c) and this decision filed.

## Options

1. **Keep `\{`-required (current).** Explicit, unambiguous, already
   implemented and documented by the engine's own error message. A user
   who wants a literal brace writes `\{`. Divergs from bash.
2. **Bare `{` literal (bash-compatible).** Change `regcomp.c` so a `{`
   not opening a valid `{n,m}` is a literal byte (like `}`). Matches
   bash/grep. Requires an engine change and re-validation of the
   `{n,m}` interval path (DD#39 counted repetition) so a real interval
   still parses.

## Notes / scope if option 2 is chosen

- Engine: `regcomp.c` `bldshortclass`/`lex` `{`-handling — fall back to
  a literal RUNE when the `{…}` does not parse as `{n[,m]}`.
- Re-check DD#39 `{n,m}` specs (39.6, E4/`^a{2,3}$`) still pass.
- `regex_validate.rs::validate_regex` must be updated in lockstep (it
  currently rejects a bare `{`, matching the engine); otherwise a
  `=~ s/a{b/…/` static pattern would compile-reject while `${var:s/a{b/…}`
  runtime-errors — the same validate-vs-engine skew noted below.

## Related latent skew (either way)

`validate_regex` (compile-time, DD#39) rejects a bare `{`, and it runs
only on the `=~` mutation form and `[[ =~ ]]`, NOT on `${var:s/…}` or
direct `$(sub/gsub/match …)`. So a bad-brace static pattern compile-rejects
via `=~` but runtime-errors via `${var:s}`/`$(sub)`. Whichever way this
decision goes, `validate_regex` and the engine should agree. Tracked
alongside this decision.
