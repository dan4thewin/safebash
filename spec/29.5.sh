#!/bin/bash
# spec/29.5.sh — sash-convert SHALL convert backtick command substitution to $()
set -eu
. common.sh
setup 29.5
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
x=`date`'
[[ $out == *'$(date)'* ]]; chk 29.5a
[[ $out != *'`'* ]]; chk 29.5b

tally
