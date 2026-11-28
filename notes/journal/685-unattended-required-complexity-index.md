title: "685: Unattended backlog - required complexity index"
date: 2026-11-28
status: complete

# 685: Unattended backlog - required complexity index

DG authorized twenty fully determined, low-complexity backlog items on a
session branch, deferring larger or oversight-dependent work without stopping
the queue. The final implementation item is the already accepted
[missing-index policy](../tasks/require-complexity-index-for-compiler-commits.md).
DG chose that policy on November 9; the AI implemented this guard and its
controls. No new complexity threshold or snapshot policy was selected.

## Enforcement

The pre-commit hook now determines compiler scope from staged paths:

```sh
git diff --cached --name-only --no-renames -- 'sash/src/'
```

Disabling rename pairing exposes source and destination independently. The
scope therefore includes moves out of the compiler, empty-file additions and
deletions, and non-Rust paths within `sash/src/`. It is deliberately broader
than the existing positive-new-line Rust hunk ranges used to select functions.
An empty set of those ranges is not evidence that the compiler was untouched.

For that scope, absence of `.indexnudge-rust.db` is a refusal unless
`SKIP_COMPLEXITY_GATE=1` is explicitly set. The hook reports bypass and
out-of-scope checks separately; `SKIP_COMPLEXITY=1` skips only the report.
Staged-path and hunk inspection failures receive explicit refusal messages.

The required SQL, current-means 3x rubric, both dispatcher exemptions,
grandfathering, normal `sqlite3` freshness path and all other gates are
unchanged. The old comment proposing eventual 2x/1.5x thresholds was corrected
to match the existing AGENTS decision, not used to change the threshold.

## Actual-hook controls

Twenty-eight cases execute the complete hook with its real
`set -euo pipefail`, in newly initialized private Git repositories.
Fixture source/configuration is fully staged. Cargo and totality outcomes are
controlled; Git produces real staged diffs, and the actual hook SQL runs
against small deterministic SQLite databases.

The fixture's two database names are hard links to the same file. Its
controlled `sqlite3` command sends the query through the ordinary PATH to
that fixture alias, preventing indexnudge from replacing synthetic metrics
with a source freshen. The development index is never moved, replaced or
removed. This is controlled gate input, not a new claim about index freshness
or metric calibration. No production commit is made by a control.

| Cases | Original hook | Repaired hook | Guard-only reversion |
| --- | --- | --- | --- |
| Ten missing-index compiler cases | Incorrectly permit | Refuse | Incorrectly permit |
| Failed staged-path probe | Never probes; permits | Explicit refusal | Explicit refusal |
| Four bypass/out-of-scope notices | Missing | Distinct notices | Distinct notices |
| Failed hunk probe | Refuses, no contextual message | Explicit refusal | Explicit refusal |
| Twelve preservation controls | Pass | Pass | Pass |

The ten missing-index cases cover ordinary edits, report-disabled edits,
nonempty and empty additions, nonempty and empty deletions, renames in/out/
within the compiler, and a non-Rust compiler path.

Preservation covers a nonviolator, separate cyclomatic-only and cognitive-only
violators, the wide and flat dispatcher exemptions independently, an untouched
violator in the same edited file, report failure, required SQL failure, empty
hunk ranges, and formatting/lint/totality refusals despite complexity bypass
or docs-only scope. Required SQL errors already refused before this change;
neither they nor the original hunk failure are claimed as permissive bugs.

The original hook fails exactly sixteen assertions. The repaired hook passes
all 28. Replacing only the missing-index exit with the old skip behavior
makes exactly ten assertions fail, while the other eighteen still pass.
That reversion exists only in the fixture copy; the live hook stays at the
verified positive bytes. Source, staged trees and stash are preserved.

## Ordinary gate

Prediction: `NEW=0 FLIPS=0 RETIRED=0 TOTAL=13529`.
The serialized, bounded ordinary gate passes at rows **7678577-7692105**:
native **6891**, C **6638**, **13516 passing** and **13 existing skips**.
There are no failures, flips, first-ever IDs, retirements, ghosts or plan
mismatches. Liveness remains 246. The sweep takes 336 seconds; the gate also
runs the ordinary build/unit and static checks.

Compiler, converter, runtime and lockfile identities remain unchanged from
the diagnostic cohort. This is a hook-only implementation, with contributor
guidance, task disposition and status updated alongside it. No new compiler
spec, dependency, coverage run, public architecture or test framework is added.

## Twenty-item disposition

Nineteen owners are implemented on `unattended-backlog-20261128`:

| Cohort | Owners | Record |
| --- | --- | --- |
| Names and definitions | 5 | [681](681-unattended-naming-guards.md), `b1a29d06` |
| Clause and loop syntax | 2 | [682](682-unattended-clause-and-loop-syntax.md), `da4efe4d` |
| Converter boundaries | 4 | [683](683-unattended-converter-boundaries.md), `fd68d349` |
| Diagnostics and C literals | 7 | [684](684-unattended-diagnostics-and-c-literals.md), `c6bc61af` |
| Required complexity index | 1 | This journal |

The twentieth,
[SB5041 arithmetic-use wording](../tasks/bug-sb5041-text-for-arithmetic-uses.md),
is explicitly deferred. The current forward-call rejection contradicts
DD81/DD86's unique-hoisted-definition contract. A wording-only fix would
legitimize the wrong restriction; its existing owner now requires the larger
resolution repair and associated spec migration.

The independently owned
[checked-input agreement guard](../tasks/q-complexity-gate-without-index.md)
remains pending. Index presence does not make partial staging safe, and this
availability repair does not claim otherwise.

Main stays at `4e5090c9`, with no branch adoption, PR or reference-worktree
change. The shared environment, unrelated `notes/dat`, runtime/lockfiles and
frozen reference work are preserved.

Private evidence is in the session's `files/batch20-20261128/`: the original,
positive and reverted hooks, control driver, real staged patches, SQL inputs,
hook output/command traces and per-case results, plus `hook-gate.log`.
`hook-evidence.tar.gz` preserves all 84 control runs and their fixtures:
5060 members, with membership, regular-file bytes and symlink targets verified.
