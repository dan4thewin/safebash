#!/bin/bash
# spec/coverage.sh — tests for reachable compiler paths that no feature-
# oriented spec happens to exercise, found via LLVM coverage (journal 188+).
# Only genuinely-new (0 prior run_sash) reachable rejections go here; shadowed/
# unreachable code is deleted instead of tested (DG 2026-08-26).
set -eu
. common.sh
setup coverage
plan=3
echo "1..$plan" >&3

set +e

# cov.1 a mutation in a `(( ))` CONDITION is rejected (validate SB5007) — no
# prior spec exercises a bare mutation in condition position
run_sash 'if (( x=1 )); then echo hi; fi'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.1
# cov.2 DD#31 comma-in-condition: a non-final part that isn't a mutation → SB5007
run_sash 'int a=0; int b=0; while (( a==1, b=2 )); do echo x; done'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.2
# cov.3 DD#31 comma-in-condition: a final part that IS a mutation → SB5007
run_sash 'int a=0; while (( a=1, a=2 )); do echo x; done'
[[ $rc != 0 && -z $out && $err == *SB5007* ]]; chk cov.3

set -e
tally
