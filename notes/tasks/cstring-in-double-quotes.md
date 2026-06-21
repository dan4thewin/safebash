---
title: "Recognize $'...' inside \"...\" (DD#64 addendum: C-string as in-string escape hatch)"
status: ready
priority: P3
kind: feature
depends: [DD64-no-shell-backslash]
gates:
  - "\"a$'\\n'b\" yields a<newline>b (C-string expands inside double quotes)"
  - "every $-form expands in \"...\": $var, ${var}, $(...), $((...)), AND $'...'"
  - "escape-aware close: \"a$'x\\'y'b\" keeps the C-string open past the escaped quote"
  - "structural chars via C-string inside \"...\": $'\\x22' -> literal \", $'\\x24' -> $, $'\\'' -> ', $'\\\\' -> backslash"
  - "$' claims its sequence inside \"...\"; a lone ' (not preceded by $) stays a literal apostrophe (\"it's\" -> it's)"
  - "C-string content is a compile-time constant -> always clean (no taint)"
  - "spec tests (wordsplit/types); native + c + janet; no net regression"
---

# Recognize `$'…'` inside `"…"` (DD#64 addendum)

## What

A `$'…'` C-string is recognized **inside double quotes** as a
compile-time-constant expansion, alongside `$var`/`${var}`/`$(…)`/`$((…))`.
So inside `"…"` every `$`-prefixed form expands — `$'…'` is no longer the
lone exception. `"a$'\n'b"` → `a`⏎`b`.

This is the in-string escape hatch DD#64 needs: DD#64 made `\` verbatim in
`"…"`, leaving no in-place way to embed a control byte; this restores it
via the one sanctioned escape syntax. Because a C-string is escape-aware
and encodes every structural char, `"…"` + `$'…'` give complete
literal-byte control inside an interpolated string with no concatenation
(even a literal `"` via `$'\x22'`). Deliberate divergence from bash (which
leaves `"$'\n'"` literal).

## Where

- `parse/lexer.rs::scan_double_quoted` (~700): it already dispatches on
  `$` for `${` / `$(` / `$((` / `$name`. Add the `$'` case → hand off to
  the existing **escape-aware** `scan_ansi_c_quoted` (~946), append its
  bytes as a literal part of the double-quoted concat.
- `$'` claims its sequence inside `"…"` (like `${`/`$(`). A lone `'` not
  preceded by `$` stays literal. Literal `$'` text is encoded via the
  C-string itself (`"$'$\''"` → `$'`).

## Notes / relations

- DD#64 addendum (2026-06-21) is the authority; see that DD.
- Resolves the DD#63/DD#65 "word context vs double-quoted" question: a
  `$'…'` is recognized wherever a `$`-form is, so it works uniformly in
  `"…"`, in `${var:-default}`, and in the `sub`/`gsub` replacement — they
  are one interpolating context (replacement adds only the `\0`–`\9`
  backref pass).
- Verified prerequisites: `scan_ansi_c_quoted` is already escape-aware
  (`$'a\'b'` → `a'b`) and encodes structural chars (`$'\x22'`→`"`, etc.).

## Discovery

2026-06-21 DD walkthrough with DG. Current sash matches bash: `"a$'\n'b"`
is literal `a$'\n'b` (every other `$`-form already expands in `"…"`).
