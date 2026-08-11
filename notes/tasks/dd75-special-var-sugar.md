---
title: "DD#75 §1 — special vars are sugar for keyword forms ($? → $(status), $! → $(getcpid), $$ → $(getpid))"
status: ready
priority: P3
kind: task
depends: []
gates:
  - "$(status)/$(getcpid)/$(getpid) are the canonical forms (emitted by sash-convert, taught in docs, used in diagnostics); $?/$!/$$ accepted on input as sugar"
  - "DD#24 taint framing updated: clean status attaches to the keyword forms; $?/$!/$$ sugar inherits it; no value changes taint class"
  - "$LINENO remains a compile-time literal (exempt — no keyword form)"
  - "0 native/c/janet spec regressions; specs pin both spellings compiling to the same behaviour"
---

# DD#75 §1 — canonical/sugar inversion for punctuation specials

Implements DD#75 §1. Reverses the journal-050 "magic variable policy" per DG's
evolved intent (2026-08-11 pivot: unambiguous/self-consistent over bash-compat).

The lowerer already collapses `$(status)`→`$?` and `$(getcpid)`→`$!` to one AST
node; this task **reframes which spelling is canonical** (the keyword form) and
adds `$$`↔`$(getpid)`. Touch points: sash-convert output, docs (GUIDE/SUBSET),
diagnostic wording, and the DD#24 taint-source list.

See DD#75 §1.
