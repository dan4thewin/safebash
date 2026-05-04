#!/bin/bash
# spec/57.65.sh — sash-convert | sash -o binary SHALL produce a working binary
set -eu
. common.sh
setup 57.65
plan=2
echo "1..$plan" >&3

cat > input.sh << 'SH'
#!/bin/bash
declare -i x=5
echo $x
SH

sash-convert input.sh | sash -o binary /dev/stdin
rc=$?
(( rc == 0 )); chk 57.65a

out=$(./binary)
[[ $out == "5" ]]; chk 57.65b

tally
