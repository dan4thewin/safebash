---
title: "c/janet backend gaps surfaced by wordsplit tally fix: 39.10 (regex \\w) and 22.12 (echo|read)"
status: ready
priority: P4
kind: bug
depends: []
gates:
  - "spec wordsplit 39.10 (`[[ $x =~ \\w+ ]]`) passes in c and janet (regex shorthand class support)"
  - "spec wordsplit 22.12 (`echo \"clean\" | read x; echo $x`) passes in c and janet (builtin echo|read pipe)"
---

# c/janet: wordsplit 39.10 and 22.12 fail (surfaced by tally fix)

## Context

`spec/wordsplit.sh` had a misplaced `tally` mid-file (line ~221): tests
appended after it (the "Additional coverage" block) only ran in **native**
(which passed the early tally and continued); c/janet exited at the early
tally because they had earlier failures. Fixed 2026-06-21 by moving
`tally` to the end of the file (closes `spec-wordsplit-count-desync`).

Moving the tally makes the previously-dead-zone tests run in **all**
modes. Most pass everywhere; two pre-existing backend gaps surfaced:

- **39.10** — `x="abc123"; [[ $x =~ \w+ ]] && echo yes` → expected `yes`.
  Fails in **c and janet** (passes native). The `\w` shorthand class in
  `=~` regex isn't handled by the c/janet regex lowering.
- **22.12** — `echo "clean" | read x; echo $x` → expected `clean`.
  Fails in **c and janet** (passes native). The builtin `echo | read`
  inline-pipe path differs in those backends.

These are **pre-existing** (the tests never ran in c/janet before, so
there is no regression — only newly-visible truth). They are unrelated
to the array-splat work that surfaced them.

## Fix

Bring the c/janet regex lowering (`\w` and friends) and the echo|read
inline-pipe to parity with native, or (if intentional backend
limitations per the janet "illustrate the IR" purpose) guard the two
tests to the modes that support them with `in_modes`/`skip_chk`.
Decide which per backend.
