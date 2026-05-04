#!/bin/bash
# spec/57.32.sh — sash-convert SHALL emit # MANUAL: for unconvertible ( cmd ) subshell
set -eu
. common.sh
setup 57.32
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
(echo hello; echo world)'
[[ $out == *'# MANUAL:'*'subshell'* ]]; chk 57.32a

tally
