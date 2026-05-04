#!/bin/bash
# spec/57.28.sh — sash-convert SHALL emit # MANUAL: annotation for select
set -eu
. common.sh
setup 57.28
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
select opt in a b c; do echo $opt; break; done'
[[ $out == *'# MANUAL:'*'select'* ]]; chk 57.28a

tally
