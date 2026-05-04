#!/bin/bash
# spec/29.8.sh — sash-convert SHALL convert let expr to (( expr ))
set -eu
. common.sh
setup 29.8
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
let "x=3+4"'
[[ $out == *'(( x=3+4 ))'* ]]; chk 29.8a
[[ $out != *'let'* ]]; chk 29.8b

tally
