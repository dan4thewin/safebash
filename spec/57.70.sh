#!/bin/bash
# spec/57.70.sh — non-sash node type SHALL be a compile error
set -eu
. common.sh
setup 57.70
plan=2
echo "1..$plan" >&3

set +e
# select is not a sash-native node type
run_sash 'select x in a b c; do echo $x; break; done'
(( rc != 0 )); chk 57.70a

# coproc is not a sash-native node type
run_sash 'coproc cat'
(( rc != 0 )); chk 57.70b

set -e
tally
