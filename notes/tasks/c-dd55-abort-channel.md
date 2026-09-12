---
title: "C backend lacks the DD#55 (A)-channel implicit-exit abort"
status: ready
scope: c/janet backends only (native complete)
priority: P2
kind: bug
depends: []
gates:
  - "[DONE] bare `/bin/false` aborts (nonzero exit) in C and janet, matching native (spec 55.1 green all modes)"
  - "[DONE] the implicit exit check fires for bare EXTERNAL commands + honors `-` suppress / if / || catch, routing through END (DD#56)"
  - "[SPLIT OFF -> dd55-abort-channel-remaining] pipefail-on-non-last (21.3/55.10), c cmdsub abort (55.7-9), cd/echo-write/redirect descriptive-message parity (55.20-29), janet pipeline abort, and the janet-diagnostic DECISION"
---

# PARTIAL 2026-07-03 (external abort landed; see dd55-abort-channel-remaining)

Commit bc65140: a bare (unchecked) external command that exits nonzero
aborts with that status in both c and janet. c threads the SSA `checked`
flag through emit_command/emit_exec/emit_pipeline and `goto`s the abort
target; DD#56 routing added via `Emit.end_block`/`abort_target` (fixes a
56.3 regression). janet re-derives checked-ness structurally
(`EmitCtx.checked`: false in `-`-suppressed and &&/|| left positions) and
does `(os/exit rc)` on a checked external's nonzero exit. Suppress / if /
|| catch all correctly do NOT abort. 0 regressions.

The remaining channels are in `dd55-abort-channel-remaining`.

---


## Context

Discovered during W3 (DD#55 syscall-abort work, commit 313843a): the
native backend implements the (A)-channel implicit-exit abort
(`emit_status_abort_check` / `emit_pipeline_abort_check`), but **C and
janet do not** — a bare `/bin/false` exits 0 in both (`emit_inst` in
emit_c.rs drops the `checked` flag; the janet backend likewise). This is
a large oracle-parity gap, already reflected in the pre-existing c=58 /
janet=106 failing baselines.

Because of this gap, the W3 abort + diagnostic specs (errors.sh 55.20-30)
are `in_modes native` only. Bringing C and janet to parity is the
precondition for un-guarding them.

## Notes

- The native (A)-channel is `emit_status_abort_check`
  (codegen/inst_handlers.rs) + `emit_pipeline_abort_check`. The C/janet
  backends need the equivalent: after each checked command/pipeline, if
  status != 0, exit with that status (honoring `-` suppression and
  if/|| catching via the same `checked` flag, plus DD#56 END routing).
- The descriptive diagnostics (sash_emsg) are native today; C links the
  real `libsash_rt` so it can call `sash_emsg` too. Janet would need its
  own stderr diagnostic. Coordinate with `abort-diagnostics-remaining`.
- janet `cd` is separately wrong: `if cd /tmp` aborts (observed during
  W3). Fix alongside the abort channel.


## Rescoped to c-only (DD#104, 2026-09-12)

The janet backend is removed. This task was filed against both
backends; the **c** half is still real and still open — it shows up
as one of the c-mode skips. The janet half is gone, not fixed.
