#!/bin/bash
# spec/57.61.sh — sash-convert SHALL write output to -o file or stdout
set -eu
. common.sh
setup 57.61
plan=2
echo "1..$plan" >&3

# Stdout (default)
out=$(echo '#!/bin/bash
declare -i x=5' | sash-convert)
[[ $out == *'int x=5'* ]]; chk 57.61a

# -o file
echo '#!/bin/bash
declare -i x=5' | sash-convert -o output.sash
[[ $(<output.sash) == *'int x=5'* ]]; chk 57.61b

tally
