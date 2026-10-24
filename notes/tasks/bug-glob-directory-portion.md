---
title: "Bug: a glob with a directory portion (`sub/*`, `./*`, `/etc/*`) silently matches nothing"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "`sub/*`, `./*`, `../x/*` and `/abs/dir/*` expand to the matching entries of that directory, as DD#73's Decision and Form rule 2 specify, in both backends"
  - "each element is the lexically normalized absolute path (DD#73), e.g. `./sub1/*` gives `<cwd>/sub1/f1`"
  - "specs pin a relative, a dot-relative, a parent-relative and an absolute directory portion in both backends; each fails on the pre-fix binary"
  - "the gate and Tests section of `glob-absolute-paths.md`, which claim `sub/*` already works, are corrected or cite this task"
---

## What

Every glob whose literal part contains a `/` compiles cleanly and yields
nothing, in both backends:

| Pattern | Run in | DD#73 requires |
|---|---|---|
| `sub1/*` | `e`, holding `sub1/f1` | `…/e/sub1/f1` |
| `./sub1/*` | `e` | `…/e/sub1/f1` |
| `./*` | `e/sub1` | `…/e/sub1/f1` |
| `../sub2/*` | `e/sub1` | `…/e/sub2/f2` |
| `/tmp/…/d/a*` (array) | anywhere | `…/d/a1 …/d/a2 …/d/a3` (`len` prints 0 today) |

Bash finds the same entries (it keeps the relative spelling).

(Probes `m6`, `m7`, `m9`, `h1` from the batch-531 port's F13 glob set.)

## Why it is a bug, not a question

DD#73 settles it. Its Decision lists `sub/*`, `./*` and `/etc/*` with the
paths each must yield (`array a=(sub/*)` → `/work/sub/foo`), and its Form
rule 2 normalizes `./*`, `../x/*` and `sub/./*`. Its "Implemented
(2026-07-29)" note says only cwd globs exist and calls the directory join
"forward-looking". `glob-absolute-paths.md` was closed `done` on the same
day, but its gate (line 10) and Tests section still list `sub/*` →
`<cwd>/sub/…`, which never held. Its resolution says the truth: "Current
globs are cwd-only".

## Cause

Both backends pass the directory `"."` to `sash_glob` and the whole pattern
as the name pattern:

- native: `emit_glob_expansion` (`sash/src/codegen/value_handlers.rs`),
  `let dir_str = ".";`
- C: `emit_glob_c` (`sash/src/emit_c.rs`), `sash_glob(".", 1, …)`

A directory entry's name never contains `/`, so a pattern with a `/` can
never match.

## Fix

Split the literal at the last `/` before the star segment. The directory
part goes to `sash_glob` as `dir` and the rest as the pattern. A leading `/`
makes it absolute; anything else joins with `getcwd`.
`sash_glob_abs_prefix` and `sash_normalize_abs` already do the joining and
normalizing for a non-`.` dir. The transform's glob recognisers
(`try_array_single_glob` in `transform/expressions.rs`, `try_dispatch_glob`
in `transform/control_flow.rs`) must carry the split pattern to both
emitters. A `*` before the last `/` (`*/f1`) is a separate, open question
(`q-glob-semantics`, item 4).

Found during the batch-531 port (F13), unrelated to the port itself. No
current spec uses a directory-portion glob: glob.sh g.1–g.14 and the
wordsplit glob checks are all cwd patterns.
