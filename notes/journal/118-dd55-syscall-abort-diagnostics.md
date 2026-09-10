---
title: "DD#55 W3 — syscall aborts (cd/echo/redirect) + descriptive stderr diagnostics"
date: 2026-06-25
---

## What shipped (native)

Worked the DD#55 (A)-channel for the **internal syscall-wrapping
builtins**, spec-first, with DG steering the model over many turns.

### The error model (DG-ratified)

- **(A) runtime abort-if-unchecked** — genuine failures the runtime
  observes: syscalls we issue (chdir/open/write), external child status,
  dynamic regcomp. `-` suppresses a command's own *result-verdict*.
- **(B) compile-time must-consume** — pure in-process logical results
  (`match`, `[[ ]]`, `-> int` returns). Discard is a compile error
  (SB4016 already implements this). No suppression; use `|| :`.

### Bugs fixed (commits c232b75, 313843a)

1. **cd** never set `test_result` and never aborted — `if cd /tmp` took
   the else branch, and a failed `cd` silently continued. Now threads the
   real chdir status to both `$?` and the catching positions, and aborts.
2. **echo** hardcoded `test_result=1`/`$?=0` ("echo always succeeds") —
   a failed `write(2)` (ENOSPC/EPIPE) was invisible. `lower_echo` now
   folds each write's failure and returns the status; echo aborts.
3. **redirect open(2)** silently **misrouted**: `echo foo > /bad`
   discarded the failed open and left fd 1 as stdout, leaking "foo" there
   and continuing rc=0. Now: a failed open makes the command fail without
   writing (body skipped, no misroute). Fixed for echo *and* external
   commands.

### The redirect model (the key design dialogue)

DG's framing: `echo foo > /bad` is `exec 4>/bad; echo foo >&4`. The open
is a checked syscall that runs FIRST. Its abort is governed by **context**
— it aborts unless it sits in a condition (if/||), where the failed
final-command result decides the branch. `-` binds to the command, not
the prior open, so `- echo > /bad` still **aborts**. Crucially this needs
NO new per-instruction IR bit: `in_condition` is derived at the emit site
from the `Branch` terminator (mirroring transform's
`mark_unchecked_in_conditions`), kept distinct from the `-`-polluted
`checked`. (An earlier `suppressed`-bit / M1 detour was reverted after DG
clarified.)

### Descriptive diagnostics (DG: "for ALL abort cases")

Every abort prints `<argv0>: <context>: <reason>` to stderr, prefixed
with the program name (argv[0], DG's call over `sash:`). `<reason>` is the
OS strerror via a hand-rolled errno table (sash-rt is no_std/no-libc):
- runtime `sash_last_errno` (set by chdir/open/write from
  `Errno::raw_os_error`), `errno_str` table, `sash_emsg(seg1, seg2)`.
- codegen: `ExternFuncs.emsg`, `AbortMsg` + `diag_static_seg`/
  `diag_empty_seg`, `emit_status_abort_check` gains an optional message.
- Fires ONLY on the actual abort — caught/`-`-suppressed stay silent
  (the emsg call lives in the abort block).

This pass covers cd / redirect / echo (the syscall/errno cases). The
status-based classes (external/pipeline/cmdsub/bg/overflow) are filed in
`abort-diagnostics-remaining.md`.

## Implementation notes

- `emit_redirects_guarded` surfaces the open failure + first path;
  `emit_redirect_guarded_body` (shared by echo + handle_general_command)
  emits the fail-path (skip body, fail status, abort-or-catch with the
  `<argv0>: <path>: <reason>` diagnostic) and the success-path body.
- `in_condition` added to `InstCtx`, computed once in `emit_block_insts`.
- Extracted `emit_signal_exit_check` (flattened with early returns) and
  `emit_echo_body` to keep the wrappers under the cognitive gate.
- Harness: native runner captures runtime stderr into a new `rerr` var
  (`err` stays compile-only); abort specs assert `rerr`, caught/suppressed
  assert `-z $rerr`.

## Spec

`spec/errors.sh` 55.20-30 (native only — C/janet lack the whole
(A)-channel, see `c-janet-dd55-abort-channel.md`): cd / echo-write /
redirect-open aborts with diagnostics, if/|| catching, `-` semantics, and
no-misroute. Native 0 failing; c=58 / janet=106 unchanged.

## Deferred

- `abort-diagnostics-remaining.md` — external/pipeline/cmdsub/bg/overflow
  diagnostics + regerror9 argv0 reprefix.
- `c-janet-dd55-abort-channel.md` — bring the C/janet oracles to (A)-channel
  parity, then un-guard 55.20-30 cross-mode.
- DD write-up of the (A)/(B) error-model refinement (mentioned to DG).
- DD#63 regex: `match` boolean form (1b-bool) + captures/`\xnn`/sugar.
