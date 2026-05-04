#!/bin/bash
# spec/40.1.sh — sash-convert SHALL run shellcheck internally and consume its JSON output
# Verified indirectly: SC2086 findings appear as CHECK hints
set -eu
. common.sh
setup 40.1
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
x=hello
echo $x'
[[ $out == *'CHECK'*'SC2086'* ]]; chk 40.1a

tally
