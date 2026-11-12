title: "docs: full write-up of the regex dialect (man page)"
status: done
priority: P2
kind: docs
depends: []
gates:
  - "docs/sash.1 (or GUIDE) states the engine is Plan 9 / Pike's, chosen deliberately, and that POSIX compliance is not a goal"
  - "the supported surface is listed, including `{n,m}` (cap 32) and `\\d` `\\w` `\\s` `\\D` `\\W` `\\S`"
  - "the deliberate divergences are listed with their spelling: `\\-`, `\\{`, `\\[`"
  - "the unsupported surface is listed (lookahead/lookbehind)"
  - "each claim is verified against the binary, not against DD text"

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

## Closed 2026-09-13

`docs/sash.1` now has a `.SH REGULAR EXPRESSIONS` section between TEST
OPERATORS and SASH-CONVERT. Every claim in it was measured against the
binary, and none was transcribed from a DD.

All five gates met:

- **engine + posture** — states Plan 9 / Rob Pike, deliberate, Thompson
  NFA (linear match time, no catastrophic backtracking), and that POSIX
  compliance is not a goal.
- **supported surface** — `{n}` / `{n,m}` / `{n,}` with each bound capped
  at 32, and `\d \D \w \W \s \S`. Also `\xNN`, which the task did not
  list but DD#63 adds and the binary accepts.
- **divergences with their spelling** — `\-` for a literal hyphen in a
  bracket (`[ab-]` is rejected). `\{` and `\[` are not called out
  separately: measured, a backslash makes *any* metacharacter literal, so
  the section states the general rule instead of three instances of it.
- **unsupported surface** — lookahead, lookbehind, non-capturing groups,
  backreferences, POSIX character classes, `\b`, counted repetition of a
  group, empty alternatives, case-insensitive matching.
- **verified against the binary** — this gate is what earned the section
  its content. Probing found three things the DDs do not say:

## What the probing turned up

Running the dialect against the binary rather than reading about it found
**two bugs and one lexer collision**, all now fixed or filed:

1. **A heap overflow in `{n,m}`** reachable from a runtime pattern —
   journal 290, fixed.
2. **`(ab){2}` silently matched the wrong thing** — journal 290, now
   rejected; `notes/tasks/regex-counted-group.md` for DG.
3. **Every non-ASCII literal in sash source was double-encoded** —
   journal 291, fixed. Found because `café =~ ^caf.$` fit neither the byte
   model nor the character model.
4. **`[[ $s =~ ^((a))$ ]]` is rejected** as "expected `]]`" —
   `notes/tasks/regex-literal-double-paren.md`, filed.

## Added 2026-09-14 — the group-repeat rejection, as an engine limitation

DG: *add to the doc task to record this as a limitation of the regex engine.*

The section already listed "No counted repetition of a group" under the
unsupported surface, with `(ab)(ab)` as the workaround. What it did not say is
**why**, and that is the half that stops the bug report — the same gap that
made `bug-regex-literal-dash-in-bracket` arrive as a P1 with a three-way fork.

The entry now states:

- it is a limitation of the engine, not an oversight;
- the mechanism — the compiler expands a bound by re-lexing the single
  preceding *atom*, and `(` / `)` are operators rather than atoms, so the atom
  it would repeat is the group's last element;
- what the old behavior was: `(ab){2}` compiled as `(ab)b` and reported no
  match on `ababababab`, where bash matches;
- that rejecting is deliberate — a wrong answer with no diagnostic is worse
  than a diagnostic;
- the exact extent, measured: `a(bc){2}`, `(a)(b){2}` and `(a){2}b` are all
  rejected (it is not only a *leading* group), while `(ab)x{2}` is accepted
  because the repeated atom there is `x`.

Every one of those claims was probed against the binary, per the task's own
caution. Rendering checked with `man -P cat`; the one `groff` warning in the
file (`table wider than line width`) is pre-existing and unrelated.

## Corrections to the source material

- DD#39's top section still says there is no `{n,m}`. There is; the
  clarification further down is the current text. Not edited here (a DD
  is a record, and the 2026-09-11 clarification already supersedes it),
  but a reader of the top section alone will be misled.
- The task's "Source material" note that `regexec.c` and `regaux.c` are
  byte-identical to upstream still holds. `regcomp.c` now diverges in one
  more place than the note lists: besides `parsecounted` and
  `bldshortclass`, it has the `newinst` bounds check and the group-repeat
  rejection from journal 290.
- **`lib9.h` is the third modified file** and the note omits it. Its
  `chartorune` is an ASCII-only stub, which is *the* reason the dialect is
  byte-oriented — the single most user-visible divergence in the section.
