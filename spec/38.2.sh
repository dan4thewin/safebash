#!/bin/bash
# spec/38.2.sh — sash-convert SHALL convert subshells with assignments to synthetic function
set -eu
. common.sh
setup 38.2
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
(x=5; echo $x)'
# Should contain a function or local declaration, not raw subshell
[[ $out != *'(x=5'* ]] || [[ $out == *'MANUAL'* ]]; chk 38.2a

tally
