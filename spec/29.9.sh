#!/bin/bash
# spec/29.9.sh — sash-convert SHALL convert typeset to bare assignment
set -eu
. common.sh
setup 29.9
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
typeset y=10'
[[ $out == *'y=10'* ]]; chk 29.9a
[[ $out != *'typeset'* ]]; chk 29.9b

tally
