---
title: chk discards failure evidence, so a transient cannot be diagnosed
status: open
priority: P3
kind: tooling
source: journal 508 (the c:cli.31/32/40/41/42 flake)
depends: []
gates:
  - DG's call — this changes the authoritative gate's harness.
---

## What

`spec/common.sh`'s `chk` records a status and nothing else. On failure it
prints `not ok <index>` to fd 3 and writes a `failing` row to the TSV; `$out`,
`$err` and `$rerr` are discarded at that moment and never recovered.

For a **reproducible** failure this costs nothing — you re-run the spec and
look. For a **transient** it is fatal. The only artefact is a row in `runs`
saying something was wrong once, with no way to learn what.

## Why it came up

Five specs — `c:cli.31`, `32`, `40`, `41`, `42` — flipped passing→failing in
one sweep and back in the next. Everything checkable was checked and cleared
(no code change but a doc comment; identical test bodies in both modes, yet
only `c:` failed; per-file `mktemp -d` with an EXIT trap verified not to fire
in subshells; no disk or inode pressure). It did not reproduce in 40 targeted
iterations or in a second full sweep.

It stays unexplained **because the evidence was discarded at the instant it
existed.** Journal 508.

These five compound the problem: they assert `$(grep -c SB f) = 0` alongside
an exact message match, a shape that cannot distinguish *wrong output* from
*no output at all*. A regression and a transient render identically.

## Options

1. **Nothing.** Transients are rare; the cost is one unexplained row.
2. **Echo context on failure only.** In `chk`'s failing branch, emit `$out` /
   `$err` / `$rerr` to the sweep log. Cheap, no schema change; the log grows
   only when something is already wrong.
3. **Record it.** Add a column to `runs` (or a side table) holding the captured
   streams for failing rows, so the history is queryable rather than living in
   a log that is not kept.

Option 2 is the smallest thing that would have made the flake diagnosable.

## Caution

`chk` reads `$?` as its first action. Anything added to it must not disturb
that — `local -i last_rc=$?` has to stay the first statement, and any new
command must come after the comparison. A context dump that clobbers `$?` for
the *next* `chk` would be a silent, suite-wide fault.
