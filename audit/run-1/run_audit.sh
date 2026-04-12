#!/usr/bin/env bash
# Run the adversarial audit matrix
# 3 prompts × 7 models = 21 runs
# Usage: ./run_audit.sh [pass] [model]
#   No args = run all. Pass = bypass|ir|spec. Model = specific model.

set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
cd /home/dangood/src/safebash

MODELS=(
    gpt-4.1
    gpt-5.2
    gpt-5.3-codex
    claude-sonnet-4
    claude-opus-4.5
    claude-opus-4.6
    gemini-2.5-pro
)

PASSES=(
    bypass:PROMPT_BYPASS.txt
    ir:PROMPT_IR_FIDELITY.txt
    spec:PROMPT_SPEC_COVERAGE.txt
)

run_one() {
    local pass="$1" prompt_file="$2" model="$3"
    local outfile="$DIR/${pass}_${model}.txt"

    if [[ -f "$outfile" ]] && [[ -s "$outfile" ]]; then
        echo "SKIP $pass/$model (already exists)"
        return 0
    fi

    echo "START $pass/$model ..."
    if ~/bin/copilot -p "$(cat "$DIR/$prompt_file")" --model "$model" \
        > "$outfile" 2>&1; then
        echo "DONE  $pass/$model ($(wc -l < "$outfile") lines)"
    else
        echo "FAIL  $pass/$model (exit $?)"
    fi
}

filter_pass="${1:-}"
filter_model="${2:-}"

for entry in "${PASSES[@]}"; do
    pass="${entry%%:*}"
    prompt="${entry#*:}"

    [[ -n "$filter_pass" ]] && [[ "$pass" != "$filter_pass" ]] && continue

    for model in "${MODELS[@]}"; do
        [[ -n "$filter_model" ]] && [[ "$model" != "$filter_model" ]] && continue
        run_one "$pass" "$prompt" "$model" &
    done
    # Wait for each pass to complete before starting the next
    # (avoids overwhelming the API)
    wait
done

echo "=== All runs complete ==="
echo "Output files:"
ls -la "$DIR"/*.txt | grep -v PROMPT
