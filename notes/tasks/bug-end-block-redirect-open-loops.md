---
status: done
priority: P1
---

# Redirect-open failure inside `END { }` never terminates

**Done** — journal 368, specs `56.80`-`56.87`. The fix was wider than
filed: the unchecked marking's label test only matched END's *entry*
block, so any failing command in a nested block inside END looped too.
`SsaProgram::end_region()` (reachability) replaces the label test and is
now shared by the transform and both backends.

A redirect whose `open(2)` fails, on a plain command inside an `END`
block, loops forever emitting the diagnostic.

```sh
echo main
END { echo hi > /nonexistent/x; echo tail }
```

Measured: rc=124 under `timeout 3`, 380,232 identical lines of
`./pb: /nonexistent/x: No such file or directory`. Both backends.

## Cause

The uncaught-failure path in `emit_open_failure_abort`
(`sash/src/codegen/inst_handlers.rs`, and its `emit_c.rs` mirror) routes
the abort through `ctx.end_cl_block` so an END block still runs. Inside
END, `end_cl_block` is still `Some` — so the jump targets the block it
is already executing.

`mark_unchecked_in_end_blocks` (`transform/entry.rs`) exists to stop
exactly this; its comment states a failed cleanup step "must not abort
the program if it fails (otherwise a failed cleanup step would skip the
rest of END)". It cannot reach this path: it clears `checked`, while the
redirect-open abort keys on positional `in_condition` and never consults
`checked`.

## Scope — redirect-specific

Measured inside `END { }`:

| failure                       | behaviour        |
|-------------------------------|------------------|
| command failure (`/bin/false`)| continues        |
| exec failure (`/nonexistent/prog`) | diagnoses, continues |
| division by zero              | exits cleanly rc=2 |
| **redirect open failure**     | **infinite loop** |

The new `RedirectPush` scope (`f > file`, journal 367) is already
correct here — it was written with this hazard in mind and measures
rc=0, cleanup completes.

## Fix — DG chose A1 (2026-09-18)

Diagnose, restore the fds, set status 1, and **continue the rest of the
cleanup**. Matches `mark_unchecked_in_end_blocks`'s stated intent, the
exec-failure precedent, and what the new scope already does.

Rejected A2 (diagnose and `exit(1)`, skipping remaining cleanup): one
bad path would silently truncate the user's cleanup.

## Not a bug — do not "fix" these while here

`errors.sh` 55.26 pins `- echo > /bad` as deliberately still aborting
("`-` suppresses echo's write verdict, not the prior `exec N>file` open,
which runs first"), and 55.25/55.50/55.63/55.64 pin "no misroute, no
diag" in condition position. The exec-failure path differs in both, but
that divergence is recorded design, not drift.

## Gates

Needs specs in both positions (plain command and `read`/`readline`,
which share the guard) plus a `timeout` so a regression fails loudly
instead of hanging the sweep.
