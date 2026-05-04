#!/bin/bash
# spec/57.27.sh — sash-convert SHALL emit # MANUAL: annotation for export
set -eu
. common.sh
setup 57.27
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
export PATH="/usr/bin"'
[[ $out == *'# MANUAL:'*'export'* ]]; chk 57.27a

tally
