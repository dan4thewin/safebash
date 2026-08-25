---
title: "Converter: $@ / $* → @args / $(join args); warn on quoted splat \"$@\""
status: ready
priority: P3
kind: converter
depends: []
gates:
  - "sash-convert rewrites in-function `$@` → `@args` (splat) where the context takes a splat (for-in source, command arg list)"
  - "in-function `\"$@\"` (quoted, join intent) → `$(join args)`; `$*` → `$(join args)`"
  - "when it cannot determine intent, emit a review WARNING pointing at @args vs $(join args) (a bare `\"@args\"` literal is NOT a valid target — see below)"
  - "no regression in sash-convert tests"
---

## Context

DD#75/DD#81 dropped `$@` from the sash language: in-function `$@`/`${@}` is
rejected (SB5045). The splat is `@args`; a joined string is `$(join args)`. `$*`
was already removed → `$(join args)`. See
`notes/decisions/DD81-function-call-discipline.md` and DD#75 §5 (amended
2026-08-25).

The **converter** (bash → sash-native, `sash-convert/`) must translate the bash
forms so converted scripts compile.

## Rules

- **`$@` / `"$@"` as a for-in source or command-argument list** (splat intent) →
  **`@args`** (the splat sigil).
- **`"$@"` where a single joined string is wanted** (echo/assignment/test) →
  **`$(join args)`**.
- **`$*` / `"$*"`** → **`$(join args)`** (implicit-join is always explicit now).
- `$1..$9`, `$#` are unchanged (`$#` → `${#args}` per DD#75 sugar, still valid).

## The quoted-splat warning (DG 2026-08-25)

A bash `"$@"` is context-sensitive (word-splits per element when unquoted, one
word per element when quoted). sash keeps **`"@foo"` as a plain literal** (no
false positives on legitimate `@` literals like `user@host`), so the converter
must **not** emit `"@args"` — that would silently become the literal string
`@args`. When the converter cannot classify a `"$@"` occurrence as splat-vs-join
with confidence, emit a **review WARNING** (like the converter's other
best-effort comments) directing the author to choose `@args` (splat) or
`$(join args)` (joined), rather than guessing.

## Provenance

DG, 2026-08-25 (the `$@`-drop thread). The sash-side reject (SB5045) shipped in
the DD#81 `$@`-drop commit; this is the converter half.
