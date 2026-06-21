---
title: "BUG — janet backend: (( )) used as an if/while condition always takes the true branch"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "janet: `if (( 7 < 3 )); then …; else …; fi` runs the else branch (and all comparison/false (( )) conditions branch correctly)"
  - "drop the in_modes native,c guard on spec arithmetic.sh 31.17/31.18"
---

# BUG — janet `(( ))` condition always true

## Symptom (janet backend only)

```
if (( 7 < 3 )); then echo then; else echo else; fi   # janet prints "then"
```

All `(( ))` comparison conditions (`<`, `<=`, `>`, `==`, …) take the
`then`/loop branch even when false. native and c are correct.

## Likely cause

The janet backend (`emit_janet.rs`) evaluates a `(( ))` condition by
value-truthiness and/or doesn't thread the comparison result into the
branch the way native does (native: `LetExit` → `test_result_var` →
`Terminator::Branch`). A janet comparison yields `true`/`false` (or the
arith value), and the if/while lowering likely isn't testing it as a
0/non-0 status. Related to `bug-andor-backend-chain-status` (janet runs
`&&`/`||` chains by value-truthiness, not exit status) — same family.

Also note `echo $(( a < b ))` prints empty in janet (comparison in value
context), which may share a root.

## Discovery

Found 2026-06-20 adding `<=`/`>=` regression spec tests (arithmetic.sh
31.15–31.18) after fixing the `(( <= ))`/`(( >= ))` mis-lex bug. The
parse fix is correct (native + c pass all four; janet passes the
true-branch cases 31.15/31.16). 31.17/31.18 (false branch) are guarded
`in_modes native,c` pending this fix.
