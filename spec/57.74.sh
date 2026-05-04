#!/bin/bash
# spec/57.74.sh — compiler SHALL NOT have --emit-source, --annotate, or --no-lint flags
set -eu
. common.sh
setup 57.74
plan=3
echo "1..$plan" >&3

set +e
# Each removed flag should cause an error or be unrecognized
echo 'echo hello' > input.sash

sash --emit-source input.sash > /dev/null 2>&1
(( $? != 0 )); chk 57.74a

sash --annotate input.sash > /dev/null 2>&1
(( $? != 0 )); chk 57.74b

sash --nolint input.sash > /dev/null 2>&1
(( $? != 0 )); chk 57.74c

set -e
tally
