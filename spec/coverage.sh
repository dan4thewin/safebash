#!/bin/bash
# spec/coverage.sh — tests for reachable compiler paths that no feature-
# oriented spec happens to exercise, found via LLVM coverage (journal 188+).
# Only genuinely-new (0 prior run_sash) reachable rejections go here; shadowed/
# unreachable code is deleted instead of tested (DG 2026-08-26).
set -eu
. common.sh
setup coverage
plan=10
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
# cov.4 a static (compile-constant) invalid regex in `=~` → SB5023
run_sash '[[ abc =~ ( ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5023* ]]; chk cov.4
# cov.5 a numeric test op with a non-numeric operand → SB5019
run_sash '[[ 5 -lt abc ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5019* ]]; chk cov.5
# cov.6 a boolean-context predicate that does not return on all paths → SB4037
run_sash 'p() { if true; then return 0; fi }; if p; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk cov.6
# cov.7 `read -f` (field separator) with a single target var → SB5016
run_sash 'read -f x < /etc/hostname'
[[ $rc != 0 && -z $out && $err == *SB5016* ]]; chk cov.7
# cov.8 a `${x@OP}` transform is rejected → SB4015
run_sash 'x=hi; echo ${x@Q}'
[[ $rc != 0 && -z $out && $err == *SB4015* ]]; chk cov.8
# cov.9 the removed `case` construct → SB4029
run_sash 'case $x in a) echo m;; esac'
[[ $rc != 0 && -z $out && $err == *SB4029* ]]; chk cov.9
# cov.10 a tainted value placed into an array via subscript assign → SB5021
run_sash 'array a=(); a[0]=$(getenv HOME)'
[[ $rc != 0 && -z $out && $err == *SB5021* ]]; chk cov.10

set -e
tally
