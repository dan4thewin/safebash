---
title: "bug: the read family does not abort on a failed redirect open"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`readline v < nosuchfile` aborts (DD#55), as `/bin/cat < nosuchfile` does"
  - "`read arr < nosuchfile` likewise, rather than yielding an empty array"
---

## What

DD#55 makes a failed redirect `open(2)` an abort — verified for an external:

```sh
/bin/cat < nosuch      # -> "nosuch: No such file or directory", aborts
```

The read family does not get that guard. Both of these succeed silently:

```sh
readline v < nosuch    # v = "", exit 0, no diagnostic
read arr < nosuch      # len 0,  exit 0, no diagnostic
```

So a mistyped path is indistinguishable from an empty file — the program
carries on with an empty value it believes it read.

## Why it is not the pipeline-stage bug

Separate from `bug-pipeline-stage-input-redirect-dropped` (fixed 2026-09-09,
commit b57e1ec): there the fd was wrong, so the redirect went to the stage's
stdout. Here the fd is right and the read works when the file exists — only
the **open failure** is unchecked.

Likely the same root as the note in `is_bulk_read_cmd`: the bulk reads lower
to SSA instructions that carry no redirect list, so the redirect is applied
around them by the caller, and that path does not run
`emit_redirects_guarded`.

## Notes

Pre-existing — `readline` behaves this way and was not touched by DD#99 stage
6. Found while restoring `read`'s redirect handling during the stage 6 rename.
