---
title: "sash-convert + docs — emit/teach the canonical keyword specials ($? → $(status), $! → $(getcpid), $$ → $(getpid))"
status: ready
priority: P3
kind: task
depends: [dd75-special-var-sugar]
gates:
  - "sash-convert rewrites bash `$?`→`$(status)`, `$!`→`$(getcpid)`, `$$`→`$(getpid)` on output (input sugar still accepted by the compiler)"
  - "GUIDE.md / SUBSET.md teach the keyword forms as canonical and note the punctuation forms as accepted sugar"
  - "round-trips: converted output compiles clean; the equivalence (misc.sh spv.1-4) still holds"
---

## Why

DD#75 §1 makes the keyword forms (`$(status)`/`$(getcpid)`/`$(getpid)`)
canonical and the `$?`/`$!`/`$$` punctuation forms input sugar. The compiler
side landed 2026-08-14 (equivalence pinned, SB5012 diagnostic reframed, the
compound-arith silent-0 bug fixed). This is the remaining reframe of the two
surfaces that carry the project's *voice*:

- **sash-convert output** — split out per DG's standing convert-as-follow-up
  rule. The converter should emit the keyword spelling when lifting bash
  `$?`/`$!`/`$$`.
- **GUIDE.md / SUBSET.md** — public-facing prose; DG's voice. Reframe the
  specials section so the keyword form is taught as canonical.

Kept out of the compiler commit deliberately: the compiler already accepts both
spellings, so nothing is blocked; these are documentation/tooling changes best
reviewed with DG.
