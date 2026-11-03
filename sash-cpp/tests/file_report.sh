#!/usr/bin/env bash
set -euo pipefail

binary=$1
expected=$2
actual=$3
directory=$(mktemp -d "${TMPDIR:-/tmp}/sash-cpp-file-report.XXXXXX")
cleanup() {
    rm -f "$directory/records" "$directory/nested" "$directory/merged" \
        "$directory/unmerged" "$directory/descriptor" "$directory/failure"
    rmdir "$directory"
}
trap cleanup EXIT

timeout 10 "$binary" "$directory" > "$actual" 2> "$actual.err"
cmp "$expected" "$actual"
test ! -s "$actual.err"
test ! -s "$directory/failure"
