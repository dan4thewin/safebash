# 625 - Keep the current handoff, not another historical queue

DG asked for a review of the files directly under `notes/`, keeping only
high-value notes. The AI reviewed the fourteen files and recommended retaining
two. DG approved the exact deletion set, shortening status, preserving the
postmortem and repairing surviving references.

## What remains

`status.md` is a current handoff again. Its 20,936 lines included 460 STATUS
headings, duplicate buried frontmatter and old sections still labeled
"Architecture (current)" and "Remaining open tasks". The recent entries,
structure, managed block and representative older sections were inspected;
this was not a line-by-line re-audit of every historical claim.

The replacement keeps recent completed work, explicit deferred-work
boundaries, navigation and the existing generated gate block. Dated narrative
belongs in the journal; the pre-cleanup status remains retrievable from Git.
AGENTS now says to replace superseded progress rather than prepend another
permanent session log.

`parser-migration-postmortem.md` remains because it is the concrete process
evidence AGENTS cites: the AI narrowed choices or justified unapproved
deferrals, and DG corrected those decisions. Its original explanations and
corrective addenda are retained verbatim, behind a new historical banner.
They are not current architecture or implementation instructions.

## What was removed

| Files | Reason |
|---|---|
| `_audit-tasks.md` | All 49 links target a directory that no longer exists. |
| `_decisions.md`, `_journal.md`, `_tasks.md` | Frozen partial indexes: 47 decisions, 55 journal entries and 91 tasks, not the current collections. |
| `_r47-queue.md` | Obsolete global priority override; its task files retain their own dispositions. |
| `safebash.md` | Redundant landing page, including a dead `pinned.md` link. Status now carries the useful navigation. |
| `architecture.md`, `builtins.md` | Retired compiler pipeline, file names, commands and taint claims; superseded by current guidance and DDs. |
| `audit.md` | Old audit/Brush snapshot, not an account of current assurance. The actual audit records remain. |
| `dd-candidates.md` | Migration-era proposals and claims superseded by later decisions, including the pipe-taint and error models. |
| `opus-47-review.md` | May assessment and recommendations, not a current work queue. The surviving delegate-management task is self-contained and still pending. |
| `coverage-accounting.md` | August ledger with obsolete measurements, interfaces and reachability claims. Current methodology is in AGENTS and later journals. |

One useful coverage caution was retained in AGENTS: a demangled short name
does not identify a function uniquely. Cold generic instantiations or closures
can share a live function's name; attribution needs full symbol and source
identity. Journal 359 records the distinction.

## Retrieval and boundaries

All removed files and the full old status are in the pre-cleanup revision
`f02477dd31e67a0fe6fe3c4a359935569c4715d8`. For example:

```sh
git show f02477dd31e67a0fe6fe3c4a359935569c4715d8:notes/coverage-accounting.md
git show f02477dd31e67a0fe6fe3c4a359935569c4715d8:notes/status.md
```

Retained review-source and coverage-ledger references now point to that frozen
revision. Historical task requirements are not new work, and no task status or
priority was changed by this cleanup.

The `decisions/`, `journal/` and `tasks/` collections were not pruned.
`.iwe/config.toml` is unchanged. No compiler, runtime, helper, spec or build
source was changed; no implementation or coverage batch was started.
The generated status block remains the previous actual gate's snapshot,
not a claim that documentation cleanup ran a new sweep.

## Verification

The direct-file inventory is exactly the two retained notes; status is now
79 lines. All 25 current/new local navigation links resolve, and all four
frozen Git link targets exist at the recorded revision. The removed files
and old status match that revision byte-for-byte.

The postmortem's original body and generated status block are unchanged.
Task titles, statuses, priorities, kinds and dependencies are unchanged.
Whitespace and exact change-scope checks pass. Both local lockfiles,
compiler/helper binaries, runtime archive and iwe configuration retain their
baseline hashes; spec history remains at row 6902131.
