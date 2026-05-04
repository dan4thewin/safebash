#!/bin/bash
# spec/57.29.sh — sash-convert SHALL emit # MANUAL: annotation for coproc
set -eu
. common.sh
setup 57.29
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
coproc cat'
[[ $out == *'# MANUAL:'*'coproc'* ]]; chk 57.29a

tally
