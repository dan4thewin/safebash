---
title: "exec-failure errno diagnostic: distinguish a missing-binary 127 from a real exit-127"
status: done
priority: P3
kind: bug
depends: []
---

> **RESOLVED + DONE (DG, 2026-07-08).** DG's model dissolved the
> entanglement without a CLOEXEC pipe: split the diagnostic by **who owns
> the syscall** —
> - **child** (in `execve`): always reports `<argv0>: <cmd>: <errno>` at
>   the point of failure (covers fg via `sash_run`, bg, pipeline stages),
>   then exits 127. This is the *what*.
> - **parent** (fork site, `sash_run`): reports `<argv0>: <cmd>: <errno>`
>   only on a `fork(2)` failure (its own syscall).
> - **parent** DD#55 abort (`command failed (exit N)`): unchanged — already
>   gated on **uncaught** (`emit_status_abort_check`'s `abort` flag). It is
>   the *why-aborting* signal, not a duplicate.
>
> So there is no spurious double: caught exec-failure → only the child's
> errno; uncaught → child errno + the (warranted) uncaught-abort line; a
> command that RAN and exited nonzero → only `command failed (exit N)`.
>
> Implemented in sash-rt: `sash_exec` reports the exec errno (execve returns
> `Errno` directly on failure); `sash_fork` preserves the errno
> (`set_last_errno`); `sash_run`'s fork-failure arm reports it. spec/errors.sh
> 55.16 (bg errno), 55.30 (fg uncaught → both lines), 55.31 (caught → errno
> only, no command-failed). 0 regressions native/c/janet.


# exec-failure errno message (fg / bg / pipeline)

## Context

`abort-diagnostics-remaining` gave every DD#55 (A)-channel abort a
`<argv0>: …` diagnostic. A command that RAN and exited nonzero prints
`<argv0>: <name>: command failed (exit N)` (status-based). The gate for
**background exec failure** asked instead for an errno form,
`<argv0>: <name>: <reason>` (e.g. `No such file or directory`).

Today `/nonexistent &; wait` already prints the *status* form
(`command failed (exit 127)`), because the bg child runs through
`handle_general_command` → `sash_run` and the external status message
(commit f8382e2) covers it. Spec 55.16 asserts that.

## Why this needs a decision (the entanglement)

Under DD#23, external commands are resolved to absolute paths at compile
time, so a runtime exec-failure only happens for an absolute path that is
missing at runtime (cross-host / different sysroot), e.g. `/nonexistent`.
When `execve` fails the child exits 127. **At the parent, a 127
exec-failure is indistinguishable from a command that ran and genuinely
exited 127** — so the parent can only say `command failed (exit 127)`.

To print the errno reason, the CHILD (the `sash_run` / `sash_exec` execve
site) must write `<argv0>: <name>: <errno>` before `exit_group(127)`. But
then a fg / pipeline exec-failure gets BOTH the child's errno message and
the parent's `command failed (exit 127)` — a double message.

## Decision needed (DG)

How should an exec-FAILURE read, given it is status-indistinguishable from
a real exit-127 at the parent?

- **(a) Keep the status form** `command failed (exit 127)` for all
  exec-failures (current behavior; retire the errno gate). Simple,
  uniform, no double message.
- **(b) Child emits errno + parent-side suppression** of the status
  message when it can tell the child already spoke. Needs a child→parent
  signal (a shared flag, or encoding exec-failure in the wait status)
  because status 127 alone is ambiguous — a bare "suppress on 127" would
  silence a *real* exit-127 command that has no child errno message.
- **(c)** something else DG prefers.

Current behavior (status form) is functional; this is a precision
refinement, not a correctness bug.

## Refs
- sash-rt `sash_exec` / `sash_run` (execve sites); `sash_emsg` (errno-based)
- notes/journal/132-autopilot-abort-diagnostics.md (class 5)
- spec/errors.sh 55.16
