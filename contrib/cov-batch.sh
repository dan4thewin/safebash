#!/bin/bash
# contrib/cov-batch.sh — which of the COLD lines does this corpus warm?
#
# `cov-probe.sh` answers "does this snippet reach that line" for one line. The
# inner loop of the coverage queue asks the inverse and in bulk: I have thirty
# candidate spellings aimed at forty cold lines — which ones landed?  Probing
# one line at a time costs an lcov render per line (seconds each, and the
# corpus is re-run every time).
#
# This runs the corpus ONCE, renders ONE lcov, and reports every line that was
# cold in coverage.db and is warm now. Anything it does not list is still cold,
# so a candidate spelling that missed is visible immediately — before paying
# for a full `mk cov-union` sweep.
#
# Usage:
#   contrib/cov-probe.sh --build          # once: instrument target/release
#   contrib/cov-batch.sh CORPUS_DIR [FILE_FILTER]
#   contrib/cov-batch.sh --spec SPEC_DIR [FILE_FILTER]
#
# CORPUS_DIR holds one sash program per file (any extension). With `--spec`,
# SPEC_DIR holds spec FILES to run under the harness instead — the end-to-end
# check that the spec you actually wrote warms the line your snippet did.
# FILE_FILTER is an optional SQL LIKE pattern restricting the report to one
# source file, e.g. 'sash/src/validate.rs'.
#
# Remember `mk cov-reset` when done — target/release stays instrumented.
set -eu
cd "$(dirname "$0")/.."

IGNORE='(sash-rt|sash-convert|sash-expand|/tests/|tests\.rs|main\.rs)'
spec_mode=""
if [ "${1:-}" = "--spec" ]; then spec_mode=1; shift; fi
corpus="${1:?usage: cov-batch.sh [--spec] DIR [FILE_FILTER]}"
filter="${2:-%}"

eval "$(cargo +nightly llvm-cov show-env --sh --branch 2>/dev/null)"
[ -x target/release/sash ] ||
    { echo "cov-batch: no target/release/sash — run cov-probe.sh --build" >&2; exit 1; }

rm -f target/*.profraw
export SASH_RT_LIB="$PWD/sash-rt/target/release/libsash_rt.a"

n_run=0
for f in "$corpus"/*; do
    [ -f "$f" ] || continue
    if [ -n "$spec_mode" ]; then
        (cd spec && PATH="$PWD/../target/release:$PATH" SASH_MODE=native \
            bash "$(basename "$f")") >/dev/null 2>&1 3>&1 || true
    else
        # A compile REJECTION reaches validate arms, so nonzero is expected.
        target/release/sash "$f" -o /tmp/cov-batch.bin >/dev/null 2>&1 || true
    fi
    n_run=$((n_run + 1))
done

n=$(ls target/*.profraw 2>/dev/null | wc -l)
[ "$n" -gt 0 ] || { echo "cov-batch: no profraw written — nothing ran." >&2; exit 1; }

lcov=$(mktemp); warm=$(mktemp)
cargo +nightly llvm-cov report --release --branch \
    --ignore-filename-regex "$IGNORE" --lcov --output-path "$lcov" 2>/dev/null

awk '/^SF:/ { sf = substr($0, 4); next }
     /^DA:/ { split(substr($0, 4), d, ",")
              if (d[2] + 0 > 0) print sf "|" d[1] }' "$lcov" |
    sed 's|^.*/\(sash/src/\)|\1|' | sort -u > "$warm"

echo "── corpus: $n_run programs, $n profraw(s) ──"
# Piped as a script, not passed as an argument: `.import` and friends are
# shell dot-commands and are only recognised on their own input line.
sqlite3 coverage.db <<SQL
CREATE TEMP TABLE w(file TEXT, line INT);
.mode list
.separator |
.import '$warm' w
ATTACH '.indexnudge-rust.db' AS ix;
SELECT cl.file||':'||cl.line||'  '||
       COALESCE((SELECT s.name FROM ix.symbols s
                 WHERE s.file=cl.file AND cl.line BETWEEN s.start_line AND s.end_line
                   AND s.kind IN ('function','method') LIMIT 1), '?')||
       '  '||substr(trim((SELECT content FROM ix.lines l
                          WHERE l.file=cl.file AND l.line_num=cl.line)), 1, 70)
FROM cov_lines cl JOIN w ON w.file=cl.file AND w.line=cl.line
WHERE cl.count=0 AND cl.file LIKE '$filter'
ORDER BY cl.file, cl.line;
SQL
rm -f "$lcov" "$warm"
