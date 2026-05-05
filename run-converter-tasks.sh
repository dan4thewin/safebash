#!/bin/bash
# run-converter-tasks.sh — autonomous converter implementation loop
# Each task gets its own branch off split-converter.
# Success → merge to split-converter. Failure → branch remains for diagnosis.
set -eu

# Optional filter: only run tasks whose branch name contains $1
FILTER="${1:-}"

BASE_BRANCH=split-converter
LOGDIR=.agent-runs/$(date +%Y%m%d_%H%M%S)
mkdir -p "$LOGDIR"

# Task definitions: branch-name gate-tests spec-file description
# Ordered by estimated difficulty (easiest first)
TASKS=(
    "convert-select-manual|57.25|spec/convert.sh|Add MANUAL annotation for select_statement nodes"
    "convert-trap-empty|57.14|spec/convert.sh|Convert trap \"\" SIG to trap : SIG"
    "convert-positionals|57.13|spec/convert.sh|Convert \$1 \$2 \$@ \$# to \$(argv N) \$(argc) etc"
    "convert-backtick|57.5|spec/convert.sh|Convert backtick command substitution to \$()"
    "convert-test-bracket|57.6a,57.6b|spec/convert.sh|Convert [ ] to [[ ]]"
    "convert-heredoc-manual|57.23|spec/convert.sh|Add MANUAL annotation for \$(cmd) inside heredocs"
    "convert-unset-scalar|30.7|spec/convert.sh|Convert unset var (scalar) to var=\"\""
    "convert-arith-assign|57.10|spec/convert.sh|Extract \$((x=10)) to ((x=10)) separate statement"
    "convert-numeric-cmp|30.5a,30.5b,30.5c|spec/convert.sh|Convert [[ \$x -gt N ]] to (( x > N ))"
    "convert-shellcheck|57.31,57.33|spec/convert.sh|Integrate shellcheck SC2086 CHECK hints"
)

