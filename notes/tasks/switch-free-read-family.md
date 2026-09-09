---
title: "switch-free builtins: decompose `read`/`mapfile`/`echo` flags into distinct commands + sigils (the readr/readc question)"
status: superseded
priority: P2
kind: decision
depends: []
gates:
  - "DG decides how far to take switch-free builtins: read → readr/readc + drop -a/-f/-d/-z (via @arr sigil + sed-style split ops), and whether mapfile/echo flags follow"
  - "the two inconsistent `parse_read_flags` are unified or deleted; the -r three-way inconsistency is resolved"
---

> **UPDATE (DG, 2026-09-02): `readc` was DROPPED.** The cooked-read approach
> (parse getopt's shell-quoting with a backslash-honoring read) was retired as
> too fragile — parsing a program's quoting is the eval-adjacent thing sash
> avoids. The sanctioned getopt idiom is instead **NUL-splitting**: vendor a
> patched `sash-getopt` (util-linux getopt, defaulting to NUL field separation +
> no quoting) piped into a NUL-delimited `read`. `readc` and all its cooked
> machinery were removed (journal 230). `sash-getopt` is vendored + building +
> tested (journal 231, `sash-getopt/sash-getopt.c`).
>
> **OPEN — the NUL-read consumer surface.** The runtime is ready
> (`sash_split_into_array` is length-based / NUL-safe; `read -a arr` splits on
> `flags.delim`), so only the *surface* for requesting a NUL delimiter is
> undecided: `read -z` (currently REJECTED, SB5015 / spec cov.20), `read -d ''`
> (bash's NUL convention; `consume_byte_flag` would need empty → 0), a distinct
> command, or the `@arr` splat. This is the concrete next decision for this
> task.

> **SUPERSEDED 2026-09-08 by DD#99** (`notes/decisions/DD99-read-family.md`),
> which answers all five "Decisions for DG" below: the family becomes
> `read [$SRC] TARGET` + `readline TARGET`, NUL is the only separator, the
> target's declared kind selects array vs map (array by default), and every
> flag is retired. `echo -n` becomes `write`. DD#99 is `proposed`, with three
> open questions of its own.
>
> Note the "Progress" section below is stale: `readc` was **removed**, not
> shipped — `readc` is SB5011 ("command not found") in the current build.

# Switch-free builtins — the `readr`/`readc` question

DG (2026-09-01): "I don't like the switches (how robust even is our switch
parsing?). I'm wondering if read should simply not exist, and instead there is
readr and readc." Fits the sash ethos of **distinct constructs over flags**
(DD#60 `argv`/`arg`/`argc` replacing `$@`/`$*`/`$#`; DD#75 keyword forms;
`int`/`array`/`map` keywords replacing `declare -i/-a/-A`).

`readc` (cooked read) is being implemented **now** as the concrete first step
(it unblocks the getopt-csh idiom — see below). This task captures the larger
decision: how far to carry switch-free builtins, and what happens to the other
read/mapfile/echo flags.

## Why switches are the weak spot here

- **Two disagreeing `parse_read_flags`.** `transform/cmd_handlers.rs` rejects an
  unknown flag (SB5015); `transform/stmt_handlers.rs` silently **ignores** any
  unknown `-`flag (`if s.starts_with('-') { continue }`). So the same `read -Q x`
  is a hard error in one shape and a silent no-op in another.
- **`-r` is inconsistent three ways** (before this task's `readc` step):
  standalone `read -r` and `while read -r` → SB5015; lastpipe `cmd | read -r x`
  → silently accepted-and-ignored. And `-r` is redundant anyway — sash `read` is
  always raw.

## Survey — every switch the compiler currently parses

| command | switch | meaning | switch-free candidate |
|---|---|---|---|
| `read` | `-r` | raw (no backslash processing) | **the command name** (`readr`) — raw is the current sole behavior |
| `read` | *(new)* | cooked (honor `\` escapes) | **`readc`** (implemented now) |
| `read` | `-a arr` | read into an array | the `@arr` collection sigil: `readc @arr` |
| `read` | `-f<c>` | field separator | drop — read the line, split with a sed-style op (`gsub`/`split`); sash already has these (DG's own argument) |
| `read` | `-d<c>` | line delimiter | rare; a dedicated form or drop |
| `read` | `-z` | NUL-delimited | rare; a dedicated form or drop |
| `mapfile`/`readarray` | `-t` | trim trailing newline | ? (mapfile is already array-oriented) |
| `mapfile` | `-m` | sash map mode (KEY<sep>VALUE → map) | ? (could be a distinct `mapmap`/keyword) |
| `mapfile` | `-f<c>` | field separator | same as read `-f` |
| `echo` | `-n` | no trailing newline | ? (very common; `print`/`printf` split?) |
| `echo` | `-e` | interpret escapes | ? |
| `int`/`array`/`map`/`const` | (`-i`/`-a`/`-A`/`-r`) | already keywords | done — the model this task generalizes |

## The read decomposition (proposal)

- **`readr` / `readc`** — read one line, raw / cooked. Optionally whitespace-split
  into bare vars (`readc a b c`) or an `@array` (`readc @arr`). No flags.
- **custom field separators / delimiters** → explicit sed-style ops on the line
  (not read switches). This is exactly DG's "we have sed ops" point applied:
  the getopt-csh idiom becomes `readc line < <(getopt -s csh …)` then split +
  `gsub '' →` to strip quotes — no `eval`, no switches.
- delete the two `parse_read_flags`; the flag parser and its bug vanish.

Worked getopt-csh proof (verified in bash): cooked read keeps `'has'\ 'space'`
as one field (`\ ` holds it together); the single-quotes are a trivial `gsub`.
Raw read mangles it. So `readc` + sed ops = eval-free option parsing.

## Decisions for DG

1. **Scope:** (A) just `readr`/`readc` for raw/cooked, keep `-a`/`-f`/`-d`/`-z`;
   or (B) full switch-free read family (`@arr` sigil, sed-ops for splitting,
   flag parser deleted). DG leaning implied toward B.
2. **`read` itself:** rename `read` → `readr` (raw stays the raw command) and
   retire bare `read`? Or keep `read` as an alias?
3. **`-f`/`-d`/`-z`:** drop from read (sed-ops / dedicated NUL form), or keep one
   parameter for convenience?
4. **mapfile / echo:** do their flags (`-t`/-m/-f, `-n`/-e) get the same
   treatment, or are they out of scope?
5. **converter:** bash `read`→? `read -r`→? `read -a`→? mapping once the family
   is set.

Produces a DD (generalizes DD#60/#75 "constructs over flags" to the read family)
and retires the flag parsers. `readc` (this task's first step) ships
independently and stays valid whichever way the rest lands.

## Progress

- **`readc` (cooked read) — SHIPPED native + c** (journal 225). Runtime
  `sash_split_first_word_cooked` + `sash_deescape`; `cooked: bool` on
  `ReadLine`/`ReadLineMulti`; recognized everywhere `read` is. specs
  `spec/wordsplit.sh` 22.20–22.24. janet deferred (`janet-readc-cooked`).
  This does **not** resolve the decisions below — bare `read` still exists
  (raw), the flag parsers and their `-r` three-way inconsistency remain.
- Still open: everything under "Decisions for DG" (readr rename, drop
  `-a`/`-f`/`-d`/`-z`, unify/delete the two `parse_read_flags`).

