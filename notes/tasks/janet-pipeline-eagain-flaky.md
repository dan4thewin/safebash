title: "janet backend pipelines are flaky (os/spawn pipe EAGAIN: 'Resource temporarily unavailable')"
status: done
priority: P4
kind: bug
depends: []
gates:
  - "[DONE] a janet-emitted pipeline produces identical output on 100 consecutive runs — verified 2-stage 0/100, 3-stage 0/60, 5000-line stress 0/60"
  - "[DONE] wordsplit 22.18/22.19 un-guarded — now pass under janet"

# RESOLVED 2026-07-03

Root cause: janet's `:out :pipe` streams (and `os/pipe`'s default ends) are
**non-blocking** (O_NONBLOCK, for the ev loop). The old emission chained
stages with `:in (p{i-1} :out)`, handing a non-blocking fd to the next
child's stdin — so the child (`cat`, `wc`, …) read EAGAIN
("Resource temporarily unavailable") and the janet collector then saw nil.

Fix (`emit_janet.rs` `emit_pipeline` / `emit_pipeline_stage` /
`emit_pipeline_close_pipes` / `emit_pipeline_wait_and_collect`): connect
stages with explicit **blocking** pipes `(os/pipe :WR)`, keep `:out :pipe`
only on the last stage (janet drains it via the ev loop, where
non-blocking is correct), close the parent's inter-stage ends so EOF
propagates, and read the last stage before waiting (no buffer deadlock).

---

# (original) janet pipelines: intermittent os/spawn pipe EAGAIN


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
