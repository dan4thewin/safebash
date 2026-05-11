---
title: Status
modified: 2026-05-11
---

## Current branch: split-converter

## Priority: DD#58 nested sexpr + remaining gates

### Just completed (session 2026-05-11)

**DD#55 — all 22 gates pass (complete):**
- Suppress prefix, $? scoping, $(status)/$(getcpid), SB4018, || exit reset

**DD#58 — 7/8 gates pass:**
- IntFuncCall in arithmetic: $((f)), $(( (add 3 4) )), ((x = f)),
  if ((f > 0)), $(( (f) + 1 ))
- $(f) for -> int → SB4018 compile error
- Function/variable name collision → SB4019
- Arithmetic comparison desugaring → raw text (function-aware)
- __let_exit now sets test_result_var (fix for if/while)
- return uses expr_to_value (arithmetic return values work)

**Still failing: 58.3** — nested sexpr $(( (add (dbl 3) 4) )).
Tree-sitter can't parse nested parens with space-separated tokens
in arithmetic. ~~Needs grammar work or source preprocessing.~~
**FIXED** — source preprocessor replaces sexpr content with
placeholders before tree-sitter, transform resolves them.
All 8 DD#58 gates now pass.

### Spec gate status

Passing (newly): 55.4-55.19 (all), 58.1, 58.2, 58.4-58.8

Still failing: 31.4a/b/c

### Next priorities

1. 58.3 — nested sexpr (tree-sitter grammar or preprocessor)
2. Push and update TIMELINE
3. 31.4a/b/c (arithmetic side effects — DD#31)
