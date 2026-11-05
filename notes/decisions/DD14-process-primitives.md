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
   unreachable, and the replace form reaches neither terminal handler nor DD#55
   exit checks (control leaves via execve). If execve fails, status 127 enters
   ERR in the main process; a forked child terminates without a parent handler
   (DD#128).

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

## The `close` builtin (amended 2026-09-04, DG)

`close N` closes a file descriptor on the running process — the sanctioned form
of bash's `exec N>&-`. The `>&`/`<&` redirect target must be a plain fd number
(SB4048); the close form `N>&-` is **rejected as syntax** and directs to
`close N` instead (the reject-only decision of 2026-08-29 that closed the
`bug-redirect-dup-target-unvalidated` silent-degrade hole — a `close` builtin is
explicit and typed, matching sash's "named builtins over terse punctuation"
direction). fd **open** (`exec N> file`) and **dup** (`N>&M`) keep their redirect
syntax; only close is a builtin (DG 2026-09-04: close-only, dup stays on
`N>&M`).

- **Surface:** `close N`. `N` must be a **literal non-negative fd number** — a
  variable/expansion or a bad arity is **SB4052** (the same no-dynamic-fd
  discipline as a `N>&M` dup target). Both operands must fit the OS descriptor
  type, **0..2147483647**; overflowing literals are rejected, never wrapped or
  replaced with another descriptor.
- **Semantics:** void; `$?` = 0 on success, 1 on failure. Per DD#55 a **bare**
  `close` on a failed `close(2)` (e.g. EBADF) **aborts** with
  `<argv0>: close: <fd>: <reason>`; a **caught** (`if`/`while`/`&&`/`||`) or
  **`-`-suppressed** close does not abort (its failure is the branch verdict).
- **Native-only for now** (like `exec`): c no-ops the TODO, janet emits it as an
  external command. Runtime is `sash_close` (already present; now records the
  errno via `SASH_LAST_ERRNO` on failure so the abort reason renders). c/janet
  wiring rides with `c-janet-exec-support`.

Impl: `ssa.rs` (`BuiltinCmd::Close`), `validate.rs` (`close` in `BUILTINS` +
`check_close_arg` → SB4052), `codegen/dispatch.rs` + `codegen/inst_handlers.rs`
(`handle_close`, modeled on `handle_cd`), `parse/parser.rs` (SB4048 message
points at `close`), `sash-rt` (`set_last_errno_raw` in `sash_close`). Specs
`spec/exec.sh` cl.1–cl.8 (native). SB4052 is a new code.
