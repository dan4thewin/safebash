title: "Support the `exec` builtin: fd manipulation + program replacement"
status: done
priority: P1
kind: feature
depends: []
gates:
  - "`exec > file` / `exec 3< file` persistently redirect the running program's fds (no child; the redirect outlives the statement)"
  - "`exec cmd args` replaces the running program via execve(2) and does not return (subsequent statements are unreachable)"
  - "`exec cmd` resolves `cmd` via DD#23 compile-time path resolution (absolute path / defined function), like any external command"
  - "taint/security: `exec`-ing a tainted command path is rejected or blessed, consistent with DD#23/DD#24"
  - "specs: an exec-redirect form and an exec-replace form, native (+ c if the backend supports it); the exec cluster (emit_exec/build_argv_on_stack/finalize_exec/emit_exec_redirects_only/emit_exec_static_argv) becomes covered"

## Why (DG 2026-08-28)

DG: `exec` has **no replacement** for two legitimate use cases, and sash must
support them:
1. **manipulate the running program's file handles** — `exec > file`,
   `exec 2>&1`, `exec 3< file`: redirects applied to the *current* process that
   persist for the rest of the program (not a per-command redirect, no fork).
2. **replace the running program** — `exec cmd args`: `execve(2)` the current
   process image with `cmd`; control never returns.

## Current state

The codegen already exists in `codegen/dispatch.rs`:
- `emit_exec` — dispatches on `args.is_empty()`: redirects-only vs argv.
- `emit_exec_redirects_only` — the `exec > file` fd-manipulation path.
- `emit_exec_static_argv` → `build_argv_on_stack` → `finalize_exec` — the
  program-replacement path.
- `"exec" => BuiltinCmd::Exec` (`ssa.rs`) + the `BuiltinCmd::Exec` dispatch arm.

**Blocker:** `exec` is rejected as an unresolved command (**SB5011**) at DD#23
resolution *before* the `BuiltinCmd::Exec` dispatch is reached — so the whole
cluster is currently unreachable (0% coverage). `exec` is on no clean/intrinsic
list in `transform/resolution.rs`.

## Approach (verify with DG before building — consequential/security-sensitive)

1. Make resolution recognize `exec` as a builtin (like `signal`/`cd`/`wait`),
   so it dispatches to `emit_exec` instead of external-resolution SB5011.
2. For `exec cmd args`, resolve `cmd` through the normal DD#23 funnel (absolute
   path / function), and record it in the external-deps manifest.
3. Taint: an `exec`'d command path must be clean or blessed (DD#24) — replacing
   the process image with a tainted target is at least as dangerous as a normal
   exec. Decide reject-vs-bless with DG.
4. Verify `emit_exec_*` codegen is correct/current (it predates several
   redirect/argv changes) — it may need updating, not just unblocking.
5. Consider a DD (or amend DD#14 process-primitives) documenting `exec`'s two
   forms, no-return semantics, and the taint rule.
6. Specs: `exec > file; echo x` (x goes to file), `exec /bin/echo replaced`
   (replaces process; a trailing `echo unreached` never runs).

## Notes

- No-return semantics interact with DD#55 (the replace form never reaches the
  END block / implicit exit checks — control leaves via execve).
- `exec >file` fd-manipulation must survive to subsequent statements — confirm
  the redirect isn't auto-restored (unlike per-command redirects).
- janet backend likely can't do execve/fd-replace — scope to native (+c).
