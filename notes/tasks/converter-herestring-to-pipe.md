---
title: "converter: rewrite bash here-strings `<<<` to a pipe (DD#83)"
status: ready
priority: P3
kind: converter
depends: []
gates:
  - "a converted bash `cmd <<< WORD` no longer emits `<<<` (sash rejects it, SB4050) — it becomes `producer | cmd` or a MANUAL comment"
  - "the common `read v <<< \"$x\"` and `cmd <<< \"$(...)\"` forms convert to a runnable pipe where safe"
---

## Why

DD#83 removed here-strings from sash — `<<<` is rejected at parse (SB4050).
`sash-convert` must therefore rewrite bash `<<<` on conversion, or a converted
script that used a here-string fails to compile. `<<<` is exact sugar over a
pipe in sash (no subshell, provenance-based taint, no word-splitting), so a
mechanical rewrite exists.

## Rewrite

`consumer <<< WORD`  →  `echo-of-WORD | consumer`:

| bash                        | sash                          |
|-----------------------------|-------------------------------|
| `cat <<< "$x"`              | `echo $x \| cat`  (sash `echo` doesn't word-split, so no quotes needed) |
| `read v <<< "$x"`          | `echo $x \| read v`          |
| `cmd <<< "$(sub)"`         | `sub \| cmd`  (the `$(…)` *is* the producer — drop the capture+feed and pipe directly) |
| `cmd <<< literal`          | `printf '%s\n' literal \| cmd`  or `echo literal \| cmd` |

Notes:
- Prefer `$(sub) <<<` → `sub | cmd` (pipe the command directly) over
  `echo $(sub) | cmd` — fewer moving parts, same bytes.
- bash's `<<<` appends a trailing newline; `echo`/`printf '…\n'` do too, so the
  byte stream matches.
- Where the consumer is a compound (`while …; done <<< "$(cmd)"`), the sash form
  is `cmd | while …; done` — which works in sash (no subshell). This is the
  common bash idiom and the highest-value case to get right.
- If a rewrite isn't safe/clear (e.g. a here-string feeding an assignment
  builtin in an odd position), emit a `# MANUAL:` prefix line like the existing
  heredoc handling (`convert.rs:132`).

## Where

`sash-convert/src/convert.rs`. tree-sitter-bash spells `<<<` as a
`herestring_redirect` (confirm the node kind + the `redirect`/`value` field
names against `tree-sitter-bash 0.25`). The heredoc path already emits a MANUAL
comment (convert.rs:132–134) — model the fallback on that, but prefer the
mechanical pipe rewrite for the common cases above.

## Found

2026-08-30, filed alongside DD#83 (drop here-strings). Journal 216.
