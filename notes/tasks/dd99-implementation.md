---
title: "DD#99 implementation — read/readline, NUL-only, no flags"
status: in-progress
priority: P1
kind: task
depends: []
gates:
  - "`readline v` reads one line into a scalar; `read a` reads NUL-separated fields into an array; `read $s a` splits a scalar"
  - "`map m = @{a:DISP}` builds a map pairwise, disposition mandatory; `array a = @m` is rejected"
  - "`write` is echo-without-newline; `echo -n` is rejected"
  - "read -a / -f / -d / -r, mapfile, readarray and multi-target `read a b c` are all rejected"
  - "an argv entry containing an interior NUL aborts at runtime rather than truncating silently"
  - "3-mode sweep with 0 regressions at every stage"
---

# Staged plan

`read x` **changes meaning** (scalar line → array of NUL fields), so the new
spellings are added first, the tree is migrated onto them, and only then are the
old ones retired. Each stage is independently verifiable with a 3-mode sweep.

| # | stage | migration load |
|---|---|---|
| 1 | add `readline v` (one line → scalar) | — additive |
| 2 | migrate single-target `read v` → `readline v` | ~89 sites |
| 3 | `read a` → array-only NUL-split; add `read $s a` | ~27 + 61 sites |
| 4 | `map m = @{a:DISP}` (pairwise, disposition mandatory) | — additive |
| 5 | migrate `mapfile`/`readarray` → `read` + `map` | ~63 sites |
| 6 | add `write`; migrate `echo -n` | 2 sites |
| 7 | retire the old spellings (reject with diagnostics) | — |
| 8 | argv interior-NUL runtime guard | — |
| 9 | `array a = @m` → compile error | — |

Scale: ~250 sites across 25 of 38 spec files.

## Known blockers, all filed and independent

These are broken today and the design leans on them:

- `bug-while-read-a-zero-iterations` — `while read -a` runs zero iterations
- `bug-sed-op-on-array-empties-it` — `a =~ s///` silently empties an array;
  `gsub @a` silently joins the splat
- `bug-match-outarray-no-autodeclare` — `match S P ARR` needs a prior decl

Stage 3 subsumes the first (the `-a` shape disappears); the others are
independent.

## Progress

- **Stage 1 DONE** (2026-09-09): `readline v` added — one line into one scalar,
  no flags, exactly one bare target (SB5057). It shares the scalar lowering
  with today's single-target `read`, so stage 3 can give `read` the array-only
  form without the two meanings colliding. Specs `wordsplit.sh` rl.1–rl.10.
  Sweep 4376 passing / 39 failing / 405 skipped, 0 regressions.
  - rl.7 (`while readline`) is gated native-only: the identical `while read v`
    shape also fails in c/janet, so that is a pre-existing backend gap.
- Stage 2: next — migrate single-target `read v` → `readline v` (~89 sites)
