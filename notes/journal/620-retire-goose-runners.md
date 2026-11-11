# Retire the root Goose runners

**Date:** 2026-11-11

DG requested removing the old root task runners and `.goosehints`, then
also requested removing `.agent-runs`.

Removed `run-compiler-tasks.sh` and `run-converter-tasks.sh`. These were
May 2026 Goose batch drivers: sequential per-task branches, selected spec
gates and automatic commits/merges into the hard-coded `split-converter`
branch. The compiler runner still named deleted source and test paths.

`.goosehints` was only a two-line pointer to `AGENTS.md`; `AGENTS.md` remains.
No runner was executed, and no historical agent branches were removed.
The three deleted tracked files remain available in Git history.

`.agent-runs` held 102 ignored, untracked files totalling 2,032,318 bytes,
last modified in May 2026. A private session archive preserves them;
every file's contents and the complete directory membership were compared
before removing the repository directory. Its ignore rule remains to
prevent accidental staging of old agent logs.

The pending delegate-management skill task no longer prescribes the
retired runner. Its goal stays open, with the experimental runner to be
selected when that work is undertaken. Historical journal and completed
task references are retained.

Indexed source and the current mk/gate and Cargo entry points have no
references to the removed files. No compiler, runtime, spec or build
source changed; no full spec sweep or coverage measurement accompanies
this cleanup.

The existing README/licensing draft content is unchanged and remains
outside the cleanup commit.
