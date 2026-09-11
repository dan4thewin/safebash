---
title: "docs: full write-up of the regex dialect (man page)"
status: ready
priority: P2
kind: docs
depends: []
gates:
  - "docs/sash.1 (or GUIDE) states the engine is Plan 9 / Pike's, chosen deliberately, and that POSIX compliance is not a goal"
  - "the supported surface is listed, including `{n,m}` (cap 32) and `\\d` `\\w` `\\s` `\\D` `\\W` `\\S`"
  - "the deliberate divergences are listed with their spelling: `\\-`, `\\{`, `\\[`"
  - "the unsupported surface is listed (lookahead/lookbehind)"
  - "each claim is verified against the binary, not against DD text"
---

## What

DG, 2026-09-11:

> the regex library is chosen on faith in the author, Rob Pike. It represents
> his view of a sane pre-feature-creep implementation. POSIX compliance is not
> a goal. […] I have made small changes for convenience, not compliance. **It
> deserves a full write-up in the future docs/man page**

The ruling is recorded in `notes/decisions/DD39-glob-regex.md` (clarification
of 2026-09-11). This task is the user-facing write-up it asks for.

## Why it matters

Without it, every divergence from bash's `=~` arrives as a bug report. One
already did (`bug-regex-literal-dash-in-bracket`, closed as not-a-bug), filed
P1 with a three-way fork for DG — all of which dissolved once the posture was
stated. A documented dialect converts that whole class of report into a lookup.

## Source material

- `notes/decisions/DD39-glob-regex.md` — the engine section plus three
  clarifications (2026-05-24 runtime abort, 2026-07-08 `{`/`[`, 2026-09-11
  posture + corrections)
- `notes/decisions/DD63-regex-quoting-interpolation.md`
- `sash-rt/csrc/regcomp.c` vs `contrib/plan9port-regexp/regcomp.c` — the only
  modified file; `regexec.c` and `regaux.c` are byte-identical to upstream.
  The additions are `parsecounted` and `bldshortclass`.

## Caution

DD#39's own prose was **stale in two places** until 2026-09-11 (it claimed
`\d`/`\w` unsupported, and the top section still claims no `{n,m}`). Verify
every claim against the compiler before writing it down; do not transcribe
the DDs.

## Related

- `notes/tasks/bug-regex-rhs-space-quote.md`
- `notes/tasks/bug-regex-blind-past-interior-nul.md` — a real bug (wrong
  answers past an interior NUL), not a dialect question
- `notes/tasks/limitation-regex-stops-at-interior-nul.md`
