#!/bin/bash
set -eu
cd "$(dirname "$0")/.."

sha=$(git rev-parse --short HEAD)
release=${SASH_RELEASE_DIR:-$(pwd)/target/release}
tsv=$(mktemp)
trap 'rm -f "$tsv"' EXIT

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

-- Auto-register tests (clause_id = test_id with trailing letters stripped)
INSERT OR IGNORE INTO tests(id, clause_id, description, kind)
SELECT test_id,
       rtrim(test_id, 'abcdefghijklmnopqrstuvwxyz'),
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

# Summary per mode
sqlite3 spec.db "
SELECT substr(test_id, 1, instr(test_id, ':') - 1) AS mode, status, count(*)
FROM runs
WHERE commit_hash='$sha'
GROUP BY mode, status
ORDER BY mode, status
"
