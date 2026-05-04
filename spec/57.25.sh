#!/bin/bash
# spec/57.25.sh — sash-convert SHALL emit # MANUAL: for ${var@X} parameter transformations
set -eu
. common.sh
setup 57.25
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
x=hello
echo ${x@U}'
[[ $out == *'# MANUAL:'*'parameter transformation'* ]]; chk 57.25a

tally
