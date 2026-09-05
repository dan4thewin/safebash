---
title: "janet: builtin echo lacks -n flag handling and :~/:! elide"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "janet: `echo -n X` suppresses the trailing newline and does NOT print a literal `-n`"
  - "janet: `echo a ${x:~} b` with empty x prints `a b` (the elide arg contributes nothing — no value, no separator), matching native/c"
---

## Found

2026-09-04, fixing the builtin-echo elide ICE (journal 244). native + c now
handle a `:~`/`:!` elide arg in builtin `echo` (write the separator + value only
when non-empty). janet's `emit_janet_echo` does neither:

- **`-n` is not parsed** — `echo -n X` emits `(print (string "-n" " " X))`, so it
  prints a literal `-n ` prefix and a trailing newline.
- **elide is not honored** — `echo a ${x:~} b` emits
  `(print (string a " " x " " b))`, so an empty `x` yields `a  b` (double space)
  instead of `a b`.

Specs `wordsplit.sh` 22.29–22.31 (and the multi-arg echo-elide) are gated
`native,c` for this reason.

## Fix sketch

`emit_janet_echo` (`emit_janet.rs`):
1. Parse a leading `-n` like native/c (`echo_parse_flags`), and use `prin`
   (no newline) vs `print`.
2. For an arg that is a `:~`/`:!` elide expansion, emit
   `(if (empty? V) nil V)` and build the line with
   `(string/join (filter (comp not nil?) @[…]) " ")` so an elided arg drops out
   (a genuine empty-string non-elide arg stays, since it is `""` not `nil`).

## Context

Companion to the native/c builtin-echo-elide fix (journal 244) and the
janet-elide family (`janet-positional-default-present`, etc.). janet is the
non-gate backend.
