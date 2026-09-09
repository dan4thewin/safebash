---
title: "bug: an input redirect on a pipeline STAGE is silently dropped"
status: ready
priority: P1
kind: bug
depends: []
gates:
  - "`/usr/bin/tr '\\n' '\\0' < f | newread a` fills `a` from `f` (today: empty, no diagnostic)"
  - "`/bin/cat < f | /bin/cat` prints f's contents"
  - "a redirect that cannot be honoured on a stage is a compile error, never a silent empty read"
---

## Found

2026-09-09, checking DD#99's line-splitting idiom. DG: *"it's 1 line:
`tr \n \0 < f | newread a`"* — and it is, except the redirect never reaches
`tr`.

```sh
/bin/cat < f.txt                              # x y z   ✓
/bin/cat < f.txt | /usr/bin/tr '\n' '\0' | …  # nothing  ✗
/bin/cat f.txt   | /usr/bin/tr '\n' '\0' | …  # x y z   ✓  (workaround)
```

With stdin attached to a terminal the program **hangs** (the stage waits on
stdin that was supposed to be redirected); with `</dev/null` it silently reads
nothing and the consumer sees an empty array. Either way there is no
diagnostic.

`/bin/grep x < f.txt | /bin/cat` confirms it independently: grep exits 1
(matched nothing) because it received no input.

## Scope

A redirect on a **non-pipeline** command works. It is specifically a stage of a
pipeline that loses it. `push_command_stage` does collect a stage's redirects
(and `spill_scalar_redirect_targets` handles their targets), so the loss is
somewhere between there and the child's fd setup — worth checking whether the
stage's redirect list survives into `SsaInst::Pipeline` and is applied after
the pipe fds are wired, rather than before and then overwritten.

## Why it matters now

This is the canonical DD#99 idiom for converting a separator to NUL —
`tr SEP '\0' < file | read arr` — and for the many `mapfile` sites that stage 5
must migrate. The workaround (`cat file | tr … | read arr`) costs an extra
fork, so the redirect form is the one worth having.

Independent of DD#99: it is a plain redirect bug that predates it.
