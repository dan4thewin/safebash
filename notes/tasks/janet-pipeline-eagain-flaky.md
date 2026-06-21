---
title: "janet backend pipelines are flaky (os/spawn pipe EAGAIN: 'Resource temporarily unavailable')"
status: ready
priority: P4
kind: bug
depends: []
gates:
  - "a janet-emitted pipeline (e.g. `printf … | cat`) produces identical output on 100 consecutive runs (no 'Resource temporarily unavailable', no 'bad slot … got nil')"
---

# janet pipelines: intermittent os/spawn pipe EAGAIN

## Symptom

Janet-emitted pipelines intermittently fail (~2/10 runs) with:

```
cat: -: Resource temporarily unavailable
error: bad slot #0, expected string, symbol, keyword or buffer, got nil
  in string/trim [src/core/string.c] on line 602
```

Reproduced 2026-06-21 with both a splat pipeline (`/usr/bin/printf
'[%s]\n' @a | cat`) and a **plain** non-splat pipeline
(`printf '[%s]\n' p q r | cat`) — so this is **pre-existing** and
independent of the array-splat work. The emitted `os/spawn` chain races
on the pipe (non-blocking read returns EAGAIN; the collector then sees
nil).

## Impact

Any spec test that exercises a janet pipeline is non-deterministic. The
array-splat pipeline tests (`wordsplit` 22.18, 22.19) are guarded to
`native,c` for this reason; the janet os/execute (non-pipeline) splat
path is reliable and stays under test.

## Fix

Make the janet `os/spawn` pipeline emission robust to EAGAIN — e.g. set
the pipe fds to blocking, or use `ev/` blocking reads / `:read :all` with
a proper wait — in `emit_pipeline_stage` / `emit_pipeline_wait_and_collect`
(emit_janet.rs). Then re-enable janet for the guarded pipeline tests.
