title: Missing-index compiler commits require an explicit bypass
date: 2026-11-09

## DG's decision

After recording the single-write output rule, DG asked for the next
question. Q1 in `q-complexity-gate-without-index` concerns the hook's
warning-and-pass behavior when `.indexnudge-rust.db` is absent.

The AI recommended blocking commits that touch `sash/src/` unless the
index is available or `SKIP_COMPLEXITY_GATE=1` is deliberately set.
Documentation-only commits need no complexity measurement. The stated
cost was requiring indexnudge setup or an explicit bypass before compiler
changes can be committed from a fresh clone.

DG selected:

> Block compiler-source commits unless explicitly bypassed

This continues the policy/backlog-recording workflow. DG selected the
rule; the AI recorded its scope and enforcement obligations. The hook
has not been changed.

## The scope is the staged commit

Staged changes touching `sash/src/` require the index. Include additions,
deletions and renames into or out of that directory; an extractor that
finds no added lines has not proved that there are no compiler changes.
Unstaged compiler edits alone do not turn a docs-only commit into a
compiler-source commit.

This determines whether measurement is required, not which snapshot the
checks should measure. Q2 remains open: formatting, lint, totality and
index freshness currently operate on the worktree, while touched ranges
come from the staged diff. No automatic stash, isolated snapshot or
partial-staging restriction was selected.

The existing compiler-only rubric, 3x-current-means threshold, dispatcher
exemptions and grandfather-on-modify rule remain. So does the explicit
gate override. `SKIP_COMPLEXITY=1` skips only the report, not enforcement.
Other hook checks remain under their existing rules; a complexity bypass
does not bypass formatting, lint or totality.

## Source evidence and ownership

The tracked recording baseline is `a0ab00c9`. The hook's last modifying
commit remains `d8e6491e`, the repair recorded in journal 556.
Reading its current source confirms:

- Lines 90-101 guard the report on index presence and keep report failure
  informational.
- Lines 131-136 warn when the index is missing, then omit the gate.
- Lines 139-154 extract staged new-side hunk ranges with positive line
  counts; this is narrower than detecting every staged compiler change.
- Lines 165-192 already refuse a failed required index query.

Journal 556's controlled results were not rerun. In particular, its
withdrawn "query failures pass" claim stays withdrawn: the real hook's
`set -euo pipefail` options matter.

AGENTS.md records the accepted contributor policy with an explicit
enforcement-pending label. New P3 work item
`require-complexity-index-for-compiler-commits` owns implementation and
before/reversion proof. Keep the existing question record open for Q2
rather than make its unanswered snapshot question an implementation
dependency or falsely close it. That task split is the AI's reversible
organization choice, not another design decision.

The implementation controls must isolate the hook from the shared live
index/worktree, prove the missing-index refusal fails on the old hook,
preserve valid-index and explicit-override behavior, and run a reversion
control under the actual shell options. No new harness or index bootstrap
mechanism was selected here.

Only contributor-policy, backlog, journal and status documentation changes.
No hook, compiler, runtime, converter, spec or build code changes; no new
behavior probe, hook-control run, spec sweep or coverage measurement.
GUIDE needs no architecture revision. Hook enforcement remains queued.
