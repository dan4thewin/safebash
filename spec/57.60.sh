#!/bin/bash
# spec/57.60.sh — sash-convert SHALL accept input from a file argument or stdin
set -eu
. common.sh
setup 57.60
plan=2
echo "1..$plan" >&3

# File argument
cat > input.sh << 'SH'
#!/bin/bash
declare -i x=5
SH
out=$(sash-convert input.sh)
[[ $out == *'int x=5'* ]]; chk 57.60a

# Stdin
out=$(echo '#!/bin/bash
declare -i x=5' | sash-convert)
[[ $out == *'int x=5'* ]]; chk 57.60b

tally
