title: "Complexity gate: staged snapshot remains open; missing-index policy settled"
status: open
priority: P3
kind: question
depends: []
gates:
  - "Q1's accepted missing-index policy is implemented by require-complexity-index-for-compiler-commits; this question record does not duplicate that work"
  - "DG rules on Q2; any resulting hook and contributor-policy changes state which snapshot is checked without silently discarding or stashing user work"

Found 2026-11-01 while repairing the indexnudge shim (journal 556). Since
that commit the hook *says* when the gate measured nothing. Q1 was settled
as policy on 2026-11-09; enforcement is queued, not shipped. Q2 remains open.
Journal 606 records the new ruling; journal 556's controls remain historical
measurements, not a rerun at the decision date.

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
which contents the checks should measure is still Q2.

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

## Q2. Worktree versus staged snapshot

The hook checks the worktree: `cargo fmt --check` and `cargo clippy` compile
it, the totality script reads it, and the shim freshens the index from it. The
gate's hunks, though, come from `git diff --cached`. With unstaged edits in
`sash/src`:

- clippy and totality judge code that is not being committed;
- the gate can attribute a staged hunk to whichever function holds that line
  in the worktree.

Today's practice is to stash unrelated work before committing. The usual
mechanical alternative is `git stash --keep-index` around the checks, which
has its own failure modes (conflicts on restore, hooks that themselves edit).

Q1 does not choose automatic stashing, an isolated staged-tree check, or a
restriction on partial staging. Resolving Q2 is not a prerequisite for the
missing-index refusal, and the refusal does not establish that the measured
worktree equals the committed snapshot.
