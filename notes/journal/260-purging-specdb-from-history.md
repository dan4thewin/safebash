# 260 — purging spec.db from history, and what fell out of it

DG: *"The spec.db is meant to facilitate local development. It really never
should have been added to the repo. Purge it and all its history."*

Straightforward on its face. The interesting part was not the rewrite — that
took four seconds — but the two things the rewrite broke that nobody had
noticed were load-bearing.

## The measurement

| | |
|---|---|
| commits carrying `spec.db` | 455 of 1001 |
| blobs | 10,791 |
| pack cost | **9.0 GB** |
| `.git` on disk | 1.7 GB |

Every spec run rewrites the database wholesale, so each commit that touched
it added a fresh ~20 MB blob that deltas poorly against its predecessor. The
file was 79 MB; its history was two orders of magnitude larger.

`notes/misses.md` went with it — a single-commit scratch capture superseded by
`notes/tasks/`.

## The rewrite

`git filter-repo --path spec.db --path notes/misses.md --invert-paths
--replace-refs delete-no-add --force`, over all six branches.

`.git`: **1.7 GB → 5.7 MB**. A fresh clone from GitHub is 7.3 MB, and the
largest blob in it is now 292 KB of `codegen.rs`.

Two wrinkles worth recording:

- There was **stale filter-repo metadata from an April run** in
  `.git/filter-repo/`. Treating this run as a "continuation" would have
  composed the two mappings, keying the new commit-map on *pre-April* hashes —
  useless for repairing citations that use current hashes. Archived it to the
  backup and ran fresh.
- `--replace-refs delete-no-add` matters. Replace refs would keep the old
  objects reachable, which is exactly the space you are trying to reclaim.

Three commits were pruned as empty; all three touched only `spec.db`.

## Fallout 1: 431 dangling citations

The notes are a knowledge graph and they cite commits — "removed in commit
73caecf". Every hash from the first `spec.db` commit onward changed, so all of
them would have dangled.

filter-repo rewrites hashes inside **commit messages** itself. It does not
touch **file contents**, which is where ours live.

The repair was mechanical but wanted care about what counts as a hash. Rather
than regex-replacing anything that looks hex, I snapshotted — *before* the
rewrite — every 7-to-40-char hex token in tracked prose that `git cat-file`
confirmed resolved to a real commit. 238 distinct hashes, 431 sites, 122
files. Only those exact tokens were remapped, then re-verified to resolve
afterward, and checked for 7-char ambiguity in the new history. Zero
ambiguous, zero unmapped.

One needed judgement: `9af7ce2` was a `spec.db`-only commit, pruned as empty,
so it has no successor. It now points at its parent `58f0f23`, which carries
the identical tree once `spec.db` is removed.

`spec.db`'s own `runs` table records a `commit_hash` per run — 303 distinct
values, 1.46M rows — and got the same treatment. One value, `0961a2e`, would
not map. It turned out to be **already dangling before the rewrite**: absent
from the pre-rewrite history too, an amended or rebased commit from
2026-08-21. Not something the purge broke.

## Fallout 2: spec.db was not purely derived data

This is the part I should have checked before force-pushing, and did not.

Deleting `spec.db` from history is only correct if it is reproducible. Two
reasons it wasn't:

1. **`run.sh` never created the schema.** It only ever `INSERT`s. The tables
   came from the `spec.db` that arrived with the clone. With the file
   untracked, a fresh clone fails with `no such table: tests`. I had already
   written — and committed — an AGENTS.md claim that a fresh clone could
   rebuild it from scratch. That claim was false when I wrote it.
2. **The `clauses` table is curated, not derived.** 231 rows, 15 KB of
   hand-written DD clause text that nothing in the repo regenerates.
   `status.md` had flagged this back in September: the clause texts "exist
   only inside the 68 MB binary spec.db". Purging the file made 15 KB of
   irreplaceable prose single-machine.

The 9.0 GB was all `runs`, which *is* derived. The curated remainder just
needed a better home than a binary blob.

DG chose to track it as text. Now:

- `spec/clauses.tsv` — 231 rows, 17 KB, diffable, reviewable in a PR, which
  is what this data always wanted to be.
- `spec/schema.sql` — tables, views, indexes, all `IF NOT EXISTS`.
- `run.sh` applies both, idempotently, before recording anything.

Verified by building a `spec.db` from nothing in a scratch directory: 231
clauses, byte-identical round-trip against the source database, and stable
across a second bootstrap.

That test caught a bug in my first attempt. `.import` into a table that does
**not** exist makes sqlite3 consume the first row as column names — the
bootstrap silently loaded 230 clauses instead of 231, eating clause `5.1`.
Creating the temp table explicitly first fixes it. A count check caught what a
"did it run without error" check would not have.

## A side benefit: the regression gate got usable

The old cross-commit check was `git show HEAD:spec.db > /tmp/h.db`, then
ATTACH and compare. That recipe dies with the purge.

The replacement was already latent in the schema: `runs` carries
`commit_hash`, so "what changed since commit X" is a plain query. But `runs`
is 1.46M rows with **no indexes**, and both that query and the `test_status`
view do a correlated `MAX(rowid)` lookup per test. The first run took about
four minutes.

Two indexes, on `runs(test_id)` and `runs(commit_hash, test_id)`:

**~4 minutes → 9 milliseconds.** They cost ~95 MB of local disk, which is
free now that the file is not in git — the purge is what made this affordable.

I checked the query actually detects flips rather than just returning empty
quickly: against an old baseline it correctly reports `c:16.2` going
`failing → passing`, matching that test's recorded history.

## What I'd do differently

Ask "is this file purely derived?" *before* rewriting history, not after. The
purge was right and the numbers vindicate it, but I established
reproducibility only when DG's answer to a question I raised late forced me
to. Had `spec.db` existed on this machine alone, the clause text would have
been recoverable only from a backup I happened to take.

The backup is the reason that risk stayed theoretical. Full mirror bundle
(verified, 11 refs), raw `.git` tarball, and a copy of the live database,
written before anything was touched.

## Totals

4616 passing / 39 failing / 459 skipped — unchanged across the rewrite, and
the database that reports it now survives a fresh clone.
