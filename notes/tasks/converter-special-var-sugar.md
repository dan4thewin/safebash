---
title: "sash-convert + docs — emit/teach the canonical keyword specials ($? → $(status), $! → $(getcpid), $$ → $(getpid))"
status: done
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

## Done 2026-09-13 (journal 298)

`canonical_special` + `in_arithmetic` in `sash-convert/src/convert.rs`;
`.SH SPECIAL VALUES` in `docs/sash.1`; the two `GUIDE.md` mentions corrected.
Specs `57.111`–`57.115`. Sweep 4445 passing, 0 failing, 0 flips.

**Gate 2 named SUBSET.md, which no longer exists** (removed 2026-08-29 — it had
drifted from the DDs). The manpage is where language surface is taught now, and
it did not mention the keyword forms *at all*: `getpid`, `getcpid` and `status`
appeared nowhere in `docs/sash.1`. So the new section is not a reframe of
existing prose, it is the first documentation these three have had.

**There are two canonical spellings, not one**, and the gate's flat mapping
would have produced non-compiling output:

| context | canonical |
|---|---|
| command position | `$(status)` / `$(getpid)` / `$(getcpid)` |
| arithmetic operand | `(status)` / `(getpid)` / `(getcpid)` |

`$(status)` inside `$(( ))` is **SB4020** — a command substitution is not an
arithmetic operand. The punctuation sugar reads the same in both, which is
exactly why a mechanical rewrite has to know its context. `$#` already had
this shape (`$(( (argc) - 1 ))`) and the note on it in `convert_simple_expansion`
describes the same trap.

**These three convert inside a function body**, unlike `$@`/`$*`/`$#`. They are
process-wide; the positionals are the caller's arguments. The existing
`in_function` early-return had to move.

## One bug found, one bug fixed on the way

**`$(( (getcpid) ))` was a silent 0** — see the addendum on
`bug-arith-keyword-pid-intrinsic-zero`, which was marked *done* with that exact
gate unmet. It blocked this task: the converter cannot emit an arithmetic
spelling that evaluates to the wrong value.

**`[ … ]` conversion corrupted its own output** — unfiled, pre-existing, and
broad. See the journal; `if [ "$1" = x ]` produced
`if [[ $1 = x ]]$arg1" = x ]`.
