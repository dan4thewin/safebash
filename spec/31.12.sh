#!/bin/bash
# spec/31.12.sh — sash-convert SHALL extract $((x=10)) to ((x=10)) statement form
set -eu
. common.sh
setup 31.12
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
y=$((x=10))'
[[ $out == *'(( x=10 ))'* ]]; chk 31.12a

tally
