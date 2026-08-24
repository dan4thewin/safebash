---
title: "[[ ]] parser under-accepts file tests -k/-g/-u (setuid/setgid/sticky)"
status: ready
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
