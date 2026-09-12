---
title: "[[ ]] parser under-accepts file tests -k/-g/-u (setuid/setgid/sticky)"
status: done
priority: P4
kind: feature-gap
depends: []
gates:
  - "[[ -k f ]] / [[ -g f ]] / [[ -u f ]] parse as unary tests and codegen (file_test_op_code already implements codes 10/12/13)"
  - "spec coverage for the three; 0 regressions native/c/janet"
---

## Context

Found during the `-o`/`-R` ICE fix (journal 177). `file_test_op_code`
(`codegen/tests.rs`) already implements `-k` (sticky, code 10), `-g` (setgid,
12), and `-u` (setuid, 13) — but the parser's `is_unary_test_op`
(`parse/parser.rs`) does **not** list them, so `[[ -k f ]]` never parses as a
unary test. The capability exists in codegen but is unreachable.

This is a **missing feature (parser under-accepts), not an ICE** — unlike
`-o`/`-R` which the parser over-accepted (fixed in 177). `[[ -k f ]]` today
parses `-k`/`f` as two words and errors confusingly rather than testing the
sticky bit.

## Fix

Add `-k`, `-g`, `-u` to `is_unary_test_op`. Codegen already handles them
(file_test_op_code 10/12/13) and the runtime file-test path covers stat-mode
bits, so this should be parser-only. Verify the runtime `emit_file_test` codes
match. Add spec coverage.

## Provenance

Parser-vs-codegen unary-test-op set diff, journal 177. The reverse direction
(parser over-accepts) was the `-o`/`-R` bug already fixed.

## Verified 2026-09-12 — confirmed exactly as filed

Set-diff of the two functions:

- `codegen/tests.rs::file_test_op_code` (227–249) implements **20** ops
  including `-k`→10, `-g`→12, `-u`→13.
- `parse/parser.rs::is_unary_test_op` (1488–1520) omits **exactly those
  three** and no others from the file-test family.

Measured: `[[ -k f ]]`, `[[ -g f ]]`, `[[ -u f ]]` → **SB4014**. The
capability is implemented and unreachable, as filed.

Also checked the reverse direction the filing cites: `-o`, `-R`, and `-v` are
still listed in `is_unary_test_op`, but they now produce a clean **SB5028**
rather than an ICE — journal 177's fix holds. No action there.

### Found while sweeping: `-N` is wrong

Sweeping all 18 file-test operators against bash (3 target types each) turned
up exactly one divergence, and it is not in this task's three: `[[ -N f ]]`
answers `no` where bash answers `yes`. Filed separately as
`bug-file-test-N-diverges-from-bash` (P2). Worth doing that sweep as a spec
while adding `-k`/`-g`/`-u` coverage here.

## DONE 2026-09-12

Parser-only, exactly as the filing predicted. `-k`, `-g`, and `-u` added to
`is_unary_test_op`; `file_test_op_code` (10/12/13) and the `sash-rt` stat-mode
paths needed no change at all.

Verified against bash directly before writing the assertions: all nine
combinations of the three operators against a set-bit file, a clear-bit file,
and a missing path agree.

New spec section D in `spec/filetest.sh`, `ft.29`–`ft.36` (plan 28 -> 36),
gated native,c to match the rest of the file (janet lowers only -d/-e/-f).
Each bit is tested both **set and clear**, so a constant-`no` implementation
cannot pass. `ft.36` is a regression guard that `-g` the mode bit does not
collide with `g?`, the DD#93 content-shape test sharing the letter.

Fail-first verified: `ft.29`–`ft.35` all fail on the pre-change compiler
(SB4014); `ft.36` passes on both, which is what a guard should do.

Sweep: 5321 passing / 41 failing / 567 skipped, zero non-janet failures, and
the only flips are the 24 new rows.

### The reverse-direction note in the filing still holds

`-o`, `-R`, `-v` remain listed in `is_unary_test_op` and still produce a clean
SB5028 rather than an ICE. No action taken.
