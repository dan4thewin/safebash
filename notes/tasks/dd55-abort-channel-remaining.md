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
  - "DECISION for DG: janet abort diagnostics — the message-exact specs assert `test_bin: …`; janet's program name is the .janet script path, so it cannot match byte-for-byte. Relax janet asserts, synthesize a prog name, or keep those specs native,c-only?"
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

## 2. c cmdsub abort (55.7, 55.8, 55.9)

`x=$(/bin/false)` must abort (checked cmdsub); `x=$(- /bin/echo hello)`
must not. emit_c's emit_cmdsub (value_handlers path) doesn't emit the
status check. janet already passes 55.7/55.8 (os/spawn failure propagates)
but fails 55.9 (the `-`-in-cmdsub suppress).

## 3. Descriptive-message aborts (cd / echo-write / redirect-open) — c

55.20-29 (guarded `in_modes native`) assert exact stderr:
`test_bin: cd: <dir>: <errno>`, `test_bin: echo: write error: <errno>`,
`test_bin: <file>: <errno>`. c links libsash_rt so it can call sash_emsg
with the same segments — but `cd` is currently a TODO in emit_c and
echo-write-failure / redirect-open-failure aren't surfaced. Implement
those + the abort message, then un-guard 55.20-29 to `native,c`.

## 4. janet pipeline abort + the diagnostics DECISION

janet pipelines don't abort yet. And the message-exact specs reference
`test_bin:` (the native/c binary). Janet runs as `janet .out.janet`, so
its argv[0] is the script path — it cannot produce `test_bin: …`. DG
decision needed: relax the janet asserts (rc-only), synthesize a program
name in the emitted janet, or keep the message-exact specs native,c-only
(the janet "illuminate the IR, not a byte-oracle" ceiling suggests the
latter is acceptable).

## References
- native (A)-channel: emit_status_abort_check / emit_pipeline_abort_check
  (sash/src/codegen/inst_handlers.rs); AbortMsg + diag_static_seg.
- c: emit_exec/emit_pipeline/abort_target (sash/src/emit_c.rs).
- janet: emit_janet_general_command + EmitCtx.checked (sash/src/emit_janet.rs).
