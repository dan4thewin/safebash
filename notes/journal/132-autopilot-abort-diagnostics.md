# 132 — autopilot: prioritization + abort-diagnostics-remaining

Running commentary for the unattended/autopilot session started
2026-07-07. Working the DG-ratified tier order (notes/status.md):
finish partials → compiler bugs → compiler features → c/janet parity →
convert. Skipping the 4 decision-gated (⚠) tasks.

## Prep

- Recorded the tier ordering (commit 4fc9221).
- Correction on entering execution: `dd55-abort-channel-remaining` is all
  c/janet (native landed 707832f) → moved to Tier 4; `dd67-bash-sugar-sweep`
  compiler part done, §3 is a convert canonicalize mode → Tier 6. So the
  real first native task is `abort-diagnostics-remaining`.

## Task: abort-diagnostics-remaining (Tier 1 #1, native)

Goal: every (A)-channel abort prints a `<argv0>: …` diagnostic (DG-ratified
scheme `<argv0>: <context>: <reason>`). W3 already shipped the syscall
aborts (cd/redirect/echo-write, 55.20-29) + overflow (via the wrap-bug
fix). Remaining classes: external nonzero, pipeline aggregate, cmdsub,
background-exec, regerror9 reprefix. Native-only specs until c/janet
parity (that is the separate Tier-4 work).

Key finding: external/pipeline/cmdsub messages are STATUS-based
(`<name>: command failed (exit N)`, no errno), so `sash_emsg` (errno→
strerror) doesn't fit — needs a status-formatting runtime helper.
bg-exec is errno-based (fits sash_emsg); regerror9 is a prefix change.

(Progress appended below as each class lands.)

## Class 1: external command message — DONE

`<argv0>: <name>: command failed (exit N)` on a bare external nonzero.
- Runtime: new `sash_emsg_status(name_ptr, name_len, status)` (sash-rt) —
  status-based sibling of `sash_emsg` (which is errno-based).
- Codegen: `AbortMsg` struct → enum `{ Errno{4 segs}, Status{name} }`;
  `emit_status_abort_check` matches → `sash_emsg` vs `sash_emsg_status`
  (passing the live `status`). `handle_general_command` now passes
  `Status{name}` (name is already interned by `intern_command_parts`).
  cd/echo updated to `Errno{…}`.
- Spec: 55.1 asserts the message suffix in native (`! in_modes native ||
  [[ $rerr == *"...command failed (exit 1)"* ]]`), one chk, cross-mode-safe.
- Verified: bare aborts w/ msg; `-`/`||` stay silent. 0 regressions
  native/c/janet (authoritative latest-run-per-test diff).

## Class 2+3: cmdsub + pipeline messages — DONE

- cmdsub (`x=$(/bin/false)`, 55.8): in the value_handlers abort block, emit
  `sash_emsg_status(name, status)` (name = `argv_values.first()` literal).
- pipeline (`/bin/false | /bin/true`, 55.10): `wait_for_children` now also
  tracks the failing stage's name (select alongside the worst-status
  select) and returns `(worst, name_ptr, name_len)`;
  `emit_pipeline_abort_check` emits the status message naming that stage.
  `handle_pipeline` materializes each stage name via `diag_static_seg`.
- Verified: both name the failing command + exit; `-`-suppressed stay
  silent; last-stage failure named correctly. 0 native regressions.
