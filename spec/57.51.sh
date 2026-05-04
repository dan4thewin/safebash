#!/bin/bash
# spec/57.51.sh — sash-convert SHALL emit # BLESS: hint for unknown external commands
set -eu
. common.sh
setup 57.51
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
result=$(some_unknown_command arg1)'
[[ $out == *'# BLESS:'*'some_unknown_command'* ]]; chk 57.51a

tally
