#!/bin/bash
# spec/57.20.sh — sash-convert SHALL emit # MANUAL: annotation for eval
set -eu
. common.sh
setup 57.20
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
eval "echo hello"'
[[ $out == *'# MANUAL:'*'eval'* ]]; chk 57.20a

tally
