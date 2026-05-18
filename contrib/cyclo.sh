#!/bin/bash
# Cyclomatic complexity for Rust source files.
# Uses rust-code-analysis-cli (Mozilla) if available, falls back to awk estimator.
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

if command -v rust-code-analysis-cli >/dev/null 2>&1; then
    rust-code-analysis-cli -m -p "$dir" -O json 2>/dev/null | python3 -c "
import json, sys

results = []
for line in sys.stdin:
    data = json.loads(line)
    fname = data.get('name','')
    def walk(node):
        if node.get('kind') == 'function':
            name = node.get('name','?')
            cc = int(node.get('metrics',{}).get('cyclomatic',{}).get('sum',0))
            line = int(node.get('start_line',0))
            results.append((cc, name, fname, line))
        for space in node.get('spaces', []):
            walk(space)
    walk(data)

results.sort(reverse=True)
total = sum(r[0] for r in results)
count = len(results)
avg = total / count if count else 0
top_n = int(sys.argv[1])
thresh = int(sys.argv[2])

for cc, name, f, line in results[:top_n]:
    if cc <= thresh:
        break
    print(f'{cc:4d}  {name:40s} {f}:{line}')

print(f'\n--- {count} functions, avg {avg:.1f}, max {results[0][0] if results else 0} ---', file=sys.stderr)
" "$top" "$threshold"
else
    echo "rust-code-analysis-cli not found, using awk estimator" >&2
    # Fallback: awk-based estimator (less accurate)
    awk '
    /^[[:space:]]*(pub(\(crate\))? )?(unsafe )?(extern "C" )?fn [a-zA-Z_]/ {
        if (in_fn && depth == 0) {
            if (cc > 0) print cc, fn_name, fn_file ":" fn_line
        }
        fn_name = $0
        gsub(/.*fn /, "", fn_name)
        gsub(/[<(].*/, "", fn_name)
        gsub(/[[:space:]].*/, "", fn_name)
        fn_file = FILENAME
        fn_line = FNR
        cc = 1
        depth = 0
        in_fn = 1
    }
    in_fn {
        s = $0; gsub(/[^{]/, "", s); depth += length(s)
        s = $0; gsub(/[^}]/, "", s); depth -= length(s)
        line = $0; gsub(/\/\/.*/, "", line); gsub(/"[^"]*"/, "", line)
        if (line ~ /[[:space:]]if[[:space:](]/) cc++
        if (line ~ /else if[[:space:](]/) cc++
        if (line ~ /[[:space:]]while[[:space:](]/) cc++
        if (line ~ /[[:space:]]for[[:space:]]/) cc++
        if (line ~ /[[:space:]]loop[[:space:]]*\{/) cc++
        n = gsub(/=>/, "=>", line); if (n > 0) cc += n
        n = gsub(/&&/, "&&", line); if (n > 0) cc += n
        n = gsub(/\|\|/, "||", line); if (n > 0) cc += n
        if (depth <= 0 && FNR > fn_line) { print cc, fn_name, fn_file ":" fn_line; in_fn = 0 }
    }
    END { if (in_fn) print cc, fn_name, fn_file ":" fn_line }
    ' $(find "$dir" -name '*.rs' -not -path '*/tests/*') \
    | sort -rn \
    | awk -v top="$top" -v threshold="$threshold" '
        { sum += $1; count++; if ($1 > max) max = $1 }
        $1 > threshold && NR <= top { print }
        END { printf "\n--- %d functions, avg %.1f, max %d ---\n", count, avg=sum/count, max > "/dev/stderr" }
    '
fi
