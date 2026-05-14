#!/bin/bash
# Cyclomatic complexity estimator for Rust source files.
# Counts branch points per function: if, else if, while, for, loop,
# match arms (=>), &&, ||, ?  (each adds 1 to base complexity of 1).
#
# Usage: bash contrib/cyclo.sh [dir] [--top N] [--over N]
#
# Output: complexity function file:line
# Sorted by complexity descending.

set -eu

dir="${1:-sash/src}"
top=20
threshold=0

shift || true
while [[ $# -gt 0 ]]; do
    case "$1" in
        --top) top="$2"; shift 2 ;;
        --over) threshold="$2"; shift 2 ;;
        *) shift ;;
    esac
done

awk '
# Track brace depth to know when functions end
/^[[:space:]]*(pub(\(crate\))? )?(unsafe )?(extern "C" )?fn [a-zA-Z_]/ {
    if (in_fn && depth == 0) {
        # emit previous function
        if (cc > 0) print cc, fn_name, fn_file ":" fn_line
    }
    fn_name = $0
    # Extract function name
    gsub(/.*fn /, "", fn_name)
    gsub(/[<(].*/, "", fn_name)
    gsub(/[[:space:]].*/, "", fn_name)
    fn_file = FILENAME
    fn_line = FNR
    cc = 1  # base complexity
    depth = 0
    in_fn = 1
}

in_fn {
    # Count open/close braces to track nesting
    s = $0
    gsub(/[^{]/, "", s)
    depth += length(s)
    s = $0
    gsub(/[^}]/, "", s)
    depth -= length(s)

    # Skip comments and strings (rough)
    line = $0
    gsub(/\/\/.*/, "", line)
    gsub(/"[^"]*"/, "", line)

    # Count branch points
    # if/else if (not "else {")
    if (line ~ /[[:space:]]if[[:space:](]/) cc++
    if (line ~ /else if[[:space:](]/) cc++  # already counted by if above, subtract
    # while/for/loop
    if (line ~ /[[:space:]]while[[:space:](]/) cc++
    if (line ~ /[[:space:]]for[[:space:]]/) cc++
    if (line ~ /[[:space:]]loop[[:space:]]*\{/) cc++
    # match arms
    n = gsub(/=>/, "=>", line)
    if (n > 0) cc += n
    # logical operators
    n = gsub(/&&/, "&&", line)
    if (n > 0) cc += n
    n = gsub(/\|\|/, "||", line)
    if (n > 0) cc += n
    # ? operator
    n = gsub(/\?[^?]/, "?", line)
    # too noisy — skip ? for now

    if (depth <= 0 && FNR > fn_line) {
        print cc, fn_name, fn_file ":" fn_line
        in_fn = 0
    }
}

END {
    if (in_fn) print cc, fn_name, fn_file ":" fn_line
}
' $(find "$dir" -name '*.rs' -not -path '*/tests/*') \
| sort -rn \
| awk -v top="$top" -v threshold="$threshold" '
    { sum += $1; count++; if ($1 > max) max = $1 }
    $1 > threshold && NR <= top { print }
    END {
        avg = (count > 0) ? sum/count : 0
        printf "\n--- %d functions, avg %.1f, max %d ---\n", count, avg, max > "/dev/stderr"
    }
'
