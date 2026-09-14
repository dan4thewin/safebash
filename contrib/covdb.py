#!/usr/bin/env python3
"""Load an lcov file produced by contrib/coverage.sh into coverage.db.

    python3 contrib/covdb.py [LCOV] [DB]

Defaults to /tmp/covdata/branch.lcov -> ./coverage.db.  ATTACH the
indexnudge index and join `symbols` to rank uncovered lines by function:

    sqlite3 coverage.db \
      "ATTACH '.indexnudge-rust.db' AS ix;
       SELECT s.file, s.start_line, s.name, COUNT(*) AS missed
         FROM cov_lines c JOIN ix.symbols s
           ON s.file = c.file AND c.line BETWEEN s.start_line AND s.end_line
        WHERE c.count = 0 AND s.kind IN ('function','method')
        GROUP BY s.file, s.start_line ORDER BY missed DESC LIMIT 30;"

lcov hit counts can exceed int64, so they are clamped to 2**62.
"""
import os
import sqlite3
import sys

CLAMP = 2**62
ROOT = "/mnt/dg/src/safebash/"

lcov = sys.argv[1] if len(sys.argv) > 1 else "/tmp/covdata/branch.lcov"
db = sys.argv[2] if len(sys.argv) > 2 else "coverage.db"

if os.path.exists(db):
    os.remove(db)
c = sqlite3.connect(db)
c.executescript("""
CREATE TABLE cov_lines(file TEXT, line INT, count INT, PRIMARY KEY(file,line));
CREATE TABLE cov_fns(file TEXT, line INT, mangled TEXT, count INT);
CREATE TABLE cov_branches(file TEXT, line INT, block INT, branch TEXT, count INT);
CREATE INDEX idx_cl ON cov_lines(file, count);
CREATE INDEX idx_cb ON cov_branches(file, count);
""")

sf = None
lines, branches = [], []
fn_hits, fn_line = {}, {}
for ln in open(lcov):
    ln = ln.rstrip("\n")
    if ln.startswith("SF:"):
        sf = ln[3:].replace(ROOT, "")
    elif ln.startswith("FN:"):
        n, m = ln[3:].split(",", 1)
        fn_line[m] = int(n)
    elif ln.startswith("FNDA:"):
        n, m = ln[5:].split(",", 1)
        fn_hits[(sf, m)] = min(int(n), CLAMP)
    elif ln.startswith("DA:"):
        n, cnt = ln[3:].split(",")[:2]
        lines.append((sf, int(n), min(int(cnt), CLAMP)))
    elif ln.startswith("BRDA:"):
        n, block, branch, taken = ln[5:].split(",")
        cnt = 0 if taken == "-" else min(int(taken), CLAMP)
        branches.append((sf, int(n), int(block), branch, cnt))

c.executemany("INSERT OR REPLACE INTO cov_lines VALUES(?,?,?)", lines)
c.executemany("INSERT INTO cov_fns VALUES(?,?,?,?)",
              [(f, fn_line.get(m, 0), m, n) for (f, m), n in fn_hits.items()])
c.executemany("INSERT INTO cov_branches VALUES(?,?,?,?,?)", branches)
c.commit()

for label, q in (
    ("lines", "SELECT COUNT(*), SUM(count=0) FROM cov_lines"),
    ("branches", "SELECT COUNT(*), SUM(count=0) FROM cov_branches"),
    ("functions", "SELECT COUNT(*), SUM(count=0) FROM cov_fns"),
):
    total, missed = c.execute(q).fetchone()
    print(f"{label:10} {total or 0:6} recorded, {missed or 0:5} uncovered")
