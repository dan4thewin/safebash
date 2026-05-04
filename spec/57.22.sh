#!/bin/bash
# spec/57.22.sh — sash-convert SHALL emit # MANUAL: annotation for ${!var} indirect expansion
set -eu
. common.sh
setup 57.22
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
x=HOME
echo ${!x}'
[[ $out == *'# MANUAL:'*'indirect'* ]]; chk 57.22a

tally
