#!/bin/bash
# spec/57.30.sh — sash-convert SHALL emit # MANUAL: for set -e, set -u, shopt
set -eu
. common.sh
setup 57.30
plan=3
echo "1..$plan" >&3

run_convert '#!/bin/bash
set -e'
[[ $out == *'# MANUAL:'*'set'* ]]; chk 57.30a

run_convert '#!/bin/bash
set -u'
[[ $out == *'# MANUAL:'*'set'* ]]; chk 57.30b

run_convert '#!/bin/bash
shopt -s nullglob'
[[ $out == *'# MANUAL:'*'shopt'* ]]; chk 57.30c

tally
