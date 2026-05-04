#!/bin/bash
# spec/57.33.sh — sash-convert SHALL pass through unconvertible lines unchanged
set -eu
. common.sh
setup 57.33
plan=2
echo "1..$plan" >&3

run_convert '#!/bin/bash
eval "echo hello"'
# The eval line itself must appear in the output unchanged
[[ $out == *'eval "echo hello"'* ]]; chk 57.33a
# And the MANUAL annotation must also be present
[[ $out == *'# MANUAL:'* ]]; chk 57.33b

tally
