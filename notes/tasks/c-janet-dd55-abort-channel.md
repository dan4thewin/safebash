---
title: "C and janet backends lack the DD#55 (A)-channel implicit-exit abort"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "bare `/bin/false` aborts (nonzero exit) in C and janet, matching native"
  - "the DD#55 implicit exit check fires for external commands, pipelines, cmdsub, cd, echo, redirect-open in all three backends"
  - "caught (if/||) and `-`-suppressed failures do NOT abort, in all three backends"
  - "the W3 native-only specs (errors.sh 55.20-30) are un-guarded to run cross-mode once C/janet reach parity"
---

## Context

Discovered during W3 (DD#55 syscall-abort work, commit f9d833b): the
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
