# 201 — support the `exec` builtin (DG)

DG: `exec` has no replacement for its two use cases — **fd manipulation of the
running program** (`exec > file`) and **program replacement** (`exec cmd`) — so
sash must support both. (The other accepted-in-source bash form, `trap`, was
purged in journal 200; exec is the opposite call — a real feature to finish.)

## What was already there

The native exec codegen existed (`codegen/dispatch.rs`: `emit_exec` →
`emit_exec_redirects_only` for the fd form, `emit_exec_static_argv` →
`build_argv_on_stack` → `finalize_exec` calling the no-return `sash_exec` for
the replace form). But it was unreachable: `exec` wasn't a recognized builtin,
so `validate::check_commands` rejected it (SB5011) before dispatch. And the
codegen assumed `args[0]` was already an absolute path.

## Gaps found (by unblocking + testing)

Temporarily adding `exec` to `BUILTINS` revealed:
- `exec /bin/echo x` ✓, `exec > file` ✓ (fd form) — worked immediately.
- `exec echo x` → runtime `bug: bare command passed to sash_exec` — the target
  wasn't DD#23-resolved.
- **`exec $x hi` → silently ran** — a variable (runtime-chosen) target wasn't
  rejected. The security hole: `exec` replaces the process image, so an
  unresolved/attacker-controlled target is a code-exec vector.

## Implementation (DG: target follows DD#23/#24 like any command)

- **validate.rs**: `exec` added to `BUILTINS`; new `check_exec_target` — the
  replace target (args[0]) must be a **literal**, **external**,
  compile-time-resolvable path (absolute or PATH). A variable/expansion →
  SB5011; unresolvable → SB5011. The fd-only form (no args) is allowed.
- **transform/cmd_handlers.rs**: `resolve_exec_target_arg` rewrites a bare
  target to its external absolute path via `resolve_command_path` (bypassing the
  `echo`/`:` inline shortcut — exec runs the external), and records it in
  `external_deps` (--manifest).
- Taint is inherited free: exec's args route through the normal command SsaInst,
  so a tainted arg without `bless` is SB5020.

## Verified (native)

`exec /bin/echo x` / `exec echo x` (→ external /bin/echo) / `exec grep -c . f`
(PATH + args) / `exec > file` (fd, persists across statements) / no-return
(post-exec statements don't run) / `exec $x`→SB5011 / `exec bogus`→SB5011 /
tainted arg→SB5020 / blessed arg ✓ / target in --manifest. specs `spec/exec.sh`
ex.1-11. Native gate 0 regressions.

## Scope

**Native-only.** c emits a `/* TODO command: exec */` no-op; janet errors. Filed
`c-janet-exec-support` (P2); exec.sh is native-guarded. Transform + validation
are backend-independent, so that task is just the emit step. DD#14 amended.
