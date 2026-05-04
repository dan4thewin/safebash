#!/bin/bash
# spec/40.2.sh — sash-convert SHALL NOT require shellcheck-clean input as a precondition
set -eu
. common.sh
setup 40.2
plan=2
echo "1..$plan" >&3

# Input with shellcheck warnings should still convert successfully
run_convert '#!/bin/bash
x=hello
echo $x'
(( rc == 0 )); chk 40.2a
[[ $out == *'echo $x'* ]]; chk 40.2b

tally
