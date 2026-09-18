#!/bin/bash
set -eu
cd "$(dirname "$0")/.."

sha=$(git rev-parse --short HEAD)
release=${SASH_RELEASE_DIR:-$(pwd)/target/release}
tsv=$(mktemp)
retired_f=$(mktemp)
trap 'rm -f "$tsv" "$retired_f"' EXIT

# A sweep retires any registered id it no longer produces (the purge below).
# More than this many in one sweep is read as a partial or aborted run rather
# than a deliberate retirement, and nothing is deleted. Raise it explicitly
# (SPEC_PURGE_MAX=N) when you really did retire a large cohort.
purge_cap=${SPEC_PURGE_MAX:-25}

# Bootstrap the database. spec.db is untracked local-derived data, so a
# fresh clone starts with no file at all; schema.sql and clauses.tsv are
# the tracked inputs that make it reproducible. Both steps are idempotent.
# clauses holds curated DD clause text (the one part of spec.db that is
# NOT derived from a test run), so it is loaded from the TSV rather than
# regenerated.
#
# _clause_import is created explicitly before .import: importing into a
# table that does not exist makes sqlite3 consume the first row as column
# names, which silently drops the first clause.
sqlite3 spec.db < spec/schema.sql
sqlite3 spec.db -cmd ".mode tabs" \
    "CREATE TEMP TABLE _clause_import(id TEXT, dd INTEGER, text TEXT);" \
    ".import spec/clauses.tsv _clause_import" \
    "INSERT OR IGNORE INTO clauses(id, dd, text) SELECT * FROM _clause_import;"


# Modes to run each spec file under. Override via SPEC_MODES env var
# (e.g. SPEC_MODES="native" for fast iteration).
MODES=${SPEC_MODES:-"native c"}

cd spec
for mode in $MODES; do
    echo "=== mode: $mode ==="
    for f in *.sh; do
        [[ $f = common.sh ]] && continue
        [[ $f = run.sh ]] && continue
        SPEC_TSV="$tsv" SASH_MODE="$mode" PATH="$release:$PATH" bash "$f" 2>/dev/null || true
    done
done
cd ..

# Load TSV into spec.db
if [[ -s $tsv ]]; then
    sqlite3 spec.db <<SQL
.mode tabs
CREATE TEMP TABLE _import(test_id TEXT, status TEXT, ts TEXT);
.import $tsv _import

-- Auto-register tests. The clause_id is the test id with the MODE: prefix and
-- any sub-test letter suffix stripped ('native:56.12a' -> '56.12'), which is
-- the scheme the clauses table is keyed by. Deriving it from the full prefixed
-- id (as this did before 2026-09-07) pointed every test at a clause that could
-- not exist, silently breaking DD traceability for all three modes.
INSERT OR IGNORE INTO tests(id, clause_id, description, kind)
SELECT test_id,
       rtrim(substr(test_id, instr(test_id, ':') + 1),
             'abcdefghijklmnopqrstuvwxyz'),
       '',
       'spec'
FROM _import
WHERE test_id NOT IN (SELECT id FROM tests);

-- Record runs
INSERT INTO runs(test_id, status, commit_hash, committed_at)
SELECT test_id, status, '$sha', ts FROM _import;

-- Retire ids this sweep no longer produces. The tests table is a permanent
-- registry and test_status LEFT JOINs from it, so a deleted spec kept
-- reporting its last status forever -- and the summary below reads that view,
-- so the gate's own total drifted upward with every retirement (journal 361:
-- 5585 reported against 5583 actually run, the ghosts being a 75.99 pair
-- deleted a commit earlier).
--
-- NOTE: this heredoc is unquoted, so bash expands it. Keep backticks out of
-- these comments entirely -- a backticked word is command substitution, not
-- quoting, and 2026-09-18 it printed "tests: command not found" four times
-- before anyone noticed the SQL was being rewritten on its way in.
--
-- Scoped to the modes that actually appear in this sweep, so
-- SPEC_MODES="native" cannot purge every c: id. Capped in the DELETE below,
-- because a partial sweep is indistinguishable from a mass retirement by
-- inspection of the TSV alone.
--
-- The runs rows are deliberately left in place: they are the cross-commit
-- history, and dropping the tests row is by itself what removes the id from
-- test_status. A re-added spec re-registers on its next sweep.
CREATE TEMP TABLE _retired AS
SELECT t.id AS id FROM tests t
WHERE instr(t.id, ':') > 0
  AND substr(t.id, 1, instr(t.id, ':') - 1) IN
      (SELECT DISTINCT substr(test_id, 1, instr(test_id, ':') - 1) FROM _import)
  AND t.id NOT IN (SELECT test_id FROM _import);

.headers off
.mode list
.output $retired_f
SELECT id FROM _retired ORDER BY id;
.output stdout

DELETE FROM tests
WHERE id IN (SELECT id FROM _retired)
  AND (SELECT COUNT(*) FROM _retired) <= $purge_cap;

DROP TABLE _retired;
DROP TABLE _import;
SQL

    nret=$(grep -c . "$retired_f" || true)
    if (( nret > purge_cap )); then
        echo "--- REFUSED to retire $nret ids (cap $purge_cap) ---"
        echo "    A partial sweep looks exactly like a mass retirement, so"
        echo "    nothing was deleted. Re-run the full suite; if the cohort is"
        echo "    genuinely retired, set SPEC_PURGE_MAX=$nret."
        head -20 "$retired_f" | sed 's/^/    /'
    elif (( nret > 0 )); then
        echo "--- retired $nret id(s) the suite no longer produces ---"
        sed 's/^/    /' "$retired_f"
    fi
fi

# Summary per mode — read the CURRENT status of each test (latest run per
# test_id, via the test_status view), NOT the raw row count at this sha.
# The old `count(*) … WHERE commit_hash=$sha` inflated permanently: every
# suite run appends a row per test, so a single broken/partial run left
# thousands of stale `failing` rows that persisted even after later runs
# passed those tests. test_status dedupes to the latest result.
sqlite3 spec.db "
SELECT substr(id, 1, instr(id, ':') - 1) AS mode, status, count(*)
FROM test_status
WHERE instr(id, ':') > 0
GROUP BY mode, status
ORDER BY mode, status
"
# List any tests whose latest result is failing (actionable, deduped).
fails=$(sqlite3 spec.db "SELECT id FROM test_status WHERE status='failing' ORDER BY id")
if [[ -n $fails ]]; then
    echo "--- currently failing (latest run per test) ---"
    echo "$fails"
fi
