title: DD#14 — Composable Process Primitives
status: implemented

Fork, exec, wait, pipe as building blocks. All higher-level constructs
(pipelines, subshells, command substitution) composed from these four
primitives.

## The `exec` builtin (amended 2026-08-28, DG)

`exec` is a native builtin with two forms — it has no replacement, so sash
supports both:

1. **fd manipulation** — `exec > file`, `exec 2>&1`, `exec 3< file`: apply the
   redirects to the **running process** (no fork); they persist for the rest of
   the program. `$? = 0`.
2. **program replacement** — `exec CMD args`: `execve(2)` the current process
   with `CMD`. Control **never returns** on success — statements after it are
   unreachable, and the replace form does not reach the `END` block / DD#55
   exit checks (control leaves via execve). If execve fails, the process exits
   127.

### Target discipline (same as any command — DD#23/#24)

The replace target `CMD` (the first argument) is resolved and constrained
exactly like a command head:
- It must be a **literal**, compile-time-resolvable **external** path (an
  absolute path or a PATH-resolvable name). A **variable/expansion** target is
  a runtime-chosen command → **SB5011** (the security rule: `exec` must not
  replace the process with an unresolved target). An unresolvable name →
  SB5011 (not found).
- A bare name is resolved to its external absolute path (DD#23), bypassing the
  `echo`/`:` inline shortcut — `exec echo` runs `/bin/echo`, the external.
- The exec'd program's `args` follow normal **taint** rules (DD#24): a tainted
  arg without `bless` is SB5020, like any command.
- The resolved target is recorded in the `--manifest` external-dependency set.

### Implementation

Front-end: `validate.rs` (`exec` in `BUILTINS` + `check_exec_target`),
`transform/cmd_handlers.rs` (`resolve_exec_target_arg` rewrites the bare target
to its external path). Codegen: `codegen/dispatch.rs` `emit_exec` →
`emit_exec_redirects_only` (fd form) / `emit_exec_static_argv` →
`build_argv_on_stack` → `finalize_exec` (replace form, calls the no-return
`sash_exec`). Specs: `spec/exec.sh` (`ex.1`–`ex.11`).

**Native-only for now:** the c and janet backends do not yet emit `exec` (c
emits a no-op TODO; janet errors). See task `c-janet-exec-support`.
