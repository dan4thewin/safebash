title: Accepting the ERR-attempt branch on main
date: 2026-11-26

# Accepting the ERR-attempt branch on main

DG directed:

> take the current branch to main as accepted (no longer provisional),
> then start a new branch and implement slice as provisional

The first part accepts the complete implemented `err-attempt-trial` branch.
DG makes that adoption decision; the AI reconciles the records and lands the
existing implementation. The new slice trial is separate and does not
become part of main through this acceptance.

## Accepted scope

The evaluated tip is `74b95573dcde72db9c03e080fd78a3b20bfa0a52`, twelve
commits after main `02217248a6ea5f34f95d62b6dca740f166776d3f`.
Journals 664-675 record their individual delivery and evidence.

DD136 is accepted, including ERR-owned void/status call attempts, full-chain
abandonment, the narrow recoverable boundary, checked terminal guards,
attempt-owned resource cleanup, abandoned-prefix analysis, and independent
`_=` disposal. The companion DDs, GUIDE, outline and tour now describe those
rules without a provisional qualifier. The integrated trial owner closes.
Named-predicate capture conditions and fatal-fork cleanup retain their
independently accepted contracts and verified implementation.

The landing also includes runtime-indexed unset, snapshot map-key loops,
existing-map and typed-slot assignment, ordinary operand-word collection
keys, quoted arithmetic map keys, `cpath?`, direct immutable redirect proofs
including lexical captures, general eager lookup defaults, and
`epochuseconds`. The branch's backlog records are retained too; acceptance
does not claim their fixes.

Only the adoption records change here. Source code, spec programs, tour
programs, existing journals, lockfiles and protected local files are
preserved. The current-rule lists are reconciled in place; original
provisional requests and dated delivery evidence remain historical.

## Boundaries and landing

General file-error handling, the remaining suppression guards, converter
glob/scratch/regex migration, wider capture buffering, the recorded
read-source lowering failure, timestamp-spec investigation and coverage
work remain separately owned. Accepting DD136 neither broadens its narrow
recovery boundary nor waives any runtime guard.

There is no associated open PR or stack. The verified remote is
`git@github.com:dan4thewin/safebash.git`. Local and live remote main started
at `02217248`; the local and live trial agreed at `74b95573`.
Main is fast-forwarded without checking out the historical source tree.
The acceptance-record commit follows the evaluated tip on main.
The trial remains at that tip, with no merge commit, history rewrite,
force push, branch deletion or reference-worktree change.

The private acceptance baseline records all 1,586 tracked-file hashes,
eight protected artifact/file identities and spec-history boundary 7,530,772.
## Verification

The zero-change prediction passes: NEW=0, FLIPS=0, RETIRED=0, TOTAL=13,240.
The ordinary gate covers rows 7,530,773-7,544,012: 6,736 native and 6,504 C,
13,227 passing and 13 existing skips, with no failures, ghosts or plan
mismatches. Liveness remains 246. This runs the normal unit/static checks
too; no coverage instrumentation is introduced.

`mk -p 1 tour-doc` checks all 73 snippets with no mismatch and regenerates
the GitHub edition. Its program and output blocks are unchanged; only the
acceptance introduction and three headings change.

The private evidence directory `files/err-adoption-20261126/` retains the
baseline, gate/tour logs, document-preservation checks and publication
identities. Main is published through the normal hooks before creating
the separate provisional slice branch.
