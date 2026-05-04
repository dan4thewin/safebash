#!/bin/bash
# spec/57.26.sh — sash-convert SHALL emit # MANUAL: for $(cmd) inside heredoc body
set -eu
. common.sh
setup 57.26
plan=1
echo "1..$plan" >&3

run_convert '#!/bin/bash
cat <<EOF
today is $(date)
EOF'
[[ $out == *'# MANUAL:'*'heredoc'* ]]; chk 57.26a

tally
