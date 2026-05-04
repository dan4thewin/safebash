#!/bin/bash
# spec/45.1.sh — sash-convert output and .sash_source ELF section SHALL use same format
set -eu
. common.sh
setup 45.1
plan=1
echo "1..$plan" >&3

cat > input.sh << 'SH'
#!/bin/bash
declare -i x=5
echo $x
SH

sash-convert input.sh -o converted.sash
sash converted.sash -o binary
sash --extract binary > extracted.sash

diff -q converted.sash extracted.sash; chk 45.1a

tally
