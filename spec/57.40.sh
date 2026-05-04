#!/bin/bash
# spec/57.40.sh — sash-convert SHALL emit # CHECK: hint for unquoted $var (SC2086)
set -eu
. common.sh
setup 57.40
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
x=hello
echo $x'
[[ $out == *'# CHECK:'*'word splitting'* ]]; chk 57.40a

tally
