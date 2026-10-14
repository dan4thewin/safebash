---
title: "CLOSED: chk discards failure evidence, so a transient cannot be diagnosed"
status: closed
closed: 2026-10-14
priority: P3
kind: tooling
source: journal 508 (the c:cli.31/32/40/41/42 flake)
depends: []
gates:
  - Withdrawn in place. The premise was false; see below.
---

## Closed — the premise was wrong

This task claimed `chk` discards `$out` / `$err` / `$rerr` on failure, leaving
a transient undiagnosable. **That is not true, and DG caught it with one
question: "what about the xtrace?"**

`spec/common.sh`'s `setup()` already captures everything:

```sh
trace=.trace/$(date +%a)/"$id.$SASH_MODE"
exec 3>&1 >> "$trace" 2>&1
PS4='+$LINENO: '
set -x
```

The spec file's entire stdout and stderr — every command, every expansion,
every variable assignment, line-numbered — goes to a per-file, per-mode trace.
`exec 3>&1` is precisely why `chk` writes `ok` / `not ok` to fd 3: fd 1 is the
trace. `run.sh`'s `2>/dev/null` discards only what escapes that redirect.

So the harness does not need a failure-context feature. It has had one all
along, and I filed a task against a facility I had not read.

## What the trace actually showed

The flake it was filed over was **not a transient**. The trace held the answer
verbatim:

```
+262: sr_err='sash: --sysroot=/tmp/tmp.nkqYLkSBp3/nosuch.squashfs: Could not open ...'
++263: grep -c SB sq.err
+263: [[ 1 = 0 ]]
```

`mktemp -d` had produced `/tmp/tmp.nkqYLkSBp3` — ten random alphanumerics that
happened to contain the literal `SB`. The five specs grepped their captured
stderr for a bare `SB`; every one of those messages echoes the `--sysroot`
path; the path contains the tmpdir name. The count matched the **directory
name**, not a diagnostic code.

That accounts for every feature of it — c mode only (its `mktemp -d` differed
from native's), all five together (one dir), never reproducible (a fresh name
each run), once in 400+ runs (~0.2%: 9 positions x 1/62^2).

Fixed in `spec/cli.sh` by matching the full code shape, `SB[0-9]{4}`, with a
comment recording why. Reproduced deterministically before the fix and
verified after.

## The lesson, which did survive

Two, and neither is the one this task was filed on:

1. **Read the harness before concluding it lacks something.** The trace
   directory is right there in `spec/.trace/`, retained about a day.
2. **An assertion must not be able to match the scaffolding.** A bare `SB`
   against text that embeds a `mktemp` path is a test that fails at random and
   looks exactly like a regression when it does.

Both are now recorded in AGENTS.md.