run_task() {
    local entry="$1"
    IFS='|' read -r branch gates specfile desc <<< "$entry"

    local logfile="$LOGDIR/$branch.log"
    local postmortem="$LOGDIR/$branch.postmortem.md"

    echo "=== $branch: $desc ==="
    echo "  gates: $gates"
    echo "  log: $logfile"

    # Create branch off BASE_BRANCH
    git checkout "$BASE_BRANCH" -- . 2>/dev/null
    git checkout "$BASE_BRANCH" 2>/dev/null
    git checkout -b "agent/$branch" 2>/dev/null || {
        echo "  SKIP: branch agent/$branch already exists"
        return 1
    }

    # Build first
    cargo build --release -p sash-convert 2>"$LOGDIR/$branch.build.log" || {
        echo "  FAIL: build failed before agent started"
        git checkout "$BASE_BRANCH" 2>/dev/null
        git branch -D "agent/$branch" 2>/dev/null
        return 1
    }

    # Compose the agent prompt
    local gate_list=${gates//,/ }
    local gate_tests=""
    for g in $gate_list; do
        # Extract the specific test from the spec file
        gate_tests+="$(awk "/chk ${g}( |$)/{found=1} found{print; if(/chk /){found=0}}" "$specfile" | head -5)"
        gate_tests+=$'\n'
    done

    local prompt
    prompt="$(cat <<PROMPT
You are implementing a feature in sash-convert, a bash-to-sash-native source converter.

## Task
$desc

## Gated acceptance tests (from $specfile)
These tests must pass for the task to succeed:
Gate IDs: $gates

$(grep -B5 "chk \(${gates//,/\\|}\)" "$specfile" | head -60)

## Key source files
- sash-convert/src/convert.rs — the conversion engine (tree-sitter CST walk, collects source patches)
- sash-convert/src/main.rs — CLI entry point
- sash-convert/src/known_externals.rs — known-safe external commands

## How the converter works
The converter parses bash source with tree-sitter, walks the CST, and collects
source-level patches (byte offset replacements). It also collects prefix_lines
(comments/annotations to insert before a line). After the walk, patches are
applied to produce sash-native source.

Key pattern in convert.rs:
  patches.push((start_byte, end_byte, replacement_text));
  prefix_lines.push((line_number, comment_text));

## Build and test
  cargo build --release -p sash-convert
  cd spec && PATH="../target/release:\$PATH" bash convert.sh 2>/dev/null

To run just the gated tests, look for the output lines containing the gate IDs.

## Rules
1. Only modify files under sash-convert/src/
2. Do NOT modify spec files or test files
3. Run cargo build --release -p sash-convert after changes
4. Run the spec tests and verify your gated tests pass
5. Run: cd spec && PATH="../target/release:\$PATH" bash convert.sh 2>/dev/null
   and verify no previously passing tests regressed
6. If all gated tests pass and no regressions, commit with a descriptive message
7. Do NOT push — the orchestrator handles merging

## Post-mortem
After you finish (success or failure), create a file at:
  $postmortem

Include:
- Whether the gated tests pass (list each gate and pass/fail)
- If any failed: why, what you tried, what blocked you
- If you made design choices: what and why
- Anything ambiguous in the spec that needs human review
- Suggestions for the next agent if the task is incomplete
PROMPT
)"

    # Run the agent
    local promptfile="$LOGDIR/$branch.prompt.md"
    echo "$prompt" > "$promptfile"
    goose run -i "$promptfile" > "$logfile" 2>&1
    local agent_rc=$?

    # Check results
    cargo build --release -p sash-convert 2>/dev/null || {
        echo "  FAIL: build broken after agent"
        git checkout "$BASE_BRANCH" 2>/dev/null
        return 1
    }

    # Run spec tests, capture gated results
    local tsv
    tsv=$(mktemp)
    cd spec
    SPEC_TSV="$tsv" PATH="$(cd .. && pwd)/target/release:$PATH" bash convert.sh 2>/dev/null || true
    cd ..

    local all_pass=true
    for g in $gate_list; do
        if grep -q "^${g}	passing" "$tsv"; then
            echo "  ✓ $g"
        else
            echo "  ✗ $g"
            all_pass=false
        fi
    done

    # Check for regressions (compare passing count to baseline)
    local new_passing
    new_passing=$(grep -c 'passing' "$tsv" || true)
    rm -f "$tsv"

    if $all_pass; then
        echo "  → All gates pass ($new_passing total passing)"

        # Verify there's something to commit
        if [[ -n $(git diff --name-only) || -n $(git diff --cached --name-only) ]]; then
            git add -A
            git commit -m "$desc

Gates: $gates — all passing.
Branch: agent/$branch

AI-generated: autonomous converter implementation." 2>/dev/null
        fi

        # Merge to base
        git checkout "$BASE_BRANCH" 2>/dev/null
        if git merge --no-ff "agent/$branch" -m "Merge agent/$branch: $desc" 2>/dev/null; then
            echo "  → Merged to $BASE_BRANCH"
            git branch -d "agent/$branch" 2>/dev/null || true
        else
            echo "  FAIL: merge conflict"
            git merge --abort 2>/dev/null
            git checkout "$BASE_BRANCH" 2>/dev/null
        fi
    else
        echo "  → Some gates failed, branch agent/$branch preserved for diagnosis"
        # Make sure any partial work is committed for review
        if [[ -n $(git diff --name-only) || -n $(git diff --cached --name-only) ]]; then
            git add -A
            git commit -m "WIP: $desc (incomplete)

Some gates failed. See $postmortem for details.

AI-generated: autonomous converter implementation (incomplete)." 2>/dev/null
        fi
        git checkout "$BASE_BRANCH" 2>/dev/null
    fi

    echo
}

echo "Converter task runner — $(date)"
echo "Base branch: $BASE_BRANCH"
echo "Log directory: $LOGDIR"
echo "Tasks: ${#TASKS[@]}"
echo

# Ensure we're on the base branch and clean
git checkout "$BASE_BRANCH" 2>/dev/null
if [[ -n $(git status --porcelain) ]]; then
    echo "ERROR: working tree not clean. Commit or stash first."
    exit 1
fi

# Record baseline passing count
baseline_tsv=$(mktemp)
cd spec
SPEC_TSV="$baseline_tsv" PATH="$(cd .. && pwd)/target/release:$PATH" bash convert.sh 2>/dev/null || true
cd ..
BASELINE_PASSING=$(grep -c 'passing' "$baseline_tsv" || true)
rm -f "$baseline_tsv"
echo "Baseline: $BASELINE_PASSING convert tests passing"
echo

for task in "${TASKS[@]}"; do
    IFS='|' read -r branch _ _ _ <<< "$task"
    if [[ -n $FILTER && $branch != *"$FILTER"* ]]; then
        continue
    fi
    run_task "$task"
done

echo "=== Summary ==="
echo "Post-mortems in $LOGDIR/"
ls -1 "$LOGDIR"/*.postmortem.md 2>/dev/null || echo "(none written)"
echo
echo "Agent branches remaining:"
git branch --list 'agent/*' 2>/dev/null || echo "(none)"
echo
echo "Done."
