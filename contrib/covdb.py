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

Staleness is recorded, not remembered.  Every row is keyed by (file, line)
and nothing in an lcov says which revision it measured, so one commit that
renumbers a measured file turns "line 2056 is cold" into a line that today
holds an enum variant.  Measured after 18 such commits: 184 of 519 cold lines
-- 35% -- pointed at a blank line, a comment or a bare brace, text that cannot
carry a DA record at all.  The aggregate percentages survive that (519/27538
does not care which line is which); the per-line attribution does not, and the
per-line attribution is the entire reason to rank.

So the loader stores the HEAD commit and the WORKTREE blob hash of every
measured file in `cov_meta` / `cov_blobs`, and

    python3 contrib/covdb.py --check      # or: mk cov-stale

re-hashes them and reports the ones that have moved, exiting nonzero if any
have.  Clean means the per-line data still describes the tree in front of you.
This replaces a prose warning in plan.md that said "do not read it", which
failed exactly the way prose warnings fail.

A VIEW cannot do this job: staleness is a fact about the working tree, which
SQLite cannot see.  Hashing is deliberately of the worktree rather than the
index or HEAD, so an uncommitted edit counts -- coverage is measured against
whatever was on disk.

Records DEFINED inside a `#[cfg(test)]` module are dropped.  `coverage.sh
--lcov` already excludes `tests.rs` and `/tests/` by filename, but the
dominant unit-test shape in this repo is a `#[cfg(test)] mod` living inside a
production file -- AGENTS.md asks for it explicitly, because most of the
functions under test are private.  Those modules are test code; counting their
cold lines as production misses displaces real candidates from the head of the
by-function ranking.  `ice_text` sat there with 4 uncovered lines on the
strength of a downcast arm no ICE payload takes.

This drops lines *defined* in such a module.  It deliberately does NOT change
which RUNS count: a unit test warming a production line is the entire point of
`mk cov-union`, and that still happens.

The range is found by scanning for `#[cfg(test)]` followed by `mod NAME {`,
closing at the next column-0 `}`.  Brace counting would be wrong here -- the
ICE tests are full of `panic!("... {inst:?}")` -- whereas a `}` in a string
literal is never alone at column 0.  Verified against all 24 such modules in
the tree.

