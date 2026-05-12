---
title: Status
modified: 2026-05-11
---

## Current branch: split-converter

## Spec status: 236 passing / 39 failing (spec.db)

### Session 2026-05-11 — completed

**DD#42 (6/6): getenv/argv/argc builtins**
- Added `argv` to command_resolution known keywords
- `run_sash` now passes program args to compiled binary
- Out-of-range `$(argv N)` aborts (like integer overflow)
- Runtime: `sash_argv_get` with bounds check and panic message

**DD#56 (15/15): END block + signal model**
- END block preprocessor (`END {` → `fn __sash_end__ {`)
- Dedicated end_block in SSA, all exit paths goto it
- DD#55 abort blocks jump to end_block (END always fires)
- `exit N` in END overrides exit status
- Commands in END block are unchecked (cleanup must not abort)
- `signal N IGN/DFL/FLAG/EXIT` builtin
- `sash_signum` global + `sash_signal_flag` runtime handler
- `$(signum)` keyword reads signal number
- Signal EXIT check points after external commands
- Exit code: 128+signum on signal-triggered shutdown

**DD#30 (14/14): strict type separation**
- Wired type_check() into compilation pipeline (was dead code)
- SB4030: reject `int y=$x` when x is string-typed
- SB5019: reject -gt/-lt/-ge/-le/-eq/-ne in [[ ]] (use (( )))
- SB4030: reject `unset x` on scalar variables
- Exempt declarations/unset from SB5006 glob check (map [key]=val)
- Track indexed arrays in collections set for unset semantics
- `unset arr` creates new empty table (clears collection)
- Register signum/argc/status/getpid/getppid as int-returning keywords
- Fix misplaced tally in types.sh and wordsplit.sh

### Test suite (no regressions)
- e2e: 136 pass / 2 fail (brush compat)
- oracle: 58 / 1 (pipeline grep)
- taint: 42 / 0
- codegen: 218 pass / 48 fail (pre-existing pipeline/cmdsub)

### Next priorities

1. DD#33 array/map type enforcement (9 gates)
2. DD#18 local function scoping (4 gates)
3. DD#36 decimal conversion (4 gates)
4. DD#39 regex (8 gates)
5. DD#46 reject top-level positionals (3 gates)
