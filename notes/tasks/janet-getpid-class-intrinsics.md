---
title: "janet: implement the getpid-class intrinsics (getenv/argc/getpid/getppid/getcwd/epochseconds/random) instead of exec'ing them"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "janet `$(getpid)` / `$(getppid)` → the real pids (os/getpid; getppid via a syscall/ffi or os/proc), not `(os/spawn [\"getppid\"])`"
  - "janet `$(getenv NAME)` → the env value (os/getenv), `$(getcwd)` → os/cwd, `$(epochseconds)` → os/time, `$(argc)` → arg count"
  - "janet `$(random)` → a non-negative i64 (os/cryptorand or math/rng); janet-is-illustration, exact range not required but non-negative"
  - "un-guard the getpid-class specs (functions.sh 42.x, taint.sh getenv) to include janet where they are currently native,c only"
---

# janet getpid-class intrinsics — currently exec'd as fake commands

## Problem

`emit_expr_cmdsub_command` (emit_janet.rs) has **no intrinsic dispatch** for
the getpid-class. Every one of them is emitted as an external spawn:

```
$(getppid)     → (os/spawn ["getppid"] :p {:out :pipe})     # exec "getppid"
$(getenv HOME) → (os/spawn ["getenv" "HOME"] :p {…})        # exec "getenv"
$(random)      → (os/spawn ["random"] :p {…})               # exec "random"
```

None of these are real binaries, so at runtime janet errors
(`("getppid"): No such file or directory`). This is why the getpid-class
specs are guarded `native,c` (e.g. functions.sh 42.7-42.9) and several
janet 42.x rows sit in the ~72 janet baseline failures.

Native and c implement these as in-process intrinsics
(`value_handlers::try_native_intrinsic`, `emit_c::try_native_intrinsic_c`).
Janet needs the mirror.

## Fix

Add a janet intrinsic dispatch in `emit_expr_cmdsub_command` (before the
`os/spawn` external path) keyed on the command name:

| intrinsic       | janet form                                            |
|-----------------|-------------------------------------------------------|
| `getpid`        | `(os/getpid)` (or `(os/proc-getpid)` per janet ver.)  |
| `getppid`       | janet has no direct os/getppid — syscall/ffi, or read `/proc/self/stat` field 4, or accept a small helper |
| `getcwd`        | `(os/cwd)`                                             |
| `getenv NAME`   | `(os/getenv NAME)`                                     |
| `epochseconds`  | `(math/floor (os/time))` (or `(os/time)`)             |
| `argc`          | `(+ 1 (length _args))` — C argc incl. argv[0]          |
| `random`        | non-negative i64 from `(os/cryptorand 8)` bytes, or `(math/rng-int (math/rng (os/cryptorand 8)) …)`; janet-is-illustration so exact range is not required |
| `signum`        | the current signal number (janet signal state)        |

Per the janet-is-illustration convention (native/c are the oracles),
readability matters more than byte-fidelity — idiomatic janet is fine.

## After

Un-guard the getpid-class specs to include janet:
- functions.sh 42.1/42.2 (getenv), 42.5 (argc), 42.7-42.9 (getpid/random)
- taint.sh getenv-based DD#53 tests currently native/c

## Refs
- emit_janet.rs `emit_expr_cmdsub_command` (~1412)
- native: `value_handlers::try_native_intrinsic`; c: `emit_c::try_native_intrinsic_c`
- implement-getpid-builtins (native+c done)
