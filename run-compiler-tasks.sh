#!/usr/bin/env bash
# run-compiler-tasks.sh — autonomous loop for spec-gated compiler tasks.
# Each task runs on its own branch. Success → merge back. Failure → branch remains for diag.
#
# Usage: ./run-compiler-tasks.sh [filter]
#   filter: optional substring to match task names

set -euo pipefail

BASE_BRANCH="split-converter"
FILTER="${1:-}"
LOGDIR=".agent-runs"
mkdir -p "$LOGDIR"

# Tasks ordered by gate count (simplest first), no unresolved dependencies,
# not already passing. Excluded: tasks requiring design decisions or infra work.
TASKS=(
  # 1-gate tasks (trivial)
  dd56-signum-keyword
  dd21-grammar-dash-prefix
  dd23-remove-command-v
  dd23-remove-path-walk
  heredoc-cmdsub
  source-rejection

  # 2-gate tasks
  dd23-compile-time-resolve
  dd32-sash-source-section
  dd35-decl-stacking
  dd48-shift-tracking
  dd52-case-conversion
  implement-failglob
  janet-parity-gate
  dd55-dollar-question-scoping

  # 3-gate tasks
  compound-redirects
  dd25-overflow-traps
  dd46-reject-toplevel-positionals
  dd49-bless-anti-tautology
  if-bless-conditional
  dd55-suppress-prefix
  dd21-implement-dash-prefix

  # 4-gate tasks
  dd18-local-functions
  dd30-arith-operand-typecheck
  dd31-reject-mutation-in-expr
  dd36-decimal-conversion
  dd39-compiletime-regex-validation
  dd42-builtins
  dd55-nonvoid-functions
  mapfile-map-mode

  # 5+ gate tasks
  bug-exit-trap-wrong-scope
  dd30-separate-test-arith
  refine-unset-semantics
  dd33-array-map-types
  dd57-strip-bash
  dd22-wordsplit-runtime
  dd55-implicit-exit-checks

  # Large tasks
  dd12-string-runtime
  dd14-pipeline-primitives
  dd16-integer-type
  dd23-command-v-coercion
  array-per-element-ops
  dd39-regex-mutation
  review-taint-clean-list
)

spec_file_for_gate() {
  local gate="$1"
  local prefix="${gate%%.*}"
  case "$prefix" in
    12|14|17|23) echo "spec/commands.sh" ;;
    15) echo "spec/misc.sh" ;;
    16|33|35|41) echo "spec/types.sh" ;;
    18|42|46|48) echo "spec/functions.sh" ;;
    20|22|28|39|40) echo "spec/wordsplit.sh" ;;
    24|50|53|54) echo "spec/taint.sh" ;;
    25|30|31) echo "spec/arithmetic.sh" ;;
    32|36|37|38|52) echo "spec/misc.sh" ;;
    55) echo "spec/errors.sh" ;;
    56) echo "spec/signals.sh" ;;
    57) echo "spec/convert.sh spec/compiler.sh" ;;
    *) echo "spec/misc.sh" ;;
  esac
}

for task in "${TASKS[@]}"; do
  # Apply filter
  [[ -n "$FILTER" && "$task" != *"$FILTER"* ]] && continue

  branch="agent/$task"
  taskfile="notes/tasks/${task}.md"
  logfile="$LOGDIR/${task}.log"
  postmortem="$LOGDIR/${task}-postmortem.md"

  echo "═══════════════════════════════════════════════════"
  echo "TASK: $task"
  echo "═══════════════════════════════════════════════════"

  # Skip if task file doesn't exist
  if [[ ! -f "$taskfile" ]]; then
    echo "SKIP: no task file"
    continue
  fi

  # Read gates
  gates=$(grep '^gates:' "$taskfile" | sed 's/gates: *//; s/\[//; s/\]//; s/,/ /g')
  if [[ -z "$gates" ]]; then
    echo "SKIP: no gates"
    continue
  fi

  # Determine which spec files contain these gates
  spec_files=""
  for g in $gates; do
    for sf in $(spec_file_for_gate "$g"); do
      [[ "$spec_files" != *"$sf"* ]] && spec_files="$spec_files $sf"
    done
  done

  # Clean state
  git checkout "$BASE_BRANCH" -- . 2>/dev/null || true
  git checkout "$BASE_BRANCH" 2>/dev/null
  git branch -D "$branch" 2>/dev/null || true
  git checkout -b "$branch"

  # Build prompt
  task_content=$(cat "$taskfile")
  spec_content=""
  for sf in $spec_files; do
    [[ -f "$sf" ]] && spec_content="${spec_content}
--- $sf ---
$(cat "$sf")"
  done

  prompt="You are implementing a compiler feature for the safebash project.

## Task
$task_content

## Gated tests (must pass): $gates

## Spec test files
$spec_content

## Instructions
1. Read AGENTS.md for project conventions
2. Read the relevant source files in sash/src/
3. Implement the feature to make the gated tests pass
4. Run: cargo test --release -p sash (must pass — no regressions)
5. Run the specific spec tests to verify gates pass
6. Commit your changes with a descriptive message
7. Write a brief post-mortem to stdout explaining what you did, any design choices, and anything ambiguous

IMPORTANT:
- Do NOT modify spec test files — they define the acceptance criteria
- Do NOT modify notes/tasks/ files
- All existing tests must continue to pass (no regressions)
- If you cannot implement the feature, explain why in the post-mortem
- sash-rt is excluded from workspace; build with: cd sash-rt && cargo build --release
- The binary links sash-rt at compile time from sash-rt/target/release/libsash_rt.a
"

  # Run goose
  prompt_file="$LOGDIR/${task}-prompt.md"
  echo "$prompt" > "$prompt_file"
  echo "Running goose..."
  if goose run -i "$prompt_file" > "$logfile" 2>&1; then
    echo "Goose completed"
  else
    echo "Goose exited with error"
  fi

  # Extract post-mortem (last markdown section from log)
  tail -50 "$logfile" > "$postmortem"

  # Check results
  git checkout "$BASE_BRANCH" -- spec/ notes/ 2>/dev/null || true

  # Verify no regressions
  if (cd sash && cargo test --release 2>&1 | tail -1 | grep -q "^test result: ok"); then
    echo "Cargo tests: PASS"
  else
    echo "Cargo tests: FAIL (regressions)"
    git checkout "$BASE_BRANCH"
    continue
  fi

  # Check gated spec tests
  all_gates_pass=true
  for sf in $spec_files; do
    [[ -f "$sf" ]] && bash "$sf" > /tmp/spec_out.txt 2>&1 || true
  done
  for g in $gates; do
    if ! grep -q "^ok.*$g" /tmp/spec_out.txt 2>/dev/null; then
      all_gates_pass=false
      echo "GATE FAIL: $g"
    fi
  done

  if $all_gates_pass; then
    echo "ALL GATES PASS — merging"
    git add -A
    git commit -m "Implement $task (autonomous)" --allow-empty 2>/dev/null || true
    git checkout "$BASE_BRANCH"
    git merge --no-ff "$branch" -m "Merge $task (all gates pass)"
    git branch -d "$branch"
  else
    echo "SOME GATES FAILED — branch $branch remains for diagnosis"
    git checkout "$BASE_BRANCH"
  fi

  echo ""
done

echo "Done. Check $LOGDIR/ for logs and post-mortems."
