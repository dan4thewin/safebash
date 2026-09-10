#!/bin/bash
set -eu
cd "$(dirname "$0")/.."

sha=$(git rev-parse --short HEAD)
release=${SASH_RELEASE_DIR:-$(pwd)/target/release}
tsv=$(mktemp)
trap 'rm -f "$tsv"' EXIT

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
MODES=${SPEC_MODES:-"native janet c"}

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

DROP TABLE _import;
SQL
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
