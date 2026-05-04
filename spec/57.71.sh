#!/bin/bash
# spec/57.71.sh — non-sash declaration keyword SHALL be a compile error
set -eu
. common.sh
setup 57.71
plan=4
echo "1..$plan" >&3

set +e
run_sash 'declare -i x=5'
(( rc != 0 )); chk 57.71a

run_sash 'typeset x=5'
(( rc != 0 )); chk 57.71b

run_sash 'readonly x=5'
(( rc != 0 )); chk 57.71c

run_sash 'export FOO=bar'
(( rc != 0 )); chk 57.71d

set -e
tally
