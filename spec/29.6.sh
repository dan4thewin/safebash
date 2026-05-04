#!/bin/bash
# spec/29.6.sh — sash-convert SHALL convert [ ] test to [[ ]]
set -eu
. common.sh
setup 29.6
plan=3
echo "1..$plan" >&3

run_convert '#!/bin/bash
if [ "$x" -gt 0 ]; then echo yes; fi'
[[ $out == *'[[ "$x" -gt 0 ]]'* ]]; chk 29.6a
[[ $out != *'[ "'* ]]; chk 29.6b

# Already [[ ]] should be unchanged
run_convert '#!/bin/bash
if [[ "$x" -gt 0 ]]; then echo yes; fi'
[[ $out == *'[[ "$x" -gt 0 ]]'* ]]; chk 29.6c

tally
