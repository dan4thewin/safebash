---
title: "janet backend: cooked read (readc de-escaping)"
status: ready
priority: P3
kind: backend-parity
depends: []
gates:
  - "janet readc de-escapes like native/c (escaped-whitespace kept in field, backslash removed); wordsplit 22.20–22.24 pass in janet mode"
---

# janet: cooked `readc`

`readc` (cooked read — see `notes/journal/225`) is implemented native + c.
janet **recognizes** `readc` (it resolves and its `while readc` loop is
seen) but its codegen treats it as **raw** `read`: no de-escaping, and
`\`-escaped whitespace still splits.

The readc specs (`spec/wordsplit.sh` 22.20–22.24) are therefore gated
`in_modes native,c` and `skip_chk`-ped in janet.

## What's needed

janet's read/split emit must, when the command is `readc`:

- split fields with cooked whitespace semantics (a `\`-escaped space does
  not terminate the field), and
- de-escape each field (single-var: the whole line; multi-var: each field
  and the remainder).

The native/c primitives are `sash_split_first_word_cooked` and
`sash_deescape` (`sash-rt/src/lib.rs`); janet has its own string ops, so
this is a janet-side reimplementation of the same two behaviors (janet is
`#![no_std]`-free, pure Janet — a small `de-escape`/`cooked-split` in the
emitted prelude or inline).

## Where

`sash/src/emit_janet.rs` — the read emit and the `while read` recognizer
(`match_while_read`, now matches `read`/`readc`). The `cooked` bit is on
`SsaInst::ReadLine`/`ReadLineMulti`; thread it into the janet read emit
the way native/c do.

## Done when

Un-gate 22.20–22.24 to `in_modes native,c,janet` (drop the `skip_chk`
else-branch) and they pass in all three modes; 0 regressions.
