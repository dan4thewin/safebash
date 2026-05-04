#!/bin/bash
# spec/29.1.sh — sash-convert SHALL convert declare -i to int
set -eu
. common.sh
setup 29.1
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
declare -i x=5
echo $x'
[[ $out == *'int x=5'* ]]; chk 29.1a
[[ $out != *'declare'* ]]; chk 29.1b

tally
