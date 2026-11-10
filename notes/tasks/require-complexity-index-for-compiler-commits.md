title: "Require the complexity index for compiler-source commits"
status: open
priority: P3
kind: work
depends: []
gates:
  - "Without .indexnudge-rust.db, a commit touching sash/src/ is blocked with an explicit missing-measurement diagnostic unless SKIP_COMPLEXITY_GATE=1 is set"
  - "Commits with no staged path in sash/src/ need no complexity index; a separate checked-input mismatch may still refuse the commit"
  - "Staged additions, deletions and renames into or out of sash/src/ cannot evade the requirement through empty added-line hunk output"
  - "With a valid index, an actual touched violator fails and a nonviolating change passes under the unchanged current-means rubric"
  - "SKIP_COMPLEXITY=1 skips only the report; a required gate-query or staged-scope inspection failure is not accepted as an empty successful result"
  - "Focused before/reversion controls run under the hook's real set -euo pipefail options without moving the live index or disturbing user work"
  - "Normal repository gates pass and AGENTS.md, journal and status distinguish this enforcement from the separately owned checked-input agreement guard"

## Accepted policy

DG, 2026-11-09:

> Block compiler-source commits unless explicitly bypassed

The accepted proposal requires `.indexnudge-rust.db` when the staged commit
touches `sash/src/`, unless `SKIP_COMPLEXITY_GATE=1` is explicitly set.
No staged compiler change means this gate needs no measurement: a
documentation-only commit is not blocked merely because the index is absent.
The tradeoff is deliberate: compiler work in a fresh clone requires
indexnudge setup or an explicit bypass, not an automatic exemption.

AGENTS.md's complexity section is the contributor-policy authority.
Journal 606 records the decision. Q1 in `q-complexity-gate-without-index`
is answered as policy; DG subsequently settled Q2 by choosing refusal of
mismatched checked inputs (journal 607). That existing task now owns
Q2 enforcement. This task owns the missing-index refusal; neither change
is implemented yet.

## Scope and preservation

Use the staged change's paths to establish whether the index is required.
Include additions, deletions and renames with either side in `sash/src/`.
The existing hunk extractor records only new-side ranges with a positive
line count; its empty output cannot serve as this broader scope test.
Unstaged changes alone do not change the staged commit's scope.
They can still violate the independent checked-input agreement rule:
a docs-only commit may need no complexity index yet be refused because
its hooks would read unstaged compiler source. "No index required" is
not a blanket permission to commit mismatched inputs.

Keep the existing compiler-only, grandfather-on-modify complexity rubric:
current measured means, the 3x threshold and both dispatcher exemptions.
Requiring an index does not mean every changed path contains a function
that violates the rubric, and it does not extend the rubric to other crates.
Do not hard-code means, introduce a function exemption or select a new
threshold while making the availability check.

Keep `SKIP_COMPLEXITY_GATE=1` as the deliberate gate override, distinct
from `SKIP_COMPLEXITY=1`, which controls the informational report only.
Identify a deliberate bypass or out-of-scope check as such, not as a
successful measurement. Preserve the existing failure of a required index
query; failure to determine the staged scope must not masquerade as
"no compiler changes". Report-only query failures retain their separate
reporting behavior.

Formatting, lint and totality checks are not waived by a missing index,
an out-of-scope complexity check or the complexity-specific override.
The existing global hook escapes are not redesigned by this decision.
Use the normal indexnudge freshness path; merely finding a database is
not a new proof of its provenance or of staged/worktree equality.

## Baseline and proof

At `a0ab00c9`, `.githooks/pre-commit:131-136` warns on a missing index
and then omits the gate, regardless of staged scope. Its report guard is
at lines 90-101, hunk extraction at 139-154, and required-query failure
handling at 165-192. The hook's latest modifying commit is `d8e6491e`.
These are source observations, not newly executed controls.

Journal 556 records the existing controlled-hook method and its important
correction: omitting `set -euo pipefail` falsely suggested that query errors
passed. Run the actual hook/options in an isolated fixture with deterministic
Git/index-query inputs. Do not remove or rename the live development index,
auto-stash the real worktree, or let a control create a production commit.

The regression assertion for the missing-index compiler case must fail
before the change because the hook permits it, then pass on the refusal.
Cover default and report-disabled runs, explicit gate bypass, docs-only
scope, additions/deletions and both rename directions. Preserve valid-index
pass/violation and required-query-error controls. Revert the new guard on
the final implementation and confirm the missing-index controls go red.

`q-complexity-gate-without-index` owns Q2's accepted refusal of differences
between checked worktree inputs and Git's staged snapshot. The guards can
land independently or together, with no hard dependency inferred from
their shared hook file. Do not claim that missing-index refusal alone
fixes partial-staging attribution.

Keep controls independent: a missing-index refusal case must have matching
checked inputs, or an earlier snapshot refusal could mask the absent guard.
A docs-only/no-index preservation case must likewise satisfy the input
agreement rule. No automatic stashing or isolated staged-tree check is
introduced by either task.
