title: Refuse hook checks against a different source snapshot
date: 2026-11-09

## DG's decision

After the missing-index policy recording, DG asked for an explanation
of staged versus worktree checking. Git commits its staged snapshot;
editing a file after `git add` changes disk contents without updating
that snapshot. The current hooks read disk contents.

The AI explained that a hook can accept an unstaged repair while Git
commits the unrepaired version, or reject a valid staged version because
of later edits. Complexity has a further mismatch: staged line ranges
are compared with worktree function locations and metrics.

The alternatives presented were checking an isolated staged snapshot or
refusing when checked files differ from staging. Automatic stash/restore
was also described, with its restoration/conflict risks.

DG answered:

> refuse

This selects refusal, not snapshot construction or automatic stashing.
As with the preceding decisions, this is policy/backlog recording,
not authorization to implement the hook change.

## Apply the rule to what the hooks read

The repository source/configuration inputs to worktree-based checks must
match the staged snapshot. A comparison only of staged hunks or staged
filenames would miss a changed dependency, another checked module or an
unstaged configuration change. Whole relevant files must agree.

Formatting, lint, totality and complexity all follow the rule, not just
compiler complexity. Consumed untracked source/configuration is not
exempt merely because it has not entered Git yet. The implementation
must enumerate actual inputs and dependencies; no speculative fixed
path list was selected.

This is not a blanket clean-worktree requirement. Unrelated work can
remain unstaged; local generated indexes, build outputs and result databases
need not be staged. Their source inputs and existing freshness requirements
still matter. Repository source/configuration files are not exempt merely
because they are generated or untracked.

Partial staging of a checked file requires staging or setting aside the
remaining edits first. Refusal identifies the inputs without changing
the worktree, staging area or stash. The contributor chooses what to
stage or set aside; the hook must not do it automatically.

The consequence for docs-only commits was reported explicitly: they
need no complexity index under the preceding policy, but can still be
refused if their hooks would read unstaged compiler edits. That is a
separate check, not a reversal of the missing-index scope.

The existing complexity override stays complexity-specific. It does
not waive agreement for other checks. No new bypass or alteration of
the existing global hook escapes was selected.

## Ownership and evidence

The tracked recording baseline is clean main at `f22bd1e3`.
`.githooks/pre-commit` last changed in `d8e6491e`; the current source
inspection from the preceding recording remains applicable. Its checks
operate on worktree files, and lines 139-154 select staged hunk ranges.
No new behavior probe or hook-control run accompanies this decision.

AGENTS.md gains a shared hook-snapshot section rather than burying this
cross-check rule in the complexity rubric. The existing
`q-complexity-gate-without-index` retains its slug, open status and P3
priority, changing from question to work. It now owns Q2 enforcement;
both policy questions are answered, but both implementations remain
queued. Reusing the record is the AI's reversible organization choice.

`require-complexity-index-for-compiler-commits` retains Q1 enforcement.
The two tasks need no invented dependency. Their controls must distinguish
the guards: a snapshot mismatch must not supply a false proof of the
missing-index refusal, or vice versa.

The queued proof covers actual mismatched inputs, unchanged user work,
matching-source and unrelated-artifact controls, failures to establish
agreement, and detected input changes during checks. Run under the real
hook's shell options in an isolated fixture, with before/reversion evidence.
No new harness or comparison mechanism is selected here.

Only AGENTS.md, the two task records, this journal and status change.
No hook/compiler/runtime/converter/spec/build implementation, spec sweep
or fresh coverage measurement. GUIDE needs no architecture revision.
