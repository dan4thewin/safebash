#!/bin/bash
# spec/46.3.sh — sash-convert SHALL convert top-level positionals $1..$N to $(argv N)
set -eu
. common.sh
setup 46.3
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
echo $1'
[[ $out == *'$(argv 1)'* ]]; chk 46.3a

# $@ at top level
run_convert '#!/bin/bash
echo $@'
[[ $out == *'$(argv'* ]] || [[ $out == *'MANUAL'* ]]; chk 46.3b

tally
