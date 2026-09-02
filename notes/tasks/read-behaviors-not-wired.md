---
title: "read: behaviors not wired in (survey — DG to decide which matter)"
status: ready
priority: P2
kind: decision
depends: []
gates:
  - "DG decides which unwired read behaviors to add: notably (1) accept `-r` as a no-op (bash scripts use it ubiquitously; sash is always raw), and (2) NUL-terminator trailing-empty (drop a trailing empty from a trailing `-d` delimiter, matching bash mapfile -d '' / find -print0)"
---

# `read` — behaviors not wired in

Surfaced while unifying the read-flag parsing (journal 232). sash `read`
recognizes exactly three flags — `-a ARR`, `-d DELIM`, `-f SEP` — through one
shared classifier (`ir::classify_read_flags`). Everything else is either
rejected (SB5015) or unimplemented. This surveys the gaps so DG can pick which
matter.

## Flags sash rejects that bash has

| flag | bash meaning | sash | notes / recommendation |
|------|--------------|------|------------------------|
| `-r` | raw (no `\` processing) | **rejected** (SB5015) | **sash is ALWAYS raw**, so `-r` is a semantic no-op — yet a hand-written `read -r x` is an error. bash scripts use `read -r` *ubiquitously*. The converter strips it, so converted code is fine, but hand-written sash pays the tax. **Recommend: accept `-r` as an ignored no-op** (the raw behavior it requests is already the default). Keeping the rejection is defensible only as a "raw-is-default, don't write `-r`" nudge. |
| `-n N` / `-N N` | read exactly N chars | unimplemented | A real (non-interactive) behavior — fixed-width field reads. Not wired. Add only if a use case appears. |
| `-u FD` | read from fd N | unimplemented | sash uses redirects (`read x < file`); an arbitrary fd number isn't expressible. Probably fine to leave. |
| `-p` / `-s` / `-t` / `-e` / `-i` | prompt / silent / timeout / readline / initial-text | unimplemented | All **interactive** — irrelevant to sash's batch/compiled model. Fine to reject. |
| `-z` | (not a bash `read` flag) | rejected | NUL reading is `-d ''` in sash (bash convention), so no `-z` alias is needed — see below. |

## Semantics not wired

- **NUL-terminator trailing-empty.** `read -a arr -d ''` now splits on NUL
  (the sash-getopt idiom), but treats the delimiter as a **separator**, so
  NUL-*terminated* input yields a trailing empty element:

  ```
  sash-getopt -o ab: -- -a -b val 'has space' | read -a opts -d ''
  # opts = [-a] [-b] [val] [--] [has space] []   ← trailing [] from getopt's final NUL
  ```

  bash's `mapfile -d ''` (and the `find -print0` / `xargs -0` convention)
  treat a trailing delimiter as a **terminator** and drop the trailing empty.
  **Recommend: when `-d` is set, a trailing delimiter should not produce a
  trailing empty element** (terminator semantics) — this is the clean getopt
  idiom. (Question: apply only to `-d`, or also to the default whitespace
  split?) The fix is in `sash_split_into_array` (drop a final empty segment
  when the input ends with the delimiter).

- **`-f SEP` is a sash invention**, not a bash `read` flag (bash uses `IFS`,
  which sash doesn't have). Noted for completeness; no action.

## What IS wired (for reference)

- `read x` / `read a b` (scalar, whitespace-split remainder).
- `read -a arr` (array; whitespace split by default).
- `read -d DELIM` / `read -d ''` (record delimiter; empty = NUL).
- `read -f SEP` (sash field separator; requires ≥2 targets, SB5016).
- Unknown flags → SB5015, now **consistently** in both the standalone and
  lastpipe paths (the two parsers were unified — journal 232).
- The end-to-end getopt idiom works: `sash-getopt … | read -a arr -d ''`
  (and the `< file` form), splitting NUL-separated fields, space-bearing
  fields preserved.

## Recommendation summary

1. **`-r` → accepted no-op** (highest value; removes a ubiquitous friction).
2. **NUL-terminator trailing-empty** → drop it for `-d` (clean getopt idiom).
3. The rest (`-n`/`-N`/`-u`/interactive) → leave unless a use case appears.
