title: "Require staged/worktree agreement for checked hook inputs"
status: open
priority: P3
kind: work
depends: []
gates:
  - "Worktree-based checks refuse mismatched source/configuration inputs before assessing them, including partial staging and relevant files absent from the staged diff"
  - "The checked-input scope covers the actual formatting, lint, totality and complexity inputs, including local dependencies/configuration and consumed untracked source"
  - "Unrelated unstaged files and generated index/build artifacts remain permitted; refusal is not a blanket clean-worktree requirement"
  - "Refusal names the mismatched inputs and leaves worktree, Git index and stash unchanged; no automatic stashing, staging or isolated-checkout substitution"
  - "Matching inputs preserve normal pass/failure behavior; the missing-index policy and per-check overrides do not become general consistency bypasses"
  - "Comparison failures and detected input changes during checks cannot be accepted as evidence that the checked snapshot matches the commit"
  - "Focused before/reversion controls use the real hook shell options and isolated fixtures; normal repository gates pass and implementation status is updated"

Found 2026-11-01 while repairing the indexnudge shim (journal 556). Since
that commit the hook *says* when the gate measured nothing. DG settled both
policy questions on 2026-11-09, recorded in journals 606 and 607.
This existing task retains its slug and P3 priority but now owns Q2
enforcement; Q1 has its separate implementation owner below. Both remain
unimplemented. Journal 556's controls remain historical measurements,
not new runs at either decision date.

## Q1. Settled: require the index for compiler-source commits

DG, 2026-11-09:

> Block compiler-source commits unless explicitly bypassed

The accepted proposal scopes this to commits whose staged changes touch
`sash/src/`. Without `.indexnudge-rust.db`, block unless
`SKIP_COMPLEXITY_GATE=1` is explicitly set. A commit outside that scope,
including a documentation-only commit, does not need an index for this
gate. Other checks remain mandatory under their existing rules.

The requirement follows staged paths, including additions, deletions and
renames across the scope boundary. An empty set of added-line hunk ranges
is not proof that the commit contains no compiler changes. Unstaged compiler
edits do not by themselves make a documentation-only commit a compiler commit;
they can independently violate Q2's checked-input agreement rule below.

AGENTS.md records the contributor policy.
`require-complexity-index-for-compiler-commits` owns implementation and
before/reversion controls. The 3x-current-means rubric, dispatcher exemptions,
compiler-only scope and deliberate override remain. This is not a new
requirement to install indexnudge for every documentation edit.

### Current implementation, unchanged by the ruling

Both the complexity report and the gate in `.githooks/pre-commit` are guarded
by `[ -f .indexnudge-rust.db ]`. Without the index (a fresh clone, an rsync
that skipped it, a deleted file) the enforced gate does not run and the commit
goes through. It used to do so silently, with `all gates green` as the
hook's last line; it now prints
`complexity gate SKIPPED: .indexnudge-rust.db not found, so nothing was
measured`.

The source still has that warning-and-pass branch. DG's ruling replaces
the policy for compiler-source commits; it does not claim the hook has
already changed. `SKIP_COMPLEXITY=1` controls only the report and must not
become an alternative gate bypass.

## Q2. Settled: refuse mismatched checked inputs

DG, 2026-11-09:

> refuse

DG selected refusal when checked files differ from staging, rather than
checking an isolated staged snapshot. AGENTS.md's "Hook snapshot
consistency" section is authoritative; journal 607 records the explanation
and choice. No automatic stash/restore is authorized.

The repository source/configuration inputs read by worktree-based checks
must match Git's staged snapshot. Compare whole relevant files, including
those not named in the staged diff: a changed dependency or another checked
module can affect the result without itself being part of the commit.

This is not limited to `sash/src/` or the complexity gate. The hook also
formats/lints `sash-convert` and `sash-expand`, runs the totality checker,
and consumes workspace/build configuration. Implementation must derive the
actual input scope from those checks and their dependencies, not invent
an incomplete source-only list. A newly added, deleted or renamed input,
including an untracked source/configuration file a check would consume,
must not evade the comparison.

Partial staging of a checked file is refused until the remaining edits
are staged or set aside by the contributor. Unrelated edits remain allowed.
Do not demand that local databases or generated build products be staged;
compare their source inputs and retain their normal freshness requirements.
Repository source/configuration files have no exemption merely because
they are generated or untracked.

A docs-only commit with relevant unstaged source edits still needs no
complexity index under Q1, but can fail this independent precondition.
The complexity-specific override cannot waive agreement for the other
checks. Existing global hook escapes are not redesigned; no new
consistency-specific bypass is selected.

Refusal must identify the mismatched inputs and explain that the contributor
must stage or set aside the edits. Do not run `git add`, stash, reset,
checkout or restoration on the contributor's behalf. The policy chooses
refusal, not an automatically manufactured clean worktree.

### Current implementation, unchanged by the ruling

The hook checks the worktree: formatting and Clippy use disk files,
the totality script reads them, and the shim freshens the source index
from them. The gate's hunks instead come from `git diff --cached`.
With unstaged edits in checked inputs:

- clippy and totality judge code that is not being committed;
- the gate can attribute a staged hunk to whichever function holds that line
  in the worktree.

The latest hook change is still `d8e6491e` at the recording baseline
`f22bd1e3`. Its worktree checks and staged hunk selection have not been
modified. The earlier note about manually stashing unrelated work was a
practice description, not authorization for the hook to automate it.

### Implementation and proof

Reject mismatches before worktree-based checks. Fail explicitly if the
comparison cannot establish agreement. Do not certify a new version on
the strength of an earlier comparison if checked inputs change during
the run; detected changes require refusal rather than a silent pass.
The path enumeration and change-detection mechanisms are implementation
work, not a selected new snapshot architecture.

Use an isolated fixture with the real hook's `set -euo pipefail` behavior,
not the shared development index/worktree. Before controls must show that
the current hook can check an unstaged repair while accepting the staged
unrepaired version. Cover partial staging, a checked input outside the
staged diff, and a line-shift mismatch; include dependency/configuration
and consumed-untracked-input cases when establishing the actual scope.
Assert the refusal and the relevant path, not just an arbitrary nonzero
exit from another gate.

Preserve matching-source pass/violation controls, unrelated unstaged edits,
generated artifacts, and the separate missing-index/override contracts.
Verify refusal leaves source bytes, staged blobs and stash unchanged.
Revert the consistency guard on the final implementation and confirm
the mismatch assertions go red again.

Coordinate with `require-complexity-index-for-compiler-commits`, without
inventing a hard dependency: the two guards can land independently or
together. Presence of an index does not prove snapshot agreement, and
snapshot agreement does not supply a missing index. Keep their proof
cases distinct so one refusal does not falsely stand in for the other.
