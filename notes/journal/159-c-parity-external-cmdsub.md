# 159 — c parity (batch 2): external `$(cmd)` capture

External command substitution was entirely unimplemented in the c backend —
`emit_cmdsub` emitted `/* TODO cmdsub exec (Phase 4) */` and returned the
empty string for any non-intrinsic `$(...)`. That failed 55.7/55.8 (a failed
cmdsub must abort) and 55.9 (capture the output).

## Implementation

- **Runtime:** added `sash_capture_status(argv, envp, &len, &status)` to
  `sash-rt` — a copy of the existing `sash_capture` (fork / dup2 stdout to a
  pipe / exec / read 64KB / wait / strip trailing newline) that additionally
  writes the child's exit status. `sash_capture` discarded it, but the c
  backend needs it for the DD#55 abort.
- **emit_c:** `emit_cmdsub`'s external path now calls `emit_cmdsub_exec_c`,
  which builds the argv (reusing `emit_argv`, so `@a` splats work), calls
  `sash_capture_status`, sets `$?`/`__test_result`, and `sash_exit`s on a
  nonzero status — mirroring native `emit_cmdsub_exec`, which aborts
  unconditionally on a failed cmdsub. The captured `(ptr, len)` is returned.

The `-` suppression in `$(- cmd)` (55.9) is a compile-time taint concern
(the output is still tainted → must be blessed), orthogonal to the runtime
capture; `/bin/echo hello` succeeds so no abort question arises. For c the
runtime stderr is discarded by the harness (`2>/dev/null`), so the diagnostic
is cosmetic — the abort just needs a nonzero exit, which it produces.

## Result

Full c sweep: **0 regressions, +3** (`c:55.7`, `c:55.8`, `c:55.9`). The
unconditional abort matched native everywhere (no cmdsub-in-condition
divergence surfaced). Only two c failures remain: `38.5` / `55.19` (`$!`
background pid), deferred with the subshell/background work.

## Files

- `sash-rt/src/lib.rs` — `sash_capture_status`.
- `sash/src/emit_c.rs` — `sash_capture_status` extern; `emit_cmdsub` external
  path → `emit_cmdsub_exec_c`.

Note: `sash-rt` is a **separate cargo workspace** — build it with
`(cd sash-rt && cargo build --release)`, not `cargo build -p sash-rt` from
the repo root (that fails: "package ID `sash-rt` did not match any packages").
