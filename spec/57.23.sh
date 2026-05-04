#!/bin/bash
# spec/57.23.sh — sash-convert SHALL emit # MANUAL: for declare -n / local -n / typeset -n
set -eu
. common.sh
setup 57.23
plan=3
echo "1..$plan" >&3

run_convert '#!/bin/bash
declare -n ref=x'
[[ $out == *'# MANUAL:'*'nameref'* ]]; chk 57.23a

run_convert '#!/bin/bash
f() { local -n ref=$1; }'
[[ $out == *'# MANUAL:'* ]]; chk 57.23b

run_convert '#!/bin/bash
typeset -n ref=x'
[[ $out == *'# MANUAL:'* ]]; chk 57.23c

tally
