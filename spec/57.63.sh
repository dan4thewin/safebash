#!/bin/bash
# spec/57.63.sh — sash SHALL accept sash-native source only
set -eu
. common.sh
setup 57.63
plan=2
echo "1..$plan" >&3

# Sash-native input compiles
run_sash 'int x=5
echo $x'
(( rc == 0 )); chk 57.63a

set +e
# Bash-specific input rejected (declare is not sash-native)
run_sash 'declare -i x=5
echo $x'
(( rc != 0 )); chk 57.63b
set -e

tally
