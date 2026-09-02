#!/bin/bash
# contrib/test-getopt.sh — sash-getopt (vendored util-linux getopt) smoke test.
#
# sash-getopt canonicalizes options like getopt(1) but emits NUL-separated,
# unquoted fields (no trailing newline) — so a NUL-delimited read splits them
# unambiguously with no shell-quoting to parse. Run from the repo root with
# target/release on PATH (see `mk sash-getopt-test`).
#
# Comparison renders each NUL as `|` so a space-bearing field stays visible.
set -u

GETOPT=${GETOPT:-target/release/sash-getopt}
fail=0
n=0

# check <name> <expected> -- <getopt args...>   (NUL rendered as `|`)
check() {
    local name=$1 want=$2; shift 2
    [[ $1 == -- ]] && shift
    (( n++ ))
    local got
    got=$("$GETOPT" "$@" | tr '\0' '|')
    if [[ "$got" == "$want" ]]; then
        echo "ok $n - $name"
    else
        echo "not ok $n - $name"
        echo "    want: [$want]"
        echo "    got:  [$got]"
        fail=1
    fi
}

# Fields are NUL-separated (shown `|`), unquoted, no trailing newline.
check "short opts + arg"                '-a|-b|val|--|arg|'     -- -o ab: -- -a -b val arg
# A space-bearing argument stays ONE field (no quoting, split only on NUL).
check "space-bearing arg is one field"  '-a|--|has space|'      -- -o a -- -a 'has space'
check "long option"                     '--alpha|--|'           -- -o '' -l alpha -- --alpha
check "short opt value"                 '-b|v|--|'              -- -o b: -- -b v
# Trailing non-option args after --.
check "positional args preserved"       '--|one|two three|'     -- -o '' -- one 'two three'

if (( fail )); then
    echo "FAILED sash-getopt smoke test"
    exit 1
fi
echo "PASSED ($n sash-getopt checks)"
