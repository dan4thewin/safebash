-- spec.db schema.
--
-- spec.db is untracked local-derived data (gitignored). This file plus
-- clauses.tsv is what makes it reproducible: run.sh applies both before
-- recording results, so a fresh clone can build the database from nothing.
--
-- Everything here is idempotent; run.sh applies it on every run.

CREATE TABLE IF NOT EXISTS clauses (
    id TEXT PRIMARY KEY,
    dd INTEGER NOT NULL,
    text TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tests (
    id TEXT PRIMARY KEY,
    clause_id TEXT NOT NULL REFERENCES clauses(id),
    description TEXT NOT NULL,
    kind TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
    test_id TEXT NOT NULL REFERENCES tests(id),
    status TEXT NOT NULL,
    committed_at TEXT NOT NULL,
    commit_hash TEXT NOT NULL
);

-- runs is append-only and large (>1.4M rows). Without these, the
-- cross-commit regression query in AGENTS.md takes minutes, because both
-- it and the test_status view do a correlated MAX(rowid) lookup per test.
CREATE INDEX IF NOT EXISTS idx_runs_test ON runs(test_id);
CREATE INDEX IF NOT EXISTS idx_runs_commit_test ON runs(commit_hash, test_id);

-- Current status of each test: the latest run per test_id, or 'planned'
-- if it has never run. Do NOT count raw runs rows instead - every suite
-- run appends a row per test, so raw counts inflate permanently.
CREATE VIEW IF NOT EXISTS test_status AS
SELECT t.id, t.clause_id, t.description, t.kind,
  COALESCE(r.status, 'planned') AS status,
  r.commit_hash, r.committed_at
FROM tests t
LEFT JOIN runs r ON r.test_id = t.id
  AND r.rowid = (SELECT MAX(rowid) FROM runs WHERE test_id = t.id);

-- Every point at which a test changed status from its previous run.
CREATE VIEW IF NOT EXISTS flips AS
SELECT r1.test_id, r1.status, r1.commit_hash, r1.committed_at
FROM runs r1
LEFT JOIN runs r2 ON r2.test_id = r1.test_id AND r2.rowid = (
    SELECT MAX(rowid) FROM runs WHERE test_id = r1.test_id AND rowid < r1.rowid
)
WHERE r2.status IS NULL OR r2.status != r1.status;
