#!/bin/bash
# spec/29.7.sh — sash-convert SHALL convert ${var-x} to ${var:-x}
set -eu
. common.sh
setup 29.7
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
echo ${HOME-default}'
[[ $out == *'${HOME:-default}'* ]]; chk 29.7a

# Already ${var:-x} should be unchanged
run_convert '#!/bin/bash
echo ${HOME:-default}'
[[ $out == *'${HOME:-default}'* ]]; chk 29.7b

tally
