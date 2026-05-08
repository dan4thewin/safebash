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

TASKS=(
  # 1-gate tasks
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
    5|6|7|8|9|10|11|15|32|36|37|38|52) echo "spec/misc.sh" ;;
    12|14|17|23|54) echo "spec/commands.sh" ;;
    16|33|35|41) echo "spec/types.sh" ;;
    18|42|46|48) echo "spec/functions.sh" ;;
    20|22|28|39|40) echo "spec/wordsplit.sh" ;;
    24|50|53) echo "spec/taint.sh" ;;
    25|30|31) echo "spec/arithmetic.sh" ;;
    55) echo "spec/errors.sh" ;;
    56) echo "spec/signals.sh" ;;
    57) echo "spec/convert.sh spec/compiler.sh" ;;
    *) echo "spec/misc.sh" ;;
  esac
}

# Extract example code from task file's example: field.
# Format: example: file:symbol_name
extract_example() {
  local taskfile="$1"
  local example_line
  example_line=$(sed -n 's/^example: *//p' "$taskfile") || true
  [[ -z "$example_line" ]] && return 0

  local file="${example_line%%:*}"
  local symbol="${example_line#*:}"
  [[ -z "$symbol" || "$symbol" = "$file" ]] && return 0

  local range
  range=$(sqlite3 .indexnudge.db \
    "SELECT start_line, end_line FROM symbols
     WHERE name = '${symbol}' AND file = '${file}' LIMIT 1" 2>/dev/null) || true

  if [[ -n "$range" ]]; then
    local start="${range%%|*}" end="${range##*|}"
    echo "## Example pattern (${file}:${symbol}, lines ${start}-${end})"
    echo '```rust'
    sed -n "${start},${end}p" "$file"
    echo '```'
  fi
}

# Extract specific gate assertion lines from spec files.
extract_gate_lines() {
  local gates="$1"; shift

  echo "## Gate assertions (must pass)"
  echo "Each gate is a test in a spec file. Your task succeeds when these pass."
  echo ""

  for g in $gates; do
    for sf in "$@"; do
      [[ -f "$sf" ]] || continue
      local match
      match=$(/bin/grep -nE "chk ${g}( |$)" "$sf" 2>/dev/null | head -1) || true
      if [[ -n "$match" ]]; then
        local chk_line="${match%%:*}"
        local start=$((chk_line - 5))
        (( start < 1 )) && start=1
        echo "### Gate $g ($sf:$chk_line)"
        echo '```bash'
        sed -n "${start},${chk_line}p" "$sf"
        echo '```'
        echo ""
      fi
    done
  done
}

# Query .indexnudge.db for symbols/references related to a keyword.
index_context() {
  local keyword="$1"
  echo "## Index context for '${keyword}'"
  echo ""
  echo "### Symbols matching '${keyword}'"
  echo '```'
  sqlite3 .indexnudge.db \
    "SELECT name, kind, file, start_line, end_line
     FROM symbols
     WHERE (name LIKE '%${keyword}%' OR qualified_name LIKE '%${keyword}%')
       AND file NOT LIKE 'exp/%'
     ORDER BY file, start_line" 2>/dev/null | head -30
  echo '```'
  echo ""
  echo "### References in sash/src/"
  echo '```'
  sqlite3 .indexnudge.db \
    "SELECT file, line_num, SUBSTR(content, 1, 120)
     FROM lines
     WHERE content LIKE '%${keyword}%'
       AND file LIKE 'sash/src/%'
     ORDER BY file, line_num" 2>/dev/null | head -40
  echo '```'
  echo ""
  echo "### References in tests"
  echo '```'
  sqlite3 .indexnudge.db \
    "SELECT file, line_num, SUBSTR(content, 1, 120)
     FROM lines
     WHERE content LIKE '%${keyword}%'
       AND file LIKE 'sash/tests/%'
     ORDER BY file, line_num" 2>/dev/null | head -20
  echo '```'
}

for task in "${TASKS[@]}"; do
  [[ -n "$FILTER" && "$task" != *"$FILTER"* ]] && continue

  branch="agent/$task"
  taskfile="notes/tasks/${task}.md"
  logfile="$LOGDIR/${task}.log"
  postmortem="$LOGDIR/${task}-postmortem.md"

  echo "═══════════════════════════════════════════════════"
  echo "TASK: $task"
  echo "═══════════════════════════════════════════════════"

  if [[ ! -f "$taskfile" ]]; then
    echo "SKIP: no task file"; continue
  fi

  # Parse gates from frontmatter
  gates=$(sed -n 's/^gates: *\[//p' "$taskfile" | sed 's/\].*//; s/,/ /g')
  if [[ -z "$gates" ]]; then
    echo "SKIP: no gates"; continue
  fi

  # Parse index_keyword (optional, falls back to last segment of task name)
  index_keyword=$(sed -n 's/^index_keyword: *//p' "$taskfile") || true
  [[ -z "$index_keyword" ]] && index_keyword="${task##*-}"

  # Determine spec files for gates
  spec_files=()
  seen_specs=""
  for g in $gates; do
    for sf in $(spec_file_for_gate "$g"); do
      if [[ "$seen_specs" != *"$sf"* ]]; then
        spec_files+=("$sf")
        seen_specs="$seen_specs $sf"
      fi
    done
  done

  # Clean state — fresh branch from base
  git checkout "$BASE_BRANCH" -- . 2>/dev/null || true
  git checkout "$BASE_BRANCH" 2>/dev/null
  git branch -D "$branch" 2>/dev/null || true
  git checkout -b "$branch"

  # Build prompt sections
  task_content=$(cat "$taskfile")
  gate_block=$(extract_gate_lines "$gates" "${spec_files[@]}")
  example_block=$(extract_example "$taskfile")
  index_block=$(index_context "$index_keyword")

  prompt="You are implementing a compiler task for safebash — a compiler from
a safe Bash subset to static native binaries.

## Your task
${task_content}

${gate_block}

${example_block:-}

${index_block}

## How to navigate the codebase

You have a code index at \`.indexnudge.db\` (refreshed automatically by the
sqlite3 shim). Use sqlite3 queries to find exactly what you need.
**DO NOT read entire files.** The key files are thousands of lines long.
Use the index to find the exact lines, then \`sed -n 'START,ENDp' FILE\`.

### Find a symbol (function, struct, enum) by name
\`\`\`sql
SELECT name, kind, file, start_line, end_line
FROM symbols WHERE name LIKE '%keyword%' AND file NOT LIKE 'exp/%';
\`\`\`

### Read a specific function (use line range from symbols table)
\`\`\`bash
sed -n 'START,ENDp' FILE
\`\`\`

### Find all references to a name in compiler source
\`\`\`sql
SELECT file, line_num, content FROM lines
WHERE content LIKE '%keyword%' AND file LIKE 'sash/src/%'
ORDER BY file, line_num;
\`\`\`

### Find what calls a function
\`\`\`sql
SELECT caller_file, caller_symbol, line_num FROM calls
WHERE callee = 'function_name' AND caller_file NOT LIKE 'exp/%';
\`\`\`

### Find what a function calls
\`\`\`sql
SELECT callee, line_num FROM calls
WHERE caller_symbol = 'function_name' AND caller_file = 'sash/src/codegen.rs';
\`\`\`

### Find which symbol owns a line number
\`\`\`sql
SELECT DISTINCT symbol FROM lines
WHERE file = 'FILE' AND line_num = LINE_NUM;
\`\`\`

### Full-text search (exact word match)
\`\`\`sql
SELECT file, line_num, content FROM lines_fts
WHERE content MATCH '\"keyword\"' AND file NOT LIKE 'exp/%';
\`\`\`

## Work rules

1. **Compile after every edit.** \`cargo build --release -p sash\`
   Fix errors before editing more files.
2. **Test frequently.** \`cargo test --release -p sash 2>&1 | tail -5\`
   Do this after every logical change. Zero regressions.
3. **Never read entire large files.** Use index queries + sed ranges.
   codegen.rs is 4500 lines. lower.rs is 2500 lines.
4. **Do NOT modify** spec files (spec/*.sh), task files (notes/tasks/),
   or anything in exp/.
5. **sash-rt is a separate crate** excluded from the workspace.
   Build with: \`cd sash-rt && cargo build --release\`
6. **After all edits, verify:**
   \`\`\`bash
   cargo test --release -p sash 2>&1 | tail -3    # all tests pass
   cd spec && bash SPEC_FILE 3>&1 2>/dev/null      # gated spec tests
   \`\`\`
7. **Always commit** with a descriptive message when gates pass.
   If you made changes but gates don't pass, commit anyway on the branch
   with a message explaining what's incomplete.
8. **Do NOT push.** Do not run \`git push\`. The harness handles merging.

## Anti-patterns (DO NOT do these)

- **Do NOT suppress stderr globally.** Never add \`Stdio::null()\`,
  \`Stdio::piped()\`, \`2>/dev/null\`, or \`.stderr(...)\` on compiler,
  linker, or external command invocations. If ld or shellcheck produce
  warnings, that's acceptable — the spec test harness already handles it.
- **Do NOT change eprintln to println** or remove user-facing messages
  to make tests pass. The spec harness filters stderr appropriately.
- **Do NOT delete user-facing output** (like success messages) to make
  tests pass. If a test is failing because of stderr output, the fix is
  to filter in the test harness or fix the message format, not to remove it.
  But since you can't modify spec files, look for a targeted fix.
- **Do NOT read AGENTS.md.** Everything you need is in this prompt.
- **Do NOT read entire source files.** Use the index.
- **Do NOT make speculative changes** to files not mentioned in the task.
  If you think something else needs changing, note it but don't do it.

## Key files (do NOT read whole — use index)
- sash/src/codegen.rs — Cranelift codegen (4500 lines)
- sash/src/lower.rs — CST→IR lowering (2500 lines)
- sash/src/transform.rs — IR→SSA (2600 lines)
- sash/src/ir.rs — IR definitions
- sash/src/ssa.rs — SSA instructions
- sash/src/ssa_taint.rs — SSA taint checks
- sash/src/type_check.rs — type checking
- sash/src/taint.rs — CST taint analysis
- sash/src/validator.rs + sash/src/rules/ — validation
- sash-rt/src/lib.rs — runtime (2600 lines, no_std)
- sash/tests/codegen_tests.rs — codegen tests (3000 lines)
- sash/tests/e2e_tests.rs — e2e tests (1200 lines)
"

  prompt_file="$LOGDIR/${task}-prompt.md"
  echo "$prompt" > "$prompt_file"
  echo "Running goose..."
  if goose run -i "$prompt_file" > "$logfile" 2>&1; then
    echo "Goose completed"
  else
    echo "Goose exited with error"
  fi

  tail -50 "$logfile" > "$postmortem"

  # Restore spec/notes from base (agent shouldn't have touched them)
  git checkout "$BASE_BRANCH" -- spec/ notes/ 2>/dev/null || true

  # Verify no regressions
  if cargo test --release -p sash 2>&1 | /bin/grep -q "^error: test failed"; then
    echo "Cargo tests: FAIL (regressions)"
    git checkout "$BASE_BRANCH"
    continue
  else
    echo "Cargo tests: PASS"
  fi

  # Check gated spec tests
  all_gates_pass=true
  spec_tsv="/tmp/spec_gates.tsv"
  rm -f "$spec_tsv"
  for sf in "${spec_files[@]}"; do
    [[ -f "$sf" ]] && SPEC_TSV="$spec_tsv" bash "$sf" 3>/dev/null 2>/dev/null || true
  done
  for g in $gates; do
    if /bin/grep -qP "^${g}\tpassing\t" "$spec_tsv" 2>/dev/null; then
      echo "GATE PASS: $g"
    else
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
