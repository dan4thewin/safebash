---
title: "advance DD#23: hoist command resolution ahead of the front-end; unresolved = fatal; --sysroot incl. squashfs"
status: ready
priority: P2
kind: feature
depends: []
gates:
  - "every external command is resolved to an absolute path BEFORE the front-end checks (validate/taint/type-check) run — the taint pass sees /usr/bin/cat, not bare cat"
  - "an unresolved command (not a builtin, not a user function, not found in the resolution root) is a FATAL compile error (not a runtime 'bare command passed to sash_exec')"
  - "resolution honors --sysroot=<dir> AND --sysroot=<image.squashfs> (read the squashfs to resolve); build host is the default root"
  - "the 'bare command passed to sash_exec' runtime error becomes unreachable (it was a symptom of late resolution)"
---

# Advance DD#23 — compile-time resolution ahead of the front-end

DD#23 (status: partial): "External commands are resolved to absolute
paths at compile time… `--sysroot=/mnt/target`… `--sysroot=image.squashfs`
… Unresolved commands are fatal compile errors."

Today resolution runs in the **transform** (`resolve_pipeline_command` in
`build_argv`/exec paths), which is *after* the front-end taint/validation
pass. So the front-end only ever sees unresolved names — the root cause of
the cmdsub taint mis-classification (see `tighten-cmdsub-taint-execve`).

## Work
- **Hoist resolution to a front-end pass** that runs before
  validate/taint/type-check, replacing each external command head with its
  absolute path in the IR (builtins and user-function calls are left
  as-is; intrinsics getenv/argv/arg untouched).
- **Unresolved → fatal compile error** (new SB code), not a runtime
  failure. Retires the whole "bare command passed to sash_exec" class.
- **`--sysroot` incl. squashfs**: resolution searches the build host by
  default, or a `--sysroot=<dir>` / `--sysroot=<image.squashfs>` root.
  `resolve_command_path` already takes a sysroot for the dir case; add the
  squashfs reader for the image case.

## Enables
- `tighten-cmdsub-taint-execve` (the taint pass then classifies on the
  resolved/exec form).
- Retires `bug-cmdsubst-builtin-not-resolved`'s remaining sharp edges.
