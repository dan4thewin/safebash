---
title: "`write` failures are reported as `echo:`"
status: open
priority: P4
kind: bug
depends: []
gates:
  - "w1 and w2 below name `write`, both backends"
---

## What

`write` is lowered as `echo`, so every runtime message about it says `echo:`.

| Id | Program | Runtime stderr (both backends, rc 1) |
|---|---|---|
| w1 | `write hi >/dev/full \| /bin/cat` | `echo: write error: No space left on device` ⏎ `echo: command failed (exit 1)` |
| w2 | `write hi >/dev/full` | `echo: write error: No space left on device` |

## Cause

`transform/cmd_handlers.rs` `rewrite_write_as_echo`, called from
`emit_external_command` and `push_command_stage`. The runtime's echo prints
the `echo:` label, and the stage report uses the rewritten name.

Pre-existing on every version. Found during F2 (journal 539): w5/w7, the
same shape with an argument capture, used to ICE, which was B81.
