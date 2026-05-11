---
title: Status
modified: 2026-05-11
---

## Current branch: split-converter

## Priority: DD#58 + remaining gates

### Just completed (session 2026-05-11)

**DD#55 full implementation — all 25 spec gates pass:**
- Suppress prefix (`- cmd`) in grammar, IR, lowerer, transform
- Per-segment pipeline suppression (`- cmd1 | cmd2`)
- `$?` scoping: SB5013 (after `-`), SB5014 (stale in handling branch)
- `$(status)` → `$?` and `$(getcpid)` → `$!` syntactic sugar
- `$(f)` for `-> int` function → SB4018 compile error
- `||` exit status reset (RHS completion → exit 0)
- Taint fixes for suppressed command substitutions
- SSA taint: basename matching for `/bin/echo` etc.

**New in tree-sitter-bash-safebash:**
- `suppressed_command` node (like `negated_command` but with `-`)

### Spec gate status

Passing (newly): 55.4, 55.5, 55.9, 55.11, 55.12, 55.15, 55.17b,
  55.17c, 55.18, 55.19

Still failing: 31.4a/b/c, 58.1-58.8 (no implementation yet)

### Next priorities

1. DD#58 implementation (sexpr parsing in arithmetic, $(f) type error)
2. Push and update TIMELINE
3. Second autonomous batch run with remaining tasks
4. 31.4a/b/c (arithmetic side effects — DD#31)
