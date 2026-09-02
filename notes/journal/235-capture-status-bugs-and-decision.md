---
title: "if ! out=$(cmd): decision filed; two capture/status bugs fixed"
date: 2026-09-02
---

# `if ! out=$(cmd)` — decision task + two bug fixes

DG asked whether SB4025's "sequence it" advice actually works for
`if ! out=$(cmd) { echo bang }`. It doesn't — and investigating surfaced a
design gap (no way to capture stdout AND branch on a command's status), a
DD#55 ↔ DD#70/#71 conflict, and two concrete bugs. DG: "capture this as a
decision task; then address only the existing bugs, but not new features"
(and: "I consider the bad sequencing advice a bug too").

## Decision task (feature — deferred)

Filed `notes/tasks/multi-assignment-and-capture-trio.md`: DG's direction to
"support multiple assignment in general via return statement; a capture of an
external command could assign each of stdout, status, and stderr." Captures the
axes (binding syntax, arity/typing, taint split, trio order, backends, DD#74
reconciliation, staging) and the crux rule (**binding the status toggles catch
vs. abort**). No implementation until DG settles the axes. The task also owns
the resolution of the SB4025 ↔ DD#55 conflict (whether `$(cmd)`-RHS assignments
become catchable, making `if ! out=$(cmd)` / `out=$(cmd) || h` work).

## Bug 1 — SB4025's "sequence it" advice is wrong for a capture

`nonstatus_in_condition_diag` (validate.rs) always suggested "sequence it
instead (e.g. `x=foo; if …`)". Correct for a literal (`x=hi && echo`), but for
a capture (`if ! out=$(cmd)`) sequencing can't reproduce capture+branch — a
failed `$(cmd)` aborts before any follow-up check. Made the diagnostic
RHS-aware (`value_is_capture` walks the value for a `CommandSubstitution`):

- capture RHS → honest advice: "…run it in the condition without capturing:
  `if ! cmd { … }`. Capturing output *and* branching on that command's status
  isn't currently supported."
- literal RHS → keeps "sequence it".

Same SB4025 code; both assignment and declaration call sites pass the flag.
specs errors.sh e.br8 (capture → honest), e.br9 (literal → "sequence it").

## Bug 2 — `$(- cmd)` capture aborted on a failing command

DD#55 says `x=$(- cmd)` suppresses the exit check (status discarded, no abort),
but `x=$(- /bin/false)` **aborted** — spec 55.9 only ever tested a succeeding
`$(- /bin/echo)`, so the failing path was never exercised. Root cause: the
single-command capture fast-path (`handle_command_substitution` →
`SsaValue::CmdSubst`) dropped the `-` suppression — `single_command_subst`
peels the `Suppressed` wrapper to reach the command, and `CmdSubst` carried no
suppression flag, so codegen emitted the unconditional DD#55 abort.

Fix: `CmdSubst` gains `suppressed: bool`; `handle_command_substitution` detects
the peeled `Suppressed` (`subst_is_suppressed`) and sets it; native codegen
(`emit_cmdsub_exec`) and the c backend (`emit_cmdsub_exec_c`) skip the abort
when suppressed (status still stored/`$?` still rejected downstream, stdout
still captured). The three intrinsic `CmdSubst` sites (getppid/argc/…,
epochseconds, argc-loop) pass `suppressed: false`. janet's `$(- cmd)` capture
is a separate pre-existing TODO (render_capture returns a stub), untouched.

Behavior:
```
x=$(- /bin/false)  -> "" , continues (no abort)   # was: abort "command failed"
x=$(- /bin/echo hi) -> "hi"                        # unchanged
x=$(/bin/false)    -> aborts                        # unsuppressed, unchanged
```

spec errors.sh 55.9b (the failing path 55.9 missed), native,c.

## Verification

`emit_cmdsub_exec` tripped the complexity gate after the suppression guard
(cog 6→7); extracted `emit_cmdsub_abort_check` (cog 4 / 1). fmt/clippy clean.
3-mode sweep vs HEAD: native 0 regressions, c 0, janet 0; +1 native / +1 c
(55.9b), e.br8/e.br9 pass all modes.

## Files

- `notes/tasks/multi-assignment-and-capture-trio.md` — the decision (filed).
- `sash/src/validate.rs` — `nonstatus_in_condition_diag` RHS-aware +
  `value_is_capture`.
- `sash/src/ssa.rs` — `CmdSubst.suppressed`.
- `sash/src/transform/expr_handlers.rs` — `subst_is_suppressed`, set on capture.
- `sash/src/transform/{pipelines,expressions}.rs` — intrinsic sites `false`.
- `sash/src/codegen/{values,value_handlers}.rs` — thread + gate abort;
  `emit_cmdsub_abort_check`.
- `sash/src/emit_c.rs` — thread + skip abort line when suppressed.
- `spec/errors.sh` — 55.9b, e.br8, e.br9.
