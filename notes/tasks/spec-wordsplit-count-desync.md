---
title: "spec hygiene: wordsplit.sh native emits plan+1 (count desync vs c/janet)"
status: ready
priority: P6
kind: bug
depends: []
gates:
  - "wordsplit.sh emits the same number of TAP results in native, c, and janet (plan exact in all modes)"
  - "find the chk/skip_chk that runs in native but not c/janet (somewhere before the 62.8-62.10 in_modes block) and balance it"
---

# wordsplit.sh native/c count desync

## Symptom

`spec/wordsplit.sh` emits **one more** TAP result in native mode than in
c/janet. Pre-existing (committed `plan=44`: native emits 45 — `ok 1..44`
then a stray `ok 45`; c/janet emit 44). The plan matches c/janet, so
native prints `PASSED` with a count-vs-plan mismatch (benign — native
genuinely passes, but sloppy).

The shift occurs **before** the `in_modes native` block at lines ~207-218
(that block is balanced: 3 `chk` in the `then`, 3 `skip_chk` in the
`else`). So there is an earlier `chk` that runs in native but emits
nothing in c/janet — likely an `if in_modes native; then … chk X; fi`
without an `else skip_chk X`, or a mode-conditional emission.

## Fix

Find the unbalanced site and add the missing `skip_chk` (or restructure)
so all three modes emit the same count, then set the plan exactly. Did
not chase during the cstring-in-double-quotes work (orthogonal,
pre-existing).

## Discovery

2026-06-21 adding wordsplit 64.4-64.8 (DD#64 addendum). The new tests are
unguarded (run in all modes), so they preserve — not cause — the desync.
