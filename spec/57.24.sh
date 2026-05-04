#!/bin/bash
# spec/57.24.sh — sash-convert SHALL emit # MANUAL: for <(...) and >(...) process substitution
set -eu
. common.sh
setup 57.24
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
diff <(echo a) <(echo b)'
[[ $out == *'# MANUAL:'*'process substitution'* ]]; chk 57.24a

tally
