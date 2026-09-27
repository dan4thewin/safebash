#!/usr/bin/env python3
"""Load an lcov file produced by contrib/coverage.sh into coverage.db.

    python3 contrib/covdb.py [LCOV] [DB]

Defaults to target/coverage/union.lcov -> ./coverage.db.  The union run is
the one to rank against: the 100% target is the UNION of spec- and unit-test
reachability, and the spec-only runs report the should-never-run ICE guards
cold because no source text reaches them.  ATTACH the indexnudge index and
join `symbols` to rank uncovered lines by function:

    sqlite3 coverage.db \
      "ATTACH '.indexnudge-rust.db' AS ix;
       SELECT s.file, s.start_line, s.name, COUNT(*) AS missed
         FROM cov_lines c JOIN ix.symbols s
           ON s.file = c.file AND c.line BETWEEN s.start_line AND s.end_line
        WHERE c.count = 0 AND s.kind IN ('function','method')
        GROUP BY s.file, s.start_line ORDER BY missed DESC LIMIT 30;"

Records are ACCUMULATED, not overwritten.  An lcov may carry more than one
entry for the same line — the same source compiled into several objects, or
repeated SF records for one file — and the merge lcov itself defines is a
sum.  This loader keyed cov_lines on (file,line) and wrote it with INSERT OR
REPLACE, so whichever entry happened to come last won; a trailing 0 erased a
real hit count silently and the line then read as dead code.

The same merge problem bit cov_fns separately and for longer, because its
key LOOKED unique.  A union lcov splices two builds -- the spec sweep's
binary and the unit-test harness -- and a Rust mangled name embeds the crate
DISAMBIGUATOR HASH, which differs between them.  So every function appeared
under two keys, one per build, and accumulating by mangled name could never
merge them: the build that did not reach a function contributed a second row
reading 0.  Function coverage printed 5135 recorded / 2076 uncovered (59.6%)
where the truth was 2928 / 113 (96.1%).  The hash is stripped before keying.

Do NOT "simplify" that to keying on (file, line) like cov_lines.  Several
distinct symbols legitimately share a start line -- a closure and the
function containing it, most often -- and merging them hides an uncovered
closure inside a covered parent.

lcov hit counts can exceed int64, so they are clamped to 2**62 after summing.
"""
import os
import re
import sqlite3
import sys

CLAMP = 2**62
ROOT = "/mnt/dg/src/safebash/"

# v0 crate disambiguator, then the legacy-mangling trailing hash.
_CS = re.compile(r"Cs[0-9A-Za-z]+_")
_LEGACY = re.compile(r"17h[0-9a-f]{16}E$")


def demangle_key(m):
    """Mangled name with every build-specific hash removed."""
    return _LEGACY.sub("", _CS.sub("", m))

lcov = sys.argv[1] if len(sys.argv) > 1 else "target/coverage/union.lcov"
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
line_hits, branch_hits = {}, {}
fn_hits, fn_line = {}, {}
for ln in open(lcov):
    ln = ln.rstrip("\n")
    if ln.startswith("SF:"):
        sf = ln[3:].replace(ROOT, "")
    elif ln.startswith("FN:"):
        n, m = ln[3:].split(",", 1)
        fn_line[demangle_key(m)] = int(n)
    elif ln.startswith("FNDA:"):
        n, m = ln[5:].split(",", 1)
        k = (sf, demangle_key(m))
        fn_hits[k] = fn_hits.get(k, 0) + int(n)
    elif ln.startswith("DA:"):
        n, cnt = ln[3:].split(",")[:2]
        k = (sf, int(n))
        line_hits[k] = line_hits.get(k, 0) + int(cnt)
    elif ln.startswith("BRDA:"):
        n, block, branch, taken = ln[5:].split(",")
        k = (sf, int(n), int(block), branch)
        branch_hits[k] = branch_hits.get(k, 0) + (0 if taken == "-" else int(taken))

c.executemany("INSERT INTO cov_lines VALUES(?,?,?)",
              [(f, n, min(v, CLAMP)) for (f, n), v in line_hits.items()])
c.executemany("INSERT INTO cov_fns VALUES(?,?,?,?)",
              [(f, fn_line.get(m, 0), m, min(v, CLAMP))
               for (f, m), v in fn_hits.items()])
c.executemany("INSERT INTO cov_branches VALUES(?,?,?,?,?)",
              [(f, n, b, br, min(v, CLAMP))
               for (f, n, b, br), v in branch_hits.items()])
c.commit()

for label, q in (
    ("lines", "SELECT COUNT(*), SUM(count=0) FROM cov_lines"),
    ("branches", "SELECT COUNT(*), SUM(count=0) FROM cov_branches"),
    ("functions", "SELECT COUNT(*), SUM(count=0) FROM cov_fns"),
):
    total, missed = c.execute(q).fetchone()
    print(f"{label:10} {total or 0:6} recorded, {missed or 0:5} uncovered")
