title: Make path-specific file failures catchable and suppressible
status: open
priority: P2
kind: bug
depends: [runtime-failure-classification]
gates:
  - "sash-owned path-specific file failures identified by the DD130 inventory have real recoverable handling; missing handling support is not relabeled fatal"
  - "a command's recoverable redirect-open failure is implicitly checked, caught or suppressed by its owning context in both backends"
  - "a failed open skips the command and later redirects, restores scoped descriptors before continuation and never sends output to the original destination"
  - "fatal causes remain fatal under conditions, negation and suppression; handled file errors never invent a successful default value"
  - "actual failing paths, ordered earlier side effects, diagnostics, captures and saved handler status remain correct"
  - "function result kinds and no-reach into helpers/operand captures remain intact; ERR analysis agrees with the actual handling"
  - "focused native/C specs fail on the original behavior or targeted reversions, and the predicted ordinary gate passes"

## Decision

DG placed path-specific file errors in the recoverable class (DD130,
2026-11-06; journal 581). A failed destination need not prevent useful
work elsewhere. The rule is not limited to output redirection: use the
classification inventory to account for the other sash-owned file APIs
and their actual failure/result contracts.

The concrete current mismatch is:

```sh
- /bin/true > missing/out
echo continued
```

With `missing/` absent, this currently aborts with status 1 instead of
continuing. The equivalent suppressed pipeline stage continues silently.
Both native results were recorded at `a7787b24` in
`q-setup-failure-suppressibility`; no repair has been implemented.

## Required handling

For an ordinary path-open failure, the unchecked form still aborts, a
legal condition handles failure, and `-` discards it. No body runs when
its required redirect failed. Restore descriptor mappings before any
continuation; never fall back to writing on the original stdout/stderr.
Stop the redirect list at the first failure, reporting that path, without
pretending to undo files already created or truncated by earlier redirects.

The classification prerequisite matters: `open` can fail for reasons other
than the selected path. Suppression must not discard a classified fatal
fault merely because it came from the same API as a recoverable failure.

Preserve actual value/status contracts for read, capture and other file
operations. If a form has no way to expose its recoverable failure, design
that handling consistently with the existing types before changing it;
do not fabricate empty success or turn a void/int result into a status.
Raise any genuine language-surface fork with DG.

This does not extend a prefix into a helper body or argument capture.
DD86 separately settles attached-redirect timing: arguments first in both
standalone commands and stages (2026-11-07; journal 585). Coordinate with
`q-stage-capture-redirect-order`, which owns that pending repair: a failed
open must not undo or pretend to skip completed argument effects. The
direct builtin stderr optimization is independently owned by
`bug-builtin-stderr-backup-restore`; retain its actual-effect proof rather
than suppressing SB5101 around unused machinery.
