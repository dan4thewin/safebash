---
title: map iteration ("keys"/"values" broken) — RESOLVED, diagnosis was wrong
status: done
priority: P1
kind: bug
depends: []
resolved: 2026-06-17
gates:
  - keys + dynamic subscript idiom iterates a clean map (types.sh 33.10)
  - keys of a tainted unblessed map rejects SB5020 (taint.sh 60.35)
  - blessed map keys idiom works (taint.sh 60.36)
---

# map iteration — RESOLVED (corrected diagnosis)

Filed 2026-06-16 as "keys emits garbage / values ICEs / keys leaks
tainted map." On investigation (2026-06-17, with DG) **the original
diagnosis was wrong**. The actual story:

## What was really happening

1. **`values` is not a builtin.** DD#41 defines `len`/`keys`/`join`
   only. `$(values m)` was just calling a nonexistent external command
   `values`; the "ICE" was the generic *for-in-over-external-cmdsub*
   ICE (`for v in $(ls)` ICEs identically), not a map bug. No `values`
   builtin was added — values are retrieved via `${m[$k]}`.

2. **`keys` was never broken.** On a correctly-built map (`mapfile -m`
   or `[k]=v` literal) `keys` yields the keys in insertion order. The
   "garbage" (4 blank iterations for a 2-key map) came entirely from
   the flat literal `map m=(a 1 b 2)` being **silently mis-stored** as
   a 4-element indexed array — see bug (A) below.

## The three real bugs (all fixed 2026-06-17)

- **(A) Flat map literal mis-stored.** `map m=(a 1 b 2)` parsed as an
  indexed `Array` (len=4, `${m[a]}` -> `a`), not a 2-entry map. Now
  **rejected SB4031** (DD#33) directing to the `[k]=v` form.
  `validate.rs::reject_flat_map_literal`. Spec types.sh 33.9.

- **(B) Dynamic subscript was unimplemented.** The parser literalised
  the index, so `${m[$k]}` missed (empty), `${a[$i]}` hit SB4030, and
  `a[$i]=v` / `m[$k]=v` mis-parsed as a command (`sash: bug:` abort).
  Now the `$var`/`${var}` index expands at the three read sites
  (`expr.rs::subscript_index_expr`) and the split-token write LHS is
  recognised (`parser.rs::parse_split_subscript_index`). Spec
  types.sh 33.5-33.8, 33.10.

- **(C) `keys` of a tainted unblessed map escaped taint.** `keys m`
  takes a bare name arg, so the cmdsub-arg recursion didn't see it.
  Now `validate.rs::check_collection_reader_taint` treats it as a use
  site -> SB5020 (`len` stays exempt). Spec taint.sh 60.35; blessed
  positive 60.36.

See journal 113.
