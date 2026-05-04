#!/bin/bash
# spec/29.4.sh — sash-convert SHALL convert declare -r and readonly to const
set -eu
. common.sh
setup 29.4
plan=4
echo "1..$plan" >&3

run_convert '#!/bin/bash
declare -r x=5'
[[ $out == *'const x=5'* ]]; chk 29.4a
[[ $out != *'declare'* ]]; chk 29.4b

run_convert '#!/bin/bash
readonly msg=hello'
[[ $out == *'const msg=hello'* ]]; chk 29.4c
[[ $out != *'readonly'* ]]; chk 29.4d

tally
