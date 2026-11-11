# Retire the root Goose runners

**Date:** 2026-11-11

DG requested removing the old root task runners and `.goosehints`.

Removed `run-compiler-tasks.sh` and `run-converter-tasks.sh`. These were
May 2026 Goose batch drivers: sequential per-task branches, selected spec
gates and automatic commits/merges into the hard-coded `split-converter`
branch. The compiler runner still named deleted source and test paths.

`.goosehints` was only a two-line pointer to `AGENTS.md`; `AGENTS.md` remains.
No runner was executed, and no historical agent logs or branches were
removed. The deleted files remain available in Git history.

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
