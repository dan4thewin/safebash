# 195 — DD#32 trace coverage (trace.sh)

Continuing the coverage grind. The fully-uncovered-function scan flagged
`codegen/entry.rs::emit_branch_outcome_trace` (48 lines) and
`codegen/setup.rs::intern_trace_terminator` (20 lines) — DD#32 branch/loop
runtime tracing. Both were reachable but no spec compiled with `--trace`.

## Mechanism

- `--trace` (compile flag, lib.rs:445 → `compile_to_object(source, true)`)
  makes native codegen emit the trace-write code and intern the per-terminator
  `+ <kind> → <label>` literals.
- At runtime, `SASH_TRACE=1` (checked by sash-rt at startup) sets the global
  `sash_trace_flag`; each branch/loop terminator then writes its message to
  stderr, choosing the taken vs not-taken label via a `select`.

`emit_branch_outcome_trace` early-returns unless the trace literals are interned
(compile-time `--trace`), which is why no ordinary spec reached it.

## Spec

New `spec/trace.sh` (native-only — `--trace` is a no-op in the c/janet
emitters, so it exits for those modes like convert.sh). Sets
`SASH_CFLAGS=--trace` + `SASH_TRACE=1` and asserts the runtime stderr
(`$rerr`):
- tr.1 if-then → `+ branch → if.then` (true target)
- tr.2 if-else → `+ branch → if.else` (false target)
- tr.3 while → `+ loop → while.body` (iterations) + `+ loop → while.exit`

Covers both terminator kinds (`branch`, `loop`) and both select targets. +3
native; 0 regressions. The harness already supported this: `SASH_CFLAGS`
injects the compile flag and `_run_sash_native` runs the binary in the current
env (so `SASH_TRACE=1` reaches it), capturing runtime stderr as `$rerr`.
