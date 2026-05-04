#!/bin/bash
# spec/29.3.sh — sash-convert SHALL convert declare -A to map
set -eu
. common.sh
setup 29.3
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
declare -A m=([a]=1)'
[[ $out == *'map m=([a]=1)'* ]]; chk 29.3a
[[ $out != *'declare'* ]]; chk 29.3b

tally
