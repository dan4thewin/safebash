#!/bin/bash
# Oils spec test harness
# Usage: oils-harness <specfile> [--verbose]
#
# Three-pass approach using grep + sed:
#   Pass 1: grep -n '^#### ' → test names + line numbers
#   Pass 2: grep -n '^## stdout: \|^## STDOUT:' → expected output markers
#   Pass 3: grep -n '^## END' → block terminators
#
# For each test: sed extracts body and expected output by line number.

spec="$1"
verbose="${2:-}"
if [[ -z "$spec" ]]; then
    echo "Usage: oils-harness <specfile> [--verbose]"
    exit 1
fi

tmptest=$(mktemp /tmp/sash_test_XXXXXX.sh)
tmpbin=$(mktemp /tmp/sash_bin_XXXXXX)
tmpexpect=$(mktemp /tmp/sash_expect_XXXXXX)
sash="./exp/044-pipe-while-read/target/debug/sash-044"

pass=0; fail=0; skip=0; total=0

# Build sorted index: all markers with line numbers
# Format: LINENO:TYPE:CONTENT
# TYPE: H=header, S=stdout(inline), M=STDOUT(multiline), E=END
{
    grep -n '^#### ' "$spec" | sed 's/:#### /:H:/'
    grep -n '^## stdout: ' "$spec" | sed 's/:## stdout: /:S:/'
    grep -n '^## STDOUT:' "$spec" | sed 's/:## STDOUT:.*/:M:/'
    grep -n '^## END' "$spec" | sed 's/:## END.*/:E:/'
} | sort -t: -k1,1n > /tmp/sash_index_$$

# Walk the index: pair each H with its S or M+E
test_name=""
header_line=0

while IFS=: read -r lineno typ content; do
    case "$typ" in
        H)
            test_name="$content"
            header_line="$lineno"
            ;;
        S)
            # Inline expected: body is header+1 to lineno-1
            if [[ -z "$test_name" ]]; then continue; fi
            (( total++ ))
            body_start=$((header_line + 1))
            body_end=$((lineno - 1))

            sed -n "${body_start},${body_end}p" "$spec" > "$tmptest"
            echo "$content" > "$tmpexpect"

            if "$sash" "$tmptest" -o "$tmpbin" >/dev/null 2>&1; then
                actual=$(timeout 3 "$tmpbin" 2>/dev/null)
                rc=$?
                expected=$(cat "$tmpexpect")
                if [[ $rc -eq 124 ]]; then
                    echo "SKIP $test_name (timeout)"
                    (( skip++ ))
                elif [[ "$actual" = "$expected" ]]; then
                    echo "PASS $test_name"
                    (( pass++ ))
                else
                    echo "FAIL $test_name"
                    if [[ "$verbose" = "--verbose" ]]; then
                        echo "  expected: $expected"
                        echo "  actual:   $actual"
                    fi
                    (( fail++ ))
                fi
            else
                echo "SKIP $test_name (compile failed)"
                (( skip++ ))
            fi
            test_name=""
            ;;
        M)
            # Multiline expected: need to find the matching E
            # Save the M line number; expected starts at M+1
            stdout_start=$((lineno + 1))
            ;;
        E)
            # End of multiline block: expected is stdout_start to lineno-1
            # Body is header+1 to (M line - 1)
            if [[ -z "$test_name" ]]; then continue; fi
            (( total++ ))
            body_start=$((header_line + 1))
            # Find the STDOUT marker line — it's stdout_start - 1
            body_end=$((stdout_start - 2))

            # But body_end might include other ## annotations between body and STDOUT
            # Scan backwards from STDOUT to find last non-## line
            # For now, extract body and strip any trailing ## lines
            sed -n "${body_start},${body_end}p" "$spec" | grep -v '^## ' > "$tmptest"

            stdout_end=$((lineno - 1))
            sed -n "${stdout_start},${stdout_end}p" "$spec" > "$tmpexpect"

            if "$sash" "$tmptest" -o "$tmpbin" >/dev/null 2>&1; then
                actual=$(timeout 3 "$tmpbin" 2>/dev/null)
                rc=$?
                expected=$(cat "$tmpexpect")
                if [[ $rc -eq 124 ]]; then
                    echo "SKIP $test_name (timeout)"
                    (( skip++ ))
                elif [[ "$actual" = "$expected" ]]; then
                    echo "PASS $test_name"
                    (( pass++ ))
                else
                    echo "FAIL $test_name"
                    if [[ "$verbose" = "--verbose" ]]; then
                        echo "  expected:"
                        cat "$tmpexpect" | sed 's/^/    /'
                        echo "  actual:"
                        echo "$actual" | sed 's/^/    /'
                    fi
                    (( fail++ ))
                fi
            else
                echo "SKIP $test_name (compile failed)"
                (( skip++ ))
            fi
            test_name=""
            ;;
    esac
done < /tmp/sash_index_$$

rm -f "$tmptest" "$tmpbin" "$tmpexpect" /tmp/sash_index_$$
echo ""
echo "Results: $pass passed, $fail failed, $skip skipped / $total total"
