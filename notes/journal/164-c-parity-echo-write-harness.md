# 164 — c parity (batch 8): echo write(2) abort + harness $rerr capture

Continues the descriptive-abort area (cd landed in 163). Un-guards the cd +
echo-write tests for c: **+7** (55.20-23, 55.27-29), 0 regressions.

## echo write(2)-error abort

`emit_echo` called `sash_write(1, …)` and discarded the result, so
`echo hi > /dev/full` succeeded silently (native aborts). Restructured
`emit_echo` to take `checked` and accumulate a write-failed flag
(`wf |= (sash_write(...) < 0)`) across every part + the trailing newline;
it sets `$?`/`__test_result` from `wf` and, when `checked`, aborts with
`<argv0>: echo: write error: <reason>` (`sash_emsg("echo","write error")`)
via `abort_target`. The pipeline-stage echo passes `checked=false` (it runs
in a forked child). Removed the caller's old unconditional `$?=0` override.

## Harness: capture c runtime stderr into $rerr

`_run_sash_c` discarded runtime stderr (`2>/dev/null`); the DD#55 descriptive
diagnostics are on stderr, so the message specs (55.20, 55.27) couldn't be
checked in c. Changed it to `2>rerr.raw; rerr=$(<rerr.raw)`. The binary runs
as `./test_bin`, and the specs match `*"test_bin: …"*`, so the leading `./`
is absorbed by the glob's `*`. Full c sweep confirmed this is safe (0
regressions — sash programs only write stderr on error, which pairs with a
nonzero rc).

## Block split

`spec/errors.sh` descriptive-abort block → `native,c` for the cd + echo
tests; the redirect open(2) tests (55.24/25/26/30) are wrapped in a nested
`native`-only guard — their abort needs a skip-body + `in_condition`
mechanism (`- echo > /bad` must still abort, which `checked` alone can't
express, since `checked` conflates `-` with condition-position). Filed
`redirect-open-abort`.

## Result

Full c sweep: **0 regressions, +7** (55.20/21/22/23/27/28/29). Remaining c
gaps: bgpid (38.5/55.19), redirect-open-abort (55.24/25/26/30).

## Files

- `sash/src/emit_c.rs` — `emit_echo(checked)` write-failure abort.
- `spec/common.sh` — `_run_sash_c` `$rerr` capture.
- `spec/errors.sh` — descriptive-abort block split.
