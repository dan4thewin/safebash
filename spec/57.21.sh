#!/bin/bash
# spec/57.21.sh — sash-convert SHALL emit # MANUAL: annotation for source and dot command (.)
set -eu
. common.sh
setup 57.21
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
source helper.sh'
[[ $out == *'# MANUAL:'*'source'* ]]; chk 57.21a

run_convert '#!/bin/bash
. helper.sh'
[[ $out == *'# MANUAL:'* ]]; chk 57.21b

tally
