title: Pair the regex limitation with its NUL guard
date: 2026-11-18

# Pair the regex limitation with its NUL guard

DG noticed that the regex slide mentioned the first-NUL limitation without
showing the existing predicate that makes whole-string validation possible.
DD135 fixes its polarity: `0?` means contains NUL, so the guard is `! 0?`.

The AI revised the existing anchors example to compare `apple`,
`apple\npear` and `apple\x00pear`. Its predicate checks `! 0? $text` before
the anchored regex. Only the plain word is accepted. The explanation names
the otherwise-hidden suffix: without the NUL check, the regex accepts the
`apple` prefix of the third input.

The slide retains the whole-subject/global-anchor and newline rules.
It also explains that tainted input needs both checks in the same `attest`
predicate; an ordinary condition does not clear taint. This documents the
implemented predicate, not a regex-engine change or new validation policy.

The existing tour renderer runs all **70 programs**, with **15 intended
failures** and zero outcome mismatches. GFM/output comparisons confirm that
the other **69 programs and their recorded outputs are unchanged**.
The revised output is accepted, rejected, rejected; an actual-CLI control
removing only the NUL guard instead prints accepted, rejected, accepted.

Three further actual-CLI runs acquire the inputs from a file and apply the
combined attest. The plain word succeeds; newline and NUL inputs fail the
attest before any success output. Normal taint checking is enabled, and
all temporary control files are removed.

The baseline is `aec434de` on `glob-scratch-trial`. Only the authored deck,
generated README, current status and this journal change. The existing
product gate remains journal 655's sweep ending at row 7271250; no new
product sweep, C-backend measurement, coverage or interactive-layout
result is claimed. Main remains unchanged.

Private evidence is in the session's `tour-regex-nul-20261118` directory.
