# 556 - The index shim lost its stderr, and the hook stopped in silence

2026-11-01. An interrupt inside W1b-1, at DG's priority from the start of the
session: "let's get that back to a reliable state first - it's part of the
quality gate". The tree had been rsynced from another host and indexnudge
rebuilt.

## The shim discarded every sqlite3 error

In indexnudge's `bin/_indexnudge_common.bash` (`bin-rust/`'s copy is a
symlink to it), `lock_db_shared` opened its lock file with

```bash
exec 9>>"$lf" 2>/dev/null || return 0
```

An `exec` with only redirections applies them to the shell itself, so the
`2>/dev/null` was permanent. Every later message went to /dev/null, including
those of the real `sqlite3` the shim finally execs. A malformed query returned
rc 1 and no text.

The fix scopes the redirection to the `exec`:

```bash
{ exec 9>>"$lf"; } 2>/dev/null || return 0
```

Verified: a syntax error prints again; a normal query returns its rows with
rc 0; while a query runs, the `sqlite3` process holds the lock file on fd 9
and an exclusive `flock` on it is refused, then granted once it exits, so the
shared lock is still taken and released. No other `exec` redirection exists in
`bin/`.

**The copy here is not a git repository.** It came over with the rsync. The
fix has to be carried to indexnudge's canonical repo, or the next rsync
reverts it.

The index itself was sound: `files.lines` matches `wc -l` for all 58 files,
and symbols added since the rsync are indexed at their current lines.

Two claims of mine from the diagnosis are withdrawn:

- "The freshen step never runs." That was an artifact of calling
  `indexnudge/bin/sqlite3` directly. Through PATH (`~/bin/sqlite3`, which
  links to it), `indexnudge --freshen` runs under its exclusive lock as
  designed.
- The rc 1 that started the hunt was my own SQL: `BETWEEN a b` without the
  `AND`. The shim bug is what made it look like a tooling fault.

## The complexity gate stopped in silence

My first finding here was "the gate fails open on a query error", and it was
wrong. The negative control behind it sourced the gate block without the
hook's `set -euo pipefail`, so a failed `gate_out=$(sqlite3 …)` fell through
to `if [ -n "$gate_out" ]` and printed ok. Under the hook's real options the
failed assignment ends the hook with rc 1, and the commit is blocked.

What is true is that it stops without a word. The informational top-5 report
runs first, and its `sqlite3 … 2>/dev/null | sed` fails under pipefail, so the
hook ends after printing that block's header. Before the shim fix, even a
query without `2>/dev/null` would have been silent.

The fix (decide-and-report) leaves every verdict as it was:

- the report prints `report unavailable: the index query failed` and the hook
  continues;
- the gate's query failure prints sqlite3's own error, then `the complexity
  gate's index query failed (sqlite3 rc N), so it measured nothing`, and exits
  1.

One silent **pass** does exist. Both blocks are guarded by
`[ -f .indexnudge-rust.db ]`, so without the index the enforced gate is
skipped with no output and the commit goes through. The guard may be
deliberate (a clone without indexnudge can still commit), so the verdict is
unchanged. The hook now prints `complexity gate SKIPPED: .indexnudge-rust.db
not found, so nothing was measured`. Whether a missing index should block is
filed as q-complexity-gate-without-index, together with a second gap: the hook
checks the worktree while the gate's hunks come from the staged diff.

## Controls

Harness `/tmp/hookt`: a stub `git` that reports one staged hunk, a stub
`sqlite3` that prints an error and exits 1, and a driver that sources the hook
from the report block to the end under `set -euo pipefail`. The violator is
`absorb_text` (parse/expr.rs:118, cc 13 / cog 3); the small function is
ir.rs:1055.

| case | HEAD | now |
|---|---|---|
| failing query, violator | header, then nothing; rc 1 | report unavailable; sqlite3's error; gate names the failure; rc 1 |
| real index, violator | FAIL `absorb_text`; rc 1 | same |
| real index, small function | ok; rc 0 | same |
| no index | `all gates green`, nothing else; rc 0 | `SKIPPED … nothing was measured`; rc 0 |

The fail-open control ran first without `set -e` (HEAD: ok, rc 0). Repeating
it under the hook's options is what overturned the finding. When testing a
piece of a hook, run it under that hook's shell options.

## Observation

The gate trusts that `sqlite3` on PATH is the shim, which freshens the index;
the hook itself does not freshen. git hooks inherit the user's PATH, so here
it holds.
