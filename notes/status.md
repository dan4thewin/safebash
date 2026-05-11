---
title: Status
modified: 2026-05-11
---

## Current branch: split-converter

## Priority: next correctness batch

### Just completed (session 2026-05-11)

**Four DDs fully implemented, zero regressions:**

- **DD#55** (22/22 gates): suppress prefix `- cmd`, $? scoping,
  $(status)/$(getcpid), SB4018 $(f) type error, || exit status reset
- **DD#58** (8/8 gates): int function capture via sexpr in $(( )),
  source preprocessor for nested sexprs, name collision SB4019
- **DD#31** (19/19 gates): already passing (fixed as side effect of
  DD#58 arithmetic comparison rewrite)
- **DD#22** (7/7 gates): read -f field separator, reject -r,
  pipeline inline builtins, mapfile+pipe fix

**Also fixed:**
- 21.2: pipeline builtin echo (inline builtins in forked children)
- Mapfile+pipe runtime: subscript lookup in test expressions
- codegen_tests: 3 additional tests now pass (+3)
- e2e_tests: 4 additional tests now pass (+4)

**Test suite status (no regressions):**
- e2e: 136 pass / 2 fail (brush compat, pre-existing)
- oracle: 59 pass / 0 fail
- taint: 42 pass / 0 fail
- codegen: 220 pass / 46 fail (pre-existing pipeline/cmdsub gaps)

### Spec gate status

All DD#55, DD#58, DD#31, DD#22 gates pass.

Still failing (from other spec files): various DD#14 (pipeline/cmdsub),
DD#12 (string runtime), DD#30 (type separation), DD#33 (array types),
DD#56 (exit trap/signals).

### Next priorities

1. DD#56 EXIT trap bug + END block redesign (5 gates, bug)
2. DD#30 type separation (10 gates, correctness)
3. DD#33 array/map type enforcement (6 gates, correctness)
4. DD#14 pipeline primitives (6 gates, correctness)
