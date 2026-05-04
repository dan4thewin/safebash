#!/bin/bash
# spec/29.10.sh — sash-convert output SHALL be valid sash-native source
# Test: convert a bash script, then compile the output with sash
set -eu
. common.sh
setup 29.10
plan=2
echo "1..$plan" >&3

cat > input.sh << 'SH'
#!/bin/bash
declare -i x=5
echo $x
SH

sash-convert input.sh -o converted.sash
rc=$?
(( rc == 0 )); chk 29.10a

sash converted.sash -o binary
rc=$?
(( rc == 0 )); chk 29.10b

tally
