#!/bin/bash
# spec/57.42.sh — sash-convert SHALL NOT alter unquoted variables — only annotate them
set -eu
. common.sh
setup 57.42
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
x=hello
echo $x'
# The unquoted $x must remain unquoted in the output
[[ $out == *'echo $x'* ]]; chk 57.42a

tally
