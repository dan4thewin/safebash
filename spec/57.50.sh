#!/bin/bash
# spec/57.50.sh — sash-convert SHALL emit bless lines for known externals
set -eu
. common.sh
setup 57.50
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
today=$(date +%Y-%m-%d)'
[[ $out == *'bless today'* ]]; chk 57.50a

run_convert '#!/bin/bash
host=$(hostname)'
[[ $out == *'bless host'* ]]; chk 57.50b

tally
