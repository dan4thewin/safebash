#!/bin/bash
# spec/29.2.sh — sash-convert SHALL convert declare -a to array
set -eu
. common.sh
setup 29.2
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
declare -a arr=(1 2 3)'
[[ $out == *'array arr=(1 2 3)'* ]]; chk 29.2a
[[ $out != *'declare'* ]]; chk 29.2b

tally
