---
title: "janet backend: predicate/function gaps (if <userfunc>, ! in condition, return propagation)"
status: deferred
priority: P3
kind: bug
depends: []
effort: effort-janet-backend-parity
gates:
  - "`! f` (and `! <cmd>`/`! [[ ]]`) in an if/while condition negates correctly in janet (today it lowers to `true` — emit_condition_inline `_ => \"true\"`); un-skip functions.sh 74.3"
  - "pre-existing janet function-spec failures pass or are separately tasked: 17.1 ($(func) capture), 18.1-18.3, 42.1, 48.2"
  - "0 native/c regressions"
---

# janet predicate/function gaps

## Context

DD#74 (predicate functions) Phase 1 fixed janet's plain `return N` (now sets
`__last_exit` and is wrapped in a `_ret` label) and `if <userfunc>`/`while
<userfunc>` (now calls the janet function and checks `__last_exit` instead of
`os/execute`-ing the name). That took janet functions.sh from 12 failures to 7.

Remaining janet gaps (all pre-existing, surfaced while doing DD#74):

- **`!` in a condition lowers to `true`.** `emit_condition_inline`
  (`sash/src/emit_janet.rs`) has a `_ => write!(out, "true")` fallthrough, so
  `if ! f` / `if ! [[ … ]]` / `if ! cmd` don't negate. functions.sh **74.3**
  is guarded to native,c pending this. Fix: recognize the `!`/negate form and
  emit `(not …)` around the inner condition (or honor the
  `__negate_test_result` sigil).
- **17.1** `x=$(func); …` — janet `$(func)` stdout capture of a user function.
- **18.1-18.3, 42.1, 48.2** — other janet function/positional specs failing at
  HEAD (0a6d690). Audit and fix or split into their own tasks.

native and c pass all of these.
