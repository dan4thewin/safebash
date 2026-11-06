title: Builtin stderr output should not mutate descriptors
status: complete

DG encountered SB5101 while reporting an error to stderr from ERR, then
asked why an already-open stderr needs descriptor backup/restore at all.
Copilot traced the current implementation: echo writes to hard-coded fd 1,
so the generic redirection wrapper saves fd 1, duplicates fd 2 onto it,
runs echo, and restores fd 1. ERR rejects the uncatchable backup/restore
operations even when the actual write is caught or suppressed.

DG's observation identifies the unnecessary mechanism. For a simple
builtin output destination, echo/write can write directly to fd 2, just
as they currently write to fd 1. No new stderr builtin or relaxation of
ERR's failure-handling contract is needed.

At DG's request, `bug-builtin-stderr-backup-restore` records the fix as an
open P2 bug. It covers both backends and both builtins, requires agreement
between lowering and SB5101's actual-effect analysis, and preserves the
semantics of other/compound redirects. The current CLI refusal and stdout
control were reproduced; implementation and its acceptance gates remain
pending.

Only backlog/status records changed. No compiler, runtime, spec or build
change; journal 577's accepted gate and spec-history boundary remain intact.
