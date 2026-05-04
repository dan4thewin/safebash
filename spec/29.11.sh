#!/bin/bash
# spec/29.11.sh — sash-convert --annotate SHALL add "# was: ..." comments
set -eu
. common.sh
setup 29.11
plan=3
echo "1..$plan" >&3

run_convert_annotate '#!/bin/bash
declare -i x=5'
[[ $out == *'# was: declare -i x=5'* ]]; chk 29.11a

run_convert_annotate '#!/bin/bash
readonly msg=hello'
[[ $out == *'# was: readonly msg=hello'* ]]; chk 29.11b

# Native sash syntax should NOT get annotation
run_convert_annotate '#!/bin/bash
int x=5'
[[ $out != *'# was:'* ]]; chk 29.11c

tally
