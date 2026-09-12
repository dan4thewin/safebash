---
title: "janet: capture-condition (`if x=$(cmd)`) support"
status: deferred
deferred-by: DD104
priority: P3
kind: feature
depends: []
gates:
  - "janet emits the capture-condition (DD#88): `if x=$(cmd)` / `while x=$(cmd)` and the pipeline form, binding the captured stdout and branching on the command's exit status. Blocked on janet's `$(cmd)` capture, which is a stub (`render_capture` → `(sash/capture …)`)."
---

# janet capture-condition support

DD#88 (capture-condition, `if x=$(cmd)`) is implemented for native + c. janet
is not: it emits directly from the IR (not the SSA `SetTestFromStatus` sigil),
and its single-command `$(cmd)` capture is itself a stub (`emit_janet_map.rs::
render_capture` yields `(sash/capture …)`). So the positive-behavior specs
(`spec/capture_cond.sh` 88.1–88.8) are gated `native,c`; the rejects
(SB4025/SB5050/SB5020, 88.9–88.14) already pass in janet (backend-independent
validate/taint).

## What janet needs

- A real `$(cmd)` / `$(a|b)` capture (the general capture stub gap — see also
  the janet capture TODO), producing stdout + exit status.
- The capture-condition wiring: in a condition, run the command suppressed,
  set the janet condition truth value from its status, bind the captured
  stdout to the target. janet uses an inline-condition model
  (`emit_condition_inline`) rather than native/c's `test_result` var, so the
  status→condition sync is janet-specific.

Ungate `spec/capture_cond.sh` 88.1–88.8 to all modes once done.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
