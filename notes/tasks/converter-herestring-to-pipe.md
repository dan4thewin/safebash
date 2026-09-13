---
title: "converter: rewrite bash here-strings `<<<` to a pipe (DD#83)"
status: done
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

## Done 2026-09-13 (journal 297)

`convert_herestring` in `sash-convert/src/convert.rs`, specs `57.102`–`57.110`.
Round-trip verified byte-identical to bash across all six runnable forms.

**The filing's `$(sub)` recommendation was right for the wrong reason.** It
preferred `cmd <<< "$(sub)"` → `sub | cmd` over `echo $(sub) | cmd` as "fewer
moving parts, same bytes". They are *not* the same bytes — bash's `$(…)`
collapses trailing newlines and `<<<` re-adds exactly one, while a direct pipe
passes every trailing newline through. The direct pipe is nonetheless the only
option, because `echo $(sub)` **does not compile**: a tainted command
substitution cannot be used inline (SB5020). So the rewrite emits the pipe and
a `# CHECK:` naming the divergence — it changes the iteration count of the
`while read` idiom when the producer ends in blank lines.

**One case is not `echo`.** A here-string of the literal word `-n`. Both
bash's and sash's `echo` read that as the retired no-newline flag — bare is
SB5059, quoted is silently swallowed — so `cat <<< -n` becomes
`printf "%s\n" -n | cat`. Only `-n`; `echo -e` prints `-e`.

**The bash quoting carries over unchanged.** The filing suggested dropping it
("sash `echo` doesn't word-split, so no quotes needed"). True, but keeping the
word verbatim is the choice that never has to interpret it, and `echo "$x"` is
as legal as `echo $x`.

## Two bugs found and fixed on the way

**`read v <<< "$x"` drew a false MANUAL.** tree-sitter-bash puts a
here-string redirect *inside* the `command` node (unlike `<` and `>`, which
wrap it in a `redirected_statement`), so `read_args` counted `<<< "$x"` as an
argument and `read_manual_reason` reported `read target is not a plain name —
sash binds a declared name, not a computed one (DD#99)`. The target is `v`.
Spec `57.107` pins the absence of that note. Same trap for `mapfile`.

**Patch ordering was push-order-dependent.** `apply_patches` sorted by `start`
alone, so a zero-width insertion at X and a replacement consuming forward from
X came out in whatever order they were pushed — and when the replacement won,
`pos` moved *backwards* and the output was garbage. Now sorted by
`(start, end)`, which puts insertions first by construction. This was latent
before this change; the here-string rewrite is the first pass to emit both at
one offset.

## Not done

`cmd <<< "$(sub)"` where the substitution holds more than one command
(`$(a; b)`) falls back to pass-through. Lifting it would mean deciding whether
`a; b | cmd` or a function is the right shape, and it is rare enough not to
guess.
