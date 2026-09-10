---
title: "DD#55 abort diagnostics — remaining abort classes (external / pipeline / cmdsub / bg-exec / overflow)"
status: done
priority: P2
kind: feature
depends: []
gates:
  - "every (A)-channel abort prints `<argv0>: …` diagnostic to stderr (DG: descriptive messages for ALL abort cases)"
  - "external nonzero: `<argv0>: <name>: command failed (exit N)` — uncaught only"
  - "pipeline aggregate failure: names the failing stage + exit N"
  - "command-substitution failure: same scheme"
  - "background-exec failure (fork/exec): `<argv0>: <name>: <reason>` (errno)"
  - "arithmetic overflow (DD#25): `<argv0>: arithmetic overflow`"
  - "regerror9 reprefixed from `sash:` to argv[0] for consistency"
  - "diagnostics fire only on the actual abort — caught (if/||) and `-`-suppressed stay silent"
  - "existing cross-mode abort specs (55.1, 55.8, 55.10, 55.16, 25.x) assert the message in `rerr` (native), guarded native-only until C/janet parity lands"
---

## Context

> **DONE (2026-07-07, autopilot).** Native abort diagnostics now fire for
> every class: external, cmdsub, pipeline (names the failing stage) via
> `sash_emsg_status` (`<argv0>: <name>: command failed (exit N)`); cd /
> redirect / echo-write (W3, errno) already; overflow already; regerror9
> reprefixed to argv0. Specs 55.1/55.8/55.10/55.16 assert the native
> message. Commits df807d3, a050115, 2eaf967; journal 132.
>
> Two follow-ons, both already tracked: (1) bg-exec currently uses the
> status form, not the errno form — the errno-precision refinement is a
> design call filed as `bg-exec-errno-message`; (2) un-guarding the specs
> to c/janet is the separate `c-janet-dd55-abort-channel` /
> `dd55-abort-channel-remaining` parity work.

W3 (commit 313843a) shipped the diagnostics facility (`sash_emsg`,
`sash_last_errno`, errno→strerror table) and wired it for the **syscall
aborts**: cd (chdir), redirect open(2), echo write(2). DG ratified the
scheme `<argv0>: <context>: <reason>` with the program name (argv[0]) as
prefix, and chose "W3 first, the other abort classes in their own pass."

**Update 2026-06-26:** the **overflow gate is DONE** — but via
`bug-runtime-arith-overflow-wraps` (it was a P0 silent-wrap bug, not just
a missing message). `sash_arith_overflow` prints `<argv0>: arithmetic
overflow`. The 5 status/errno gates below (external / cmdsub / pipeline /
bg-exec / regerror9) remain open.

This task is that pass: the **status-based** aborts (no errno) and the
regcomp case.

## Notes

- Status-based messages use a `sash_die(msg_ptr, msg_len, status)`-style
  helper (write msg + exit) or extend `sash_emsg` — the command name is a
  dynamic segment; "command failed (exit N)" needs the status formatted.
- The abort sites are `emit_status_abort_check` (external/cmdsub) and
  `emit_pipeline_abort_check` (pipeline) in codegen/inst_handlers.rs, and
  the DD#25 overflow abort. Pass the command name / status as message
  segments, mirroring the cd/echo wiring.
- `emit_status_abort_check` already takes an optional `AbortMsg`; the
  external caller (handle_general_command) currently passes `None` — give
  it the `<name>: command failed (exit N)` message.
- The C and janet backends lack the whole (A)-channel abort
  (see `notes/tasks/` C/janet parity) — keep these message specs native
  only (in_modes native) until that lands.
