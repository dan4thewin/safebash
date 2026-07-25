---
title: "DD#55 (A)-channel abort — remaining: pipefail, c cmdsub, descriptive messages, janet pipeline"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "pipefail-on-non-last aborts in c and janet (21.3 `/bin/false | echo ok`, 55.10 `/bin/false | /bin/true`), WITHOUT breaking per-segment `-` suppression (21.2 `- /bin/false | echo ok`, 55.11 `- /bin/false | /bin/true`)"
  - "c cmdsub abort: `x=$(/bin/false)` aborts, `x=$(- cmd)` does not (55.7, 55.8, 55.9)"
  - "cd / echo-write(EPIPE/ENOSPC) / redirect-open failures abort with the native descriptive message in c (55.20-29 un-guarded to native,c)"
  - "janet pipeline abort (bare failing pipeline aborts)"
  - "[RESOLVED, DG 2026-07-04] janet abort diagnostics use janet's OWN argv[0] (`(dyn :args) 0`) — that's faithful (`<argv0>: <reason>`); janet is a script, not a `test_bin`. The message-exact specs must NOT hardcode `test_bin`: assert the message SUFFIX (the `: <detail>` after the program name, which is backend-independent) or derive the expected progname per mode. Then un-guard 55.20-29 to all three modes."
---

# DD#55 (A) abort — remaining work

The **external-command** bare abort landed in both backends (commit
707832f): a bare `/bin/false` aborts with its status, honoring `-`
suppression / if / || catching (via the SSA `checked` flag in c, a
structural `EmitCtx.checked` in janet), routing through the END block
(DD#56). Spec 55.1 green in all three modes. This task tracks the rest.

## 1. Pipefail + per-segment `-` suppression (c and janet)

`/bin/false | echo ok` must abort (pipefail: a non-last stage failed),
but `- /bin/false | echo ok` must NOT (the `-` suppresses that segment's
pipefail contribution). c's emit_pipeline currently takes status =
LAST-stage only; switching to last-nonzero (pipefail) fixes 21.3/55.10 but
breaks 21.2/55.11 because the stage tuples
(`&[(String, Vec<SsaValue>, Vec<Redirect>)]`) carry **no per-stage suppress
flag**. Native tracks per-segment suppression (see the SSA Pipeline). The
fix needs that per-stage suppress info threaded into both emit_pipeline
paths, then pipefail = last **non-suppressed** nonzero stage.

## 2. c cmdsub abort (55.7, 55.8, 55.9) — GATED on implementing c external cmdsub

`x=$(/bin/false)` must abort (checked cmdsub); `x=$(- /bin/echo hello)`
must not. **Prerequisite discovered 2026-07-04**: emit_c's `emit_cmdsub`
does not run external commands at all — it emits
`/* TODO cmdsub exec: <name> (Phase 4) */` and returns `""` (verified:
`x=$(/bin/echo hi)` → empty in c). So external `$(cmd)` must first be
implemented in c (fork/exec, capture stdout, set the var), THEN add the
DD#55 status check/abort. `SsaValue::CmdSubst` carries no `checked` flag
today — it would need one (like Command/Pipeline) or the abort must be
derived at the assignment site. janet already passes 55.7/55.8 (os/spawn
failure propagates) but fails 55.9 (the `-`-in-cmdsub suppress).

## 3. Descriptive-message aborts (cd / echo-write / redirect-open) — c

55.20-29 (guarded `in_modes native`) assert exact stderr:
`test_bin: cd: <dir>: <errno>`, `test_bin: echo: write error: <errno>`,
`test_bin: <file>: <errno>`. c links libsash_rt so it can call sash_emsg
with the same segments — but `cd` is currently a TODO in emit_c and
echo-write-failure / redirect-open-failure aren't surfaced. Implement
those + the abort message, then un-guard 55.20-29 to `native,c`.

## 4. janet pipeline abort + the diagnostics decision (RESOLVED)

janet pipelines don't abort yet. And the descriptive-message aborts: DG
ruled (2026-07-04) that **janet uses its own argv[0]** — `<argv0>:
<reason>` with janet's actual program name (`(dyn :args) 0`, the script
path) is the faithful form; janet is a script, not a `test_bin`. The
divergence is a harness artifact, not a janet defect. So the message-exact
specs (55.20-29) must not hardcode `test_bin`: assert the message SUFFIX
(the `: <detail>` after the program name — backend-independent) or derive
the expected progname per mode, then un-guard them to all three modes.
janet's abort should therefore print `<argv0>: <reason>` to stderr via
eprin before `(os/exit rc)`.

## References
- native (A)-channel: emit_status_abort_check / emit_pipeline_abort_check
  (sash/src/codegen/inst_handlers.rs); AbortMsg + diag_static_seg.
- c: emit_exec/emit_pipeline/abort_target (sash/src/emit_c.rs).
- janet: emit_janet_general_command + EmitCtx.checked (sash/src/emit_janet.rs).

## Pipefail + per-segment suppression: DONE (2026-07-24, commit 845f494)

The pipefail/per-segment-`-` half is resolved as a full DD#55 semantics pass
(ratified with DG). A pipeline's aggregate is now context-independent (worst
over non-suppressed segments); position decides only abort-vs-branch.
Represented via a per-segment `suppressed` mask on the SSA `Pipeline`
(native/emit_c) and from the AST (emit_janet). Fixed 20.6/21.3/55.10/55.11 in
c AND janet, plus pipefail-in-conditions, leading-`-` later-stage abort, and
non-first-`-` stage-drop. `pipeline && C`/`pipeline || C` now rejected
(SB4023). Specs errors.sh 55.32-40. See DD55-error-model.md + journal.

**Still remaining under this task:** c external `$(cmd)` cmdsub abort
(55.7/8/9 — emit_cmdsub is still a TODO for externals), descriptive-message
aborts (cd/echo-write/redirect-open, 55.20-29 native-only), and `$!`/getcpid
(38.5, 55.19). These are independent of the pipefail work.
