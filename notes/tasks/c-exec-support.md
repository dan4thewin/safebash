---
title: "c backend: emit the `exec` builtin (currently native-only)"
status: ready
priority: P2
kind: feature
depends: []
gates:
  - "c backend: `exec /bin/echo hi` replaces the process (execve) like native — no silent `/* TODO command: exec (Exec) */` no-op"
  - "c backend: `exec > file` applies the fd redirect to the running process"
  - "c/janet backend: `close N` closes the fd (native works; c no-ops the TODO, janet mis-emits it as an external `os/execute [\"close\"]`)"
  - "janet backend: either implement exec (if the runtime allows execve/dup2) or emit a clean compile-time 'exec unsupported in janet' rather than a runtime error"
  - "spec/exec.sh un-guarded from native-only for whichever backends gain support"
---

## Context

DD#14 `exec` (process replacement + fd manipulation) was implemented native
this session (journal 201) and is native-only:
- **c** (`emit_c.rs`): the general command lowering hits `_ => /* TODO command:
  exec (Exec) */`, a silent no-op — `exec /bin/echo hi` compiles but does
  nothing (no execve, no redirect).
- **janet** (`emit_janet.rs`): no Exec handler → tries to run `exec` as an
  external command → runtime error.

`spec/exec.sh` is guarded `native`-only because of this.

## Work

- **c**: add a `BuiltinCmd::Exec` arm to `emit_c`'s command lowering mirroring
  native `emit_exec`: `emit_exec_redirects_only` (apply redirects to the
  process, `$?=0`) and the replace form (build argv, call the runtime
  `sash_exec` — already declared `extern … __attribute__((noreturn))` in the
  emitted C, so the runtime symbol exists). The target is already resolved to an
  absolute path by the transform (`resolve_exec_target_arg`), so emit_c just
  lowers args[0] as-is.
- **janet**: janet may not support execve/dup2 cleanly. Decide with DG: implement
  via janet's `os/execve`-equivalent, or emit a compile-time unsupported error
  (better than the current runtime failure).

## Notes

The transform + validation are backend-independent (already done): the c/janet
work is purely the emit step. Native `emit_exec` in `codegen/dispatch.rs` is the
reference.


## Rescoped to c-only (DD#104, 2026-09-12)

The janet backend is removed. This task was filed against both
backends; the **c** half is still real and still open — it shows up
as one of the c-mode skips. The janet half is gone, not fixed.
