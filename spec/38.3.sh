#!/bin/bash
# spec/38.3.sh — sash-convert SHALL convert subshells with cd to save/restore cwd
set -eu
. common.sh
setup 38.3
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
(cd /tmp; ls)'
# Should convert or mark as MANUAL
[[ $out != *'(cd /tmp'* ]] || [[ $out == *'MANUAL'* ]]; chk 38.3a

tally
