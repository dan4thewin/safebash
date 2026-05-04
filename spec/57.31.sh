#!/bin/bash
# spec/57.31.sh — sash-convert SHALL emit # MANUAL: annotation for alias
set -eu
. common.sh
setup 57.31
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
alias ll="ls -la"'
[[ $out == *'# MANUAL:'*'alias'* ]]; chk 57.31a

tally
