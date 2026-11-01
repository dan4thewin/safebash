---
title: "Question: what should the complexity gate do when it cannot measure the commit?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on Q1 (and Q2 if wanted); the hook and AGENTS.md's complexity section say what was ruled"
---

Found 2026-11-01 while repairing the indexnudge shim (journal 556). Since
that commit the hook *says* when the gate measured nothing; neither question
below changed a verdict.

## Q1. No index

Both the complexity report and the gate in `.githooks/pre-commit` are guarded
by `[ -f .indexnudge-rust.db ]`. Without the index (a fresh clone, an rsync
that skipped it, a deleted file) the enforced gate does not run and the commit
goes through. It used to do so silently, with `all gates green` as the
hook's last line; it now prints
`complexity gate SKIPPED: .indexnudge-rust.db not found, so nothing was
measured`.

- (a) Keep: warn and pass. A clone without indexnudge can still commit.
- (b) Block: exit 1 unless `SKIP_COMPLEXITY_GATE=1`, which already exists for
  a deliberate skip.

The guard may be deliberate, so it is unchanged until DG rules.

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
