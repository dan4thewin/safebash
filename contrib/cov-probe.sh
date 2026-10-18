#!/bin/bash
# contrib/cov-probe.sh — did THIS snippet flip THAT branch?
#
# `mk cov-branch` answers "where are we" in about forty minutes: it rebuilds
# instrumented, runs the whole spec sweep, and reports every file. That is the
# wrong instrument for the inner loop of closing branches one at a time, where
# the question is much narrower — I wrote a candidate spec, does it reach the
# arm I aimed it at?
#
# This answers that in seconds. It keeps the instrumented binary in place,
# runs only the snippets you hand it, and prints the per-branch counts for one
# file:line. A branch that is still 0 after your snippet means the snippet does
# not reach it — find a different spelling BEFORE paying for a full sweep.
#
# The trap this avoids: a cold arm is not evidence of dead code until you have
# found the spelling that reaches it, and the first spelling you try is often
# not it. `if (( x++ ))` reports the Assign arm, not PostIncDec; the inc/dec
# arms needed `while (( x++ < 3 ))`. Guessing wrong costs a 40-minute cycle
# without this script, and about four seconds with it.
#
# Usage:
#   contrib/cov-probe.sh --build                 # once: instrument target/release
#   contrib/cov-probe.sh FILE:LINE SNIPPET...    # run snippets, report that line
#
# Reports the BRANCH records at that line, or — when the line is a branch
# BODY rather than a condition, which is what the cold-LINE queue ranks — its
# line count instead.
#   contrib/cov-probe.sh --spec FILE SNIPPET...  # ...or name a whole spec file
#
# Example:
#   contrib/cov-probe.sh --build
#   contrib/cov-probe.sh sash/src/validate.rs:1985 \
#       'int x=0; while (( x++ < 3 )); do echo $x; done'
#
# Remember to `mk cov-reset` when done — target/release stays instrumented,
# and an instrumented sash is slow and litters default_*.profraw.
set -eu
cd "$(dirname "$0")/.."

. contrib/cov-common.sh

build_instrumented() {
    eval "$(cargo +nightly llvm-cov show-env --sh --branch 2>/dev/null)"
    rm -rf target/release
    cargo +nightly build --release >/dev/null 2>&1
    # The C helpers live in target/release too and just went with it.
    cc -O2 -o target/release/sash-regex sash-rt/csrc/sash-regex.c \
        sash-rt/csrc/regcomp.c sash-rt/csrc/regexec.c sash-rt/csrc/regaux.c \
        -I sash-rt/csrc
    cc -O2 -o target/release/sash-getopt sash-getopt/sash-getopt.c
    rm -f target/*.profraw
    # Same canary as contrib/coverage.sh: an uninstrumented binary reports a
    # clean-looking zero for every branch, which reads exactly like "your
    # snippet did not reach it".
    target/release/sash --help >/dev/null 2>&1 || true
    if [ "$(ls target/*.profraw 2>/dev/null | wc -l)" -eq 0 ]; then
        echo "cov-probe: target/release/sash is NOT instrumented — aborting." >&2
        exit 1
    fi
    echo "cov-probe: instrumented build ready. Run 'mk cov-reset' when done."
}

if [ "${1:-}" = "--build" ]; then
    build_instrumented
    exit 0
fi

[ $# -ge 2 ] || { sed -n '25,31p' "$0" >&2; exit 1; }

eval "$(cargo +nightly llvm-cov show-env --sh --branch 2>/dev/null)"
if [ ! -x target/release/sash ]; then
    echo "cov-probe: no target/release/sash — run '$0 --build' first." >&2
    exit 1
fi

spec_mode=""
if [ "$1" = "--spec" ]; then spec_mode=1; shift; fi
target="$1"; shift

rm -f target/*.profraw
export SASH_RT_LIB="$PWD/sash-rt/target/release/libsash_rt.a"

for snip in "$@"; do
    if [ -n "$spec_mode" ]; then
        (cd spec && PATH="$PWD/../target/release:$PATH" SASH_MODE=native \
            bash "$snip") >/dev/null 2>&1 3>&1 || true
    else
        # A compile REJECTION is a perfectly good way to reach a branch, so a
        # nonzero exit here is not an error — only a missing profraw is.
        target/release/sash -c "$snip" -o /tmp/cov-probe.bin >/dev/null 2>&1 || true
    fi
done

n=$(ls target/*.profraw 2>/dev/null | wc -l)
[ "$n" -gt 0 ] || { echo "cov-probe: no profraw written — nothing ran." >&2; exit 1; }

out=$(mktemp)
cargo +nightly llvm-cov report --release --branch \
    "${COV_REPORT_SCOPE[@]}" --lcov --output-path "$out" 2>/dev/null
cov_check_scope "$out"

file="${target%:*}"; line="${target##*:}"
echo "── $file:$line  (from $n profraw(s)) ──"
awk -v f="$file" -v l="$line" '
    /^SF:/      { sf = substr($0, 4); next }
    /^BRDA:/    {
        if (sf !~ f "$") next
        split(substr($0, 6), a, ",")
        if (a[1] != l) next
        hit = (a[4] == "-") ? 0 : a[4]
        printf "  block %s branch %s  count %s%s\n", a[2], a[3], hit,
               (hit == 0 ? "   <- STILL COLD" : "")
        seen = 1
    }
    # Line fallback. A branch BODY (the arm of a match, the `then` of an `if`)
    # carries no BRDA of its own, and the coverage queue is ranked by cold
    # LINES, so aiming at one and being told "no branch records" reads exactly
    # like "not instrumented". The DA record answers it directly.
    /^DA:/      {
        if (sf !~ f "$") next
        split(substr($0, 4), d, ",")
        if (d[1] != l) next
        da = 1; dacount = d[2]
    }
    END {
        if (seen) exit
        if (da) printf "  line count %s%s\n", dacount,
                       (dacount == 0 ? "   <- STILL COLD" : "")
        else print "  (no branch or line records at that line — wrong line, or not instrumented)"
    }
' "$out"
rm -f "$out" /tmp/cov-probe.bin
