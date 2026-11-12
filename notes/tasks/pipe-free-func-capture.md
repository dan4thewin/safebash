title: Pipe-free user-function capture
status: done
depends: []
gates:
  - 17.1
  - 17.2
  - tests
  - commit

`$(myfunc)` currently redirects stdout through a pipe even though
the function body is inlined. Should accumulate into a buffer
directly, eliminating the pipe/read overhead. External commands
still need fork+pipe. This is the trampoline model's (DD#27)
killer advantage — the compiler can see inside the function.

## Actually done 2026-09-13 (journal 277)

**This task was marked `done` in `c958d3d` ("housekeeping: update
decision/task statuses") without the work being done.** Its gates — 17.1,
17.2 — test that `$(myfunc)` *works*, not that it works pipe-free, so they
were green the whole time and the bulk status sweep closed it on their
strength. The pipe was still there. It then survived two more capture
sessions (journals 275, 276) with nobody looking at this file, because a
`done` task is not something you re-read.

Worth stating plainly: **closing a task on its gates is only sound when the
gates encode the requirement.** These encoded the feature.

It is done now, and by the route this filing predicted — "the compiler can
see inside the function". `transform/capture_purity.rs` proves a body never
forks onto its own stdout and never rebinds fd 1; `sash_capture_start_buf` /
`sash_capture_end_buf` then collect it in a userspace buffer with **zero**
syscalls (measured: 3014 → 4 on a 1000-line body, and all four belong to
program startup and the final `echo`).

The filing's "external commands still need fork+pipe" is right but narrower
than the real rule DG drew: a body may *contain* an external and still be
buffered, so long as the external's output arrives through an inner `$( )`
whose child gets a pipe of its own. Only a fork inheriting *this* capture's
stdout disqualifies. Impure sites keep the memfd path from journal 276.

Specs: `cap.15` (the discriminating shape), `cap.12`/`cap.17` (the visible
pure/impure split in what a body sees as its own stdout), `cap.18` (the
runtime stack's suspend rule), `cap.7` (buffer growth to ~500 KB).