lcov hit counts can exceed int64, so they are clamped to 2**62 after summing.
"""
import os
import re
import sqlite3
import subprocess
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

def git(*a):
    """git output, or "" if git is unavailable or the call fails."""
    try:
        return subprocess.run(("git",) + a, capture_output=True, text=True,
                              check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return ""


_CFG_TEST = re.compile(r"^#\[cfg\(test\)\]\s*$")
_MOD_OPEN = re.compile(r"^mod\s+\w+\s*\{\s*$")

_ranges_cache = {}


def cfg_test_ranges(relpath):
    """Inclusive line ranges of every `#[cfg(test)] mod` in a file.

    Returns [] for a file that cannot be read, so a missing or generated
    source degrades to "exclude nothing" rather than crashing the load.
    """
    if relpath in _ranges_cache:
        return _ranges_cache[relpath]
    out = []
    try:
        with open(os.path.join(ROOT, relpath)) as fh:
            lines = fh.read().split("\n")
    except OSError:
        _ranges_cache[relpath] = out
        return out
    i = 0
    while i < len(lines):
        if _CFG_TEST.match(lines[i]) and i + 1 < len(lines) and _MOD_OPEN.match(lines[i + 1]):
            start = i + 1  # 1-based, the attribute line
            j = i + 2
            while j < len(lines) and lines[j] != "}":
                j += 1
            out.append((start, j + 1))
            i = j + 1
        else:
            i += 1
    _ranges_cache[relpath] = out
    return out


def in_cfg_test(relpath, line):
    return any(a <= line <= b for a, b in cfg_test_ranges(relpath))


def hash_worktree(files):
    """blob hash of each file AS IT IS ON DISK (not the index, not HEAD)."""
    out = git("hash-object", "--", *files).split()
    return dict(zip(files, out)) if len(out) == len(files) else {}


def check(db):
    """Report measured files whose content has moved since the lcov was taken."""
    if not os.path.exists(db):
        sys.exit(f"{db}: absent -- run `mk cov-union && mk cov-db`")
    con = sqlite3.connect(db)
    try:
        was = dict(con.execute("SELECT file, blob FROM cov_blobs"))
        meta = dict(con.execute("SELECT key, value FROM cov_meta"))
    except sqlite3.OperationalError:
        sys.exit(f"{db}: predates provenance tracking -- rebuild with `mk cov-db`")
    if not meta.get("commit"):
        print(f"{db}: provenance UNKNOWN -- built from an lcov older than the "
              "commit it was loaded at.")
        print("  Per-line data is unusable. Rebuild: `mk cov-union && mk cov-db`.")
        sys.exit(1)
    now = hash_worktree(sorted(was))
    moved = sorted(f for f, b in was.items() if now.get(f, b) != b)
    print(f"{db}: measured at {meta.get('commit', '?')[:12]} "
          f"({meta.get('built_at', '?')}), {len(was)} files")
    if not moved:
        print("  current -- per-line data describes the working tree")
        return
    print(f"  STALE: {len(moved)} of {len(was)} measured files have changed.")
    print("  Aggregate percentages still hold; per-line attribution does NOT.")
    for f in moved[:20]:
        print(f"    {f}")
    if len(moved) > 20:
        print(f"    ... and {len(moved) - 20} more")
    sys.exit(1)


if "--check" in sys.argv:
    check(sys.argv[sys.argv.index("--check") + 1]
          if len(sys.argv) > sys.argv.index("--check") + 1 else "coverage.db")
    sys.exit(0)

if os.path.exists(db):
    os.remove(db)
c = sqlite3.connect(db)
c.executescript("""
CREATE TABLE cov_lines(file TEXT, line INT, count INT, PRIMARY KEY(file,line));
CREATE TABLE cov_fns(file TEXT, line INT, mangled TEXT, count INT);
CREATE TABLE cov_branches(file TEXT, line INT, block INT, branch TEXT, count INT);
CREATE TABLE cov_meta(key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE cov_blobs(file TEXT PRIMARY KEY, blob TEXT);
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

line_rows = [(f, n, min(v, CLAMP)) for (f, n), v in line_hits.items()
             if not in_cfg_test(f, n)]
fn_rows = [(f, fn_line.get(m, 0), m, min(v, CLAMP)) for (f, m), v in fn_hits.items()
           if not in_cfg_test(f, fn_line.get(m, 0))]
branch_rows = [(f, n, b, br, min(v, CLAMP)) for (f, n, b, br), v in branch_hits.items()
               if not in_cfg_test(f, n)]
dropped = (len(line_hits) - len(line_rows), len(fn_hits) - len(fn_rows),
           len(branch_hits) - len(branch_rows))

c.executemany("INSERT INTO cov_lines VALUES(?,?,?)", line_rows)
c.executemany("INSERT INTO cov_fns VALUES(?,?,?,?)", fn_rows)
c.executemany("INSERT INTO cov_branches VALUES(?,?,?,?,?)", branch_rows)

# The lcov does not record what it measured, so HEAD is only the right answer
# when the lcov was produced from it -- the documented flow is
# `mk cov-union && mk cov-db`.  Rebuilding the db from an OLD lcov would
# otherwise stamp stale data with today's commit, which is worse than no
# provenance at all: a confident wrong answer.  Detect it by clock.
measured = sorted({f for f, _ in line_hits})
head = git("rev-parse", "HEAD").strip()
head_at = git("log", "-1", "--format=%ct").strip()
lcov_at = int(os.path.getmtime(lcov))
attributable = head_at.isdigit() and lcov_at >= int(head_at)
if not attributable:
    print(f"warning: {lcov} predates HEAD -- provenance recorded as unknown.")
    print("         Rebuild with `mk cov-union && mk cov-db` to restore it.")
c.executemany("INSERT INTO cov_meta VALUES(?,?)", [
    ("commit", head if attributable else ""),
    ("built_at", git("log", "-1", "--format=%cI").strip() if attributable else ""),
    ("lcov", lcov),
    ("lcov_mtime", str(lcov_at)),
])
c.executemany("INSERT INTO cov_blobs VALUES(?,?)", hash_worktree(measured).items())
c.commit()

for label, q in (
    ("lines", "SELECT COUNT(*), SUM(count=0) FROM cov_lines"),
    ("branches", "SELECT COUNT(*), SUM(count=0) FROM cov_branches"),
    ("functions", "SELECT COUNT(*), SUM(count=0) FROM cov_fns"),
):
    total, missed = c.execute(q).fetchone()
    print(f"{label:10} {total or 0:6} recorded, {missed or 0:5} uncovered")
print(f"excluded   {dropped[0]:6} lines, {dropped[2]} branches, {dropped[1]} functions"
      " defined in #[cfg(test)] modules")
