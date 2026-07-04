---
title: "c/janet: `echo | read` lastpipe inline-pipe (read consumes pipe + sets parent var)"
status: ready
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
9ba4bbe; this `echo|read` half is a distinct, larger feature).

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
