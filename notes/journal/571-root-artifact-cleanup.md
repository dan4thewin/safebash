# 571 - Root artifact cleanup

2026-11-05. DG found stray files in the repository root and confirmed the
seven-file cleanup scope. I archived every selected file before removal,
including the ignored files, and left the active session plan alone.

| Files | What they were | Git state before cleanup |
|---|---|---|
| `c.sh` | A parser reproducer | Tracked |
| `cfp` | Unrelated conference submission text | Tracked |
| The two `report.20260906.*.json` files | Copilot CLI Node OOM reports | Tracked |
| `a.out` | An old static executable | Ignored |
| `e.txt` | A failed shell invocation's error output | Ignored |
| Root `plan.md` | Obsolete working notes from October 17 | Ignored |

DG asked where the Node reports came from. Their headers identify
`copilot`, not sash, exhausting Node's roughly 4 GiB heap limit on
September 6 at 00:57:56 and 01:35:03 UTC. Both reports entered Git with
`570f2cd6`, the September 8 DD96 commit. They include environment/process
sections; those values are not copied into these notes.

A root-scoped Node report filename rule now prevents their ordinary
accidental re-addition. It does not ignore arbitrary JSON or reports in
subdirectories. No history rewrite is part of this cleanup.

## Preserved evidence and scope

The private session archive `files/root-cleanup-20261105.tar.gz` contains
all seven original files, 58,171 uncompressed bytes. Every member was
byte-compared before removal. Both archive and companion manifest have
mode 0600. Archive SHA256:

`011ba1fb4fef42513fad584a669458769dbff42090cb9aa4b7f0f382f601ced4`

The companion `files/root-cleanup-20261105.json` records original names,
modes, tracked state and hashes. The archive is outside the repository and
is not committed.

The databases (`spec.db`, `coverage.db`, the code index), `Cargo.lock`,
build/scratch directories, task-runner scripts and detached reference
worktrees remain. No compiler, runtime, spec or build rule changes.
The cleanup checks file scope, archive integrity and positive/negative
ignore matches; normal commit hooks still apply. The accepted source gate
and spec-history boundary remain unchanged. Coverage is still paused.
