#!/bin/bash
# Cyclomatic complexity for Rust source files.
# Uses rust-code-analysis-cli (Mozilla) if available, falls back to awk estimator.
#
# Usage: bash contrib/cyclo.sh [dir] [--top N] [--over N] [--show-allowed] [--no-allow]
#
# Output: complexity function file:line
# Sorted by complexity descending.
#
# Whitelist: if `.cyclo-allow` exists at the repo root (or alongside this
# script), each `path::function_name` line is filtered out of the output.
# Lines starting with `#` and blank lines are ignored. Rationale may follow
# `#` on the same line. Use --show-allowed to see what was filtered out.

set -eu

dir="${1:-sash/src}"
top=20
threshold=0
show_allowed=0
use_allow=1

shift || true
while [[ $# -gt 0 ]]; do
    case "$1" in
        --top) top="$2"; shift 2 ;;
        --over) threshold="$2"; shift 2 ;;
        --show-allowed) show_allowed=1; shift ;;
        --no-allow) use_allow=0; shift ;;
        *) shift ;;
    esac
done

# Locate the allow file: prefer repo root, fall back to script dir.
allow_file=""
if [[ "$use_allow" -eq 1 ]]; then
    repo_root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel 2>/dev/null || true)"
    if [[ -n "$repo_root" && -f "$repo_root/.cyclo-allow" ]]; then
        allow_file="$repo_root/.cyclo-allow"
    elif [[ -f "$(dirname "$0")/.cyclo-allow" ]]; then
        allow_file="$(dirname "$0")/.cyclo-allow"
    fi
fi
export CYCLO_ALLOW_FILE="$allow_file"
export CYCLO_SHOW_ALLOWED="$show_allowed"

if command -v rust-code-analysis-cli >/dev/null 2>&1; then
    rust-code-analysis-cli -m -p "$dir" -O json 2>/dev/null | python3 -c "
import json, os, sys

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

# Load .cyclo-allow if present. Each line is 'path::function [# rationale]'.
allowed = {}  # (path, function_name) -> rationale
allow_path = os.environ.get('CYCLO_ALLOW_FILE','')
if allow_path:
    with open(allow_path) as f:
        for raw in f:
            line = raw.split('#', 1)
            spec = line[0].strip()
            rationale = line[1].strip() if len(line) > 1 else ''
            if not spec:
                continue
            if '::' not in spec:
                continue
            path, fn = spec.split('::', 1)
            allowed[(path.strip(), fn.strip())] = rationale

show_allowed = os.environ.get('CYCLO_SHOW_ALLOWED','0') == '1'

shown = 0
suppressed = []
for cc, name, f, line in results:
    if cc <= thresh:
        break
    if (f, name) in allowed:
        # Always count suppressed, even past the display window, so the
        # summary reports the true filtered total.
        suppressed.append((cc, name, f, line, allowed[(f, name)]))
        continue
    if shown >= top_n:
        continue
    print(f'{cc:4d}  {name:40s} {f}:{line}')
    shown += 1

if show_allowed and suppressed:
    print('\n--- allowed (filtered) ---', file=sys.stderr)
    for cc, name, f, line, why in suppressed:
        why_s = f'  # {why}' if why else ''
        print(f'{cc:4d}  {name:40s} {f}:{line}{why_s}', file=sys.stderr)

print(f'\n--- {count} functions, avg {avg:.1f}, max {results[0][0] if results else 0}, {len(suppressed)} allow-filtered ---', file=sys.stderr)
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
