---
title: "c/janet: `echo | read` lastpipe inline-pipe (read consumes pipe + sets parent var)"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "`echo \"clean\" | read x; echo $x` yields `clean` (rc 0, no stderr) in c and janet"
  - "the c invalid-pipe-fd crash (`assertion failed: fd != u32::MAX as RawFd`) is fixed"
  - "un-guard wordsplit 22.12 back to all modes"
---

# c/janet: `echo | read` inline-pipe unsupported

Split from `cjanet-wordsplit-39-10-22-12` (the `\w` half was fixed in
2b06810; this `echo|read` half is a distinct, larger feature).

`echo "clean" | read x; echo $x` → `clean` in native. In a pipeline whose
last stage is the `read` builtin, `read` must consume the previous stage's
stdout and set `x` in the **parent** program (lastpipe semantics — the
read stage does not fork).

## c

Emits/links the pipeline but **crashes at runtime**:

```
out=[clean] rc=99 rterr=[assertion failed: fd != u32::MAX as RawFd]
```

An invalid (-1) fd reaches a rustix call in the pipeline read-path. The
value is even produced ("clean" prints) before the crash — a real runtime
bug in emit_c's `read`-as-pipeline-stage / lastpipe fd wiring.

## janet

`emit_pipeline` os/spawns every stage, so `read x` becomes
`(os/spawn ["read" "x"] …)` → `"read": No such file or directory`, and the
following `echo $x` reads `x` from the environment (empty). janet needs to
special-case a builtin last stage: when the last pipeline stage is `read
VAR…`, read the upstream pipe output and bind the janet variable(s)
instead of spawning.

## Fix direction

- native reference: the lastpipe path (read builtin consuming the pipe in
  the current process). Check `codegen` pipeline lowering for the
  `read`-last-stage special case.
- c: fix the invalid-fd wiring so the last-stage `read` reads the pipe in
  the parent (no fork), sets the var, then un-guard.
- janet: detect a `read` (or other builtin) last stage in `emit_pipeline`
  and emit an in-process read + variable bind rather than os/spawn.
- Then un-guard `wordsplit.sh` 22.12 (currently `in_modes native`).

## RESOLVED (2026-07-14, commit 2f183c2)

Implemented for c and janet, and extended beyond the single-`read` gate to
the whole read/mapfile lastpipe family (22.4-22.10, 22.12).

- **c** (`emit_c`): `emit_pipeline` now honors `capture_read_fd` (extra
  last-stage→parent pipe, read end stashed in `__pipe_read_fd`), fixing the
  fd=-1 (u32::MAX) rustix crash. Added `ReadLine`(fd=-1)/`ReadLineMulti`/
  `Mapfile`/`ReadArray` emit arms (were TODO no-ops) reusing the runtime
  split/read primitives.
- **janet** (`emit_janet`): `emit_pipeline` detects a `read`/`mapfile` last
  stage and binds vars in-process from the captured producer output. Also
  fixed a coupled pre-existing bug: bare `array a;`/`map m;` emitted `""`
  and didn't register the map type (broke `${m[key]}`); now emit `@[]`/`@{}`
  from the `-a`/`-A` decl flag.
- Un-guarded wordsplit 22.12 to all modes.

native 0 (unchanged); c 47→38, janet 70→61; 0 regressions.
