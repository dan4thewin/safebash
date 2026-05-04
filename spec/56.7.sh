#!/bin/bash
# spec/56.7.sh — sash-convert SHALL convert trap "" SIG to trap : SIG
set -eu
. common.sh
setup 56.7
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
trap "" SIGINT'
[[ $out == *'trap : SIGINT'* ]]; chk 56.7a
[[ $out != *'trap ""'* ]]; chk 56.7b

tally
