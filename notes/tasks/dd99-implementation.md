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

DG (2026-09-09): *"maybe call it `newread` instead of `read` — finish
implementing `readline` and `newread` and then replace."*

The array read ships under a **temporary name**, so at no point does an
existing spelling change meaning while code still depends on it. The old
`read` keeps working untouched until everything has moved off it.

| # | stage | migration load |
|---|---|---|
| 1 | add `readline v` (one line → scalar) ✅ | — additive |
| 2 | add `newread a` / `newread $s a` (NUL fields → array) | — additive |
| 3 | add `map m = @{a:DISP}` (pairwise, disposition mandatory) | — additive |
| 4 | add `write` (echo without the newline) | — additive |
| 5 | migrate spec/ + contrib/ onto the new spellings | ~250 sites |
| 6 | retire `read -a/-f/-d/-r`, `mapfile`, `readarray`, `echo -n`, multi-target read; **rename `newread` → `read`** | — |
| 7 | argv interior-NUL runtime guard | — |
| 8 | `array a = @m` → compile error | — |

Scale: ~250 sites across 25 of 38 spec files.

Stages 1–4 are purely additive and independently verifiable. Stage 6 is the
only breaking one, and by then nothing depends on the old spellings.

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
- **Stage 2a DONE** (2026-09-09): `newread ARRAY` — reads stdin, splits on
  NUL, fills an auto-declared array. No flags, exactly one bare target
  (SB5057, shared with `readline`). Lowers to the existing `ReadArray` with
  `delim = Some(0)`. Specs `wordsplit.sh` nr.1–nr.8, including nr.5 — the full
  DD#99 round trip with no external `tr`:

  ```sh
  sep=$'\0'; s=a:b:c; s =~ s/:/$sep/g
  echo $s | newread f          # → 3 elements
  ```

  Sweep 4395 passing / 39 failing / 410 skipped, 0 regressions.
- Stage 2b: next — `newread $SRC ARRAY` (split a scalar). Needs a new SSA
  instruction; `sash_split_into_array` already does the work.
