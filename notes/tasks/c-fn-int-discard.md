---
title: "c parity: a `-> int` function used in `|| :` (discard) fails to compile/run"
status: done
scope: c/janet backends only (native complete)
priority: P3
kind: bug
depends: []
gates:
  - "c: `fn f -> int { return 3 }; f || :; echo done` — emitted C must compile (today gcc errors 'In function main') and run → `done`"
  - "janet: the same program must run → `done` (today: a janet runtime error, `user0: <tuple>`)"
  - "un-guard spec/errors.sh e.b70.fndisc to all modes once fixed"
---

# `-> int` function in `|| :` discard: c/janet backend gap

## Symptom

```
fn f -> int { return 3 }
f || :
echo done
```

- **native**: `done` (rc 0) — correct (the `|| :` discards the `-> int`
  return; DD#70 (B) explicit-discard).
- **c** (`--emit-c` → gcc): gcc rejects the emitted C ("In function
  'main'": …). The lowering of a `-> int` function call in an `|| :`
  boolean-context position produces invalid C.
- **janet** (`--janet`): runtime error `user0: <tuple …>` — the emitted
  janet mishandles the `-> int` call in the `||` position.

## Context

Surfaced writing the DD#70 contract spec (`dd70-error-model-contract-spec`,
spec/errors.sh e.b70.fndisc). The (B) must-consume model lets a `-> int`
function's return be discarded with `|| :`; native does this, c/janet do
not. e.b70.fndisc is guarded to `native` until this is fixed.

Likely related to how a user function that returns `int` is threaded
through the `||`/`&&` short-circuit lowering in the c and janet emitters
(the value-returning call in a boolean/statement position). native routes
it through the SSA `||` path correctly; the c/janet emitters need the same.

## Scope

- `sash/src/emit_c.rs` — `-> int` call in `||`/`&&`/`|| :` position.
- `sash/src/emit_janet.rs` — same.
- Compare against the native SSA lowering (`transform` + `codegen`) which
  handles it.


## Rescoped to c-only (DD#104, 2026-09-12)

The janet backend is removed. This task was filed against both
backends; the **c** half is still real and still open — it shows up
as one of the c-mode skips. The janet half is gone, not fixed.

## Re-measured 2026-09-13 — already done

The filed repro, verbatim:

```
$ SASH_MODE=c  fn f -> int { return 3 }; f || :; echo done
rc=0  out=[done]           # identical to native
```

The emitted C compiles and runs. The janet half is void (DD#104 removed the
backend).
