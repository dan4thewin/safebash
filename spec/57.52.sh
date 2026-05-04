#!/bin/bash
# spec/57.52.sh — sash-convert SHALL emit # BLESS: hint for variables populated by read
set -eu
. common.sh
setup 57.52
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
read -r line'
[[ $out == *'# BLESS:'*'line'*'read'* ]]; chk 57.52a

tally
