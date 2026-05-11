---
title: Status
modified: 2026-05-11
---

## Current branch: split-converter

## Spec status: 203 passing / 59 failing (spec.db)

### Session 2026-05-11 — completed

**DDs fully implemented (all gates pass):**
- DD#55 (22/22): suppress prefix, $? scoping, error model
- DD#58 (8/8): int function capture via sexpr in $(( ))
- DD#31 (19/19): disjoint arithmetic contexts
- DD#22 (7/7): word splitting elimination, read -f, pipeline builtins

**DD#24 taint audit:**
- Clean list eliminated (was dead code)
- Rejected vars rule: SB5017/SB5018
- $LINENO → compile-time literal
- $(getppid), $(getcwd) keywords implemented
- sash_random removed from runtime
- Converter: $HOME → $(getenv HOME) + auto-bless

**Structural:**
- main.rs → thin wrapper, all logic in lib.rs
- pub(crate) on internal modules (dead code detection)
- 10 dead items removed (zero warnings)
- Pipeline inline builtins (echo, true, false, :)
- Mapfile+pipe fix (subscript in test expressions)

### Test suite (no regressions)
- e2e: 136 pass / 2 fail (brush compat)
- oracle: 59 / 0
- taint: 42 / 0
- codegen: 219 pass / 47 fail (pre-existing pipeline/cmdsub)

### Next priorities

1. DD#56 EXIT trap bug + END block redesign (15 gates)
2. DD#30 type separation (10 gates)
3. DD#33 array/map type enforcement (6 gates)
4. DD#18 local function scoping (4 gates)
5. DD#42 builtins (3 gates)
