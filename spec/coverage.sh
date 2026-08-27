#!/bin/bash
# spec/coverage.sh — tests for reachable compiler paths that no feature-
# oriented spec happens to exercise, found via LLVM coverage (journal 188+).
# Only genuinely-new (0 prior run_sash) reachable rejections go here; shadowed/
# unreachable code is deleted instead of tested (DG 2026-08-26).
set -eu
. common.sh
setup coverage
plan=24
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
# cov.11 `source` / `.` is rejected (fixed-at-compile-time) → SB4002
run_sash 'source f'
[[ $rc != 0 && -z $out && $err == *SB4002* ]]; chk cov.11
# cov.12 indirect expansion `${!x}` is rejected → SB4003
run_sash 'x=y; echo ${!x}'
[[ $rc != 0 && -z $out && $err == *SB4003* ]]; chk cov.12
# cov.13 a `{ }` group cannot be a condition → SB4027
run_sash 'if { echo a; }; then echo hi; fi'
[[ $rc != 0 && -z $out && $err == *SB4027* ]]; chk cov.13
# cov.14 heredocs are rejected (use a herestring) → SB5002
run_sash 'cat <<EOF'
[[ $rc != 0 && -z $out && $err == *SB5002* ]]; chk cov.14
# cov.15 `$(status)` with no preceding command → SB5012
run_sash 'echo $(status)'
[[ $rc != 0 && -z $out && $err == *SB5012* ]]; chk cov.15
# cov.16 an inline `*` glob in a command argument → SB5029
run_sash 'echo *'
[[ $rc != 0 && -z $out && $err == *SB5029* ]]; chk cov.16
# cov.17 a malformed `@{…}` splat disposition → SB5033
run_sash 'array a=(1); echo @{a:x}'
[[ $rc != 0 && -z $out && $err == *SB5033* ]]; chk cov.17
# cov.18 a `$var` in a for-loop source (no word-split by design) → SB5009
run_sash 'y=a; for x in $y; do echo $x; done'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk cov.18
# cov.19 the `**` globstar is always rejected (even quoted) → SB5006
run_sash 'echo "**"'
[[ $rc != 0 && -z $out && $err == *SB5006* ]]; chk cov.19
# cov.20 `read` with an unknown flag → SB5015
run_sash 'read -z x < /etc/hostname'
[[ $rc != 0 && -z $out && $err == *SB5015* ]]; chk cov.20
# cov.21 quoting an array splat is not allowed → SB5026
run_sash 'echo "$(argv)"'
[[ $rc != 0 && -z $out && $err == *SB5026* ]]; chk cov.21
# cov.22 `eval` (dynamic execution) is rejected → SB4001
run_sash 'eval "echo hi"'
[[ $rc != 0 && -z $out && $err == *SB4001* ]]; chk cov.22
# cov.23 a prefix `++`/`--` in `(( ))` is an arith syntax error → SB4020
run_sash 'int x=0; (( ++x ))'
[[ $rc != 0 && -z $out && $err == *SB4020* ]]; chk cov.23
# cov.24 `export` is rejected (parser) → SB5001
run_sash 'export A=1'
[[ $rc != 0 && -z $out && $err == *SB5001* ]]; chk cov.24

set -e
tally
