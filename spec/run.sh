#!/bin/bash
set -eu
cd "$(dirname "$0")/.."

sha=$(git rev-parse --short HEAD)
release=$(pwd)/target/release
tsv=$(mktemp)
trap 'rm -f "$tsv"' EXIT

cd spec
for f in *.sh; do
    [[ $f = common.sh ]] && continue
    [[ $f = run.sh ]] && continue
    SPEC_TSV="$tsv" PATH="$release:$PATH" bash "$f" 2>/dev/null || true
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

# Summary
sqlite3 spec.db "
SELECT status, count(*) FROM runs
WHERE commit_hash='$sha'
GROUP BY status
"
