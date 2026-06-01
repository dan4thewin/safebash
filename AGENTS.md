# AGENTS.md — Field Notes for AI Contributors

## What this project is

safebash is a compiler for a safe subset of Bash that produces
static native binaries. DG created the project and directs the
effort. The AI implements: writing Rust, designing IRs, writing
docs. Neither works alone — DG makes design decisions; the AI has
the stamina for incremental implementation.

The compiler works end-to-end: bash source → parse → validate →
lower → type check → transform → fold → codegen → link → static ELF.
Output is byte-identical to Bash. Also supports a `--janet` backend
and `--emit-source`/`--annotate`/`--extract` modes.

Read the journal prologue (`notes/journal/001-prologue.md`) before
your first session. It sets the tone for everything.

## Decision authority

DG tasks the AI with implementing and expects all important
decisions to be discussed before acting — design, architecture,
implementation strategy, or anything else that matters. The AI
does not get to decide what's "just implementation" and skip the
conversation. When in doubt, present options with tradeoffs.

## First thing every session

Read `notes/status.md`. It tells you the current priority and
what was just completed.

Read `notes/decisions/` before changing any behavior that might
be deliberate. These are the authoritative source for what's
intentional vs. what's a gap.

## Current state

- **sash/** — the compiler (Rust, Cranelift backend)
- **sash-rt/** — pure Rust `#![no_std]` runtime crate (rustix, no libc)
- **sash-convert/** — bash → sash-native converter
- **sash-expand/** — expansion utilities
- **sash-rt-tests/** — runtime test harness
- **docs/** — GUIDE, SUBSET, TIMELINE
- **notes/** — iwe knowledge graph: journal, decisions, tasks, status, architecture
- **audit/** — adversarial audit findings and remediation
- **contrib/** — contributed scripts/tools
- **suites/** — test suites
- **exp/** — git submodule → safebash-exp (historical experiments, not needed for build)
- **spec.db** — SQLite database of all spec test results (root of repo)
- **46 design decisions** in `notes/decisions/` (one file per DD)
- **1 adversarial audit** completed (5 models, 3 passes, remediation done)
- **Tree-sitter fork** at `~/src/tree-sitter-bash-safebash` with custom grammar
  (int, const, array, map, bless, fn, `${var:s/re/str/}`, suppressed_command)

### Crate structure

`main.rs` is a thin 4-line wrapper calling `sash::cli_main()`.
All compiler logic lives in `lib.rs`. Internal modules use
`pub(crate)` — Rust warns on dead code. Modules used by
integration tests are `pub`.

### Pipeline

tree-sitter CST → validate (rules + taint) → lower (CST→IR, coercions)
→ type check → transform (IR→SSA) → fold → codegen (Cranelift) → link → static ELF

DD#58 arithmetic sexpr: source preprocessor replaces `(func args...)`
inside `$(( ))` with placeholders before tree-sitter, transform
resolves via global `ARITH_MAP`.

### Key source files (`sash/src/`)

- `lib.rs` — entry point, cli_main, arithmetic preprocessor
- `lower/` — CST → IR lowering (7 files)
  - `mod.rs` — lower_program, lower_body
  - `statements.rs` — statement/declaration/command lowering
  - `control_flow.rs` — if/while/for/case
  - `functions.rs` — function definitions, redirects
  - `expressions.rs` — expression lowering
  - `arithmetic.rs` — (( )) and C-style for
  - `helpers.rs` — utilities
- `ir.rs` — IR definitions
- `type_check.rs` — type checking pass
- `transform/` — IR → SSA transformation (9 files)
  - `entry.rs` — ir_to_ssa, ir_to_ssa_typed
  - `statements.rs` — lower_one dispatch
  - `control_flow.rs` — if/while/for/case → SSA blocks
  - `expressions.rs` — expr_to_value
  - `functions.rs` — int-func lifting, inlining
  - `pipelines.rs` — pipeline decomposition
  - `resolution.rs` — DD#23 command resolution
  - `helpers.rs` — phi insertion, positional scanning
- `ssa.rs`, `ssa_printer.rs` — SSA representation and printing
- `codegen/` — Cranelift code generation (8 files)
  - `entry.rs` — compile_to_object, compile_ssa, lower_inst dispatch
  - `types.rs` — VarSlot, ExternFuncs, InstCtx
  - `arithmetic.rs` — emit_arith_op
  - `commands.rs` — lower_echo, lower_exec (fork/exec path)
  - `tests.rs` — [[ ]] test expressions
  - `values.rs` — lower_value, lower_value_as_int
  - `redirects.rs` — fd save/restore
- `taint.rs` — CST taint analysis
- `ssa_taint.rs` — SSA taint verification
- `validator.rs`, `rule.rs` — validation framework
- `rules/` — validation rules (16 rules)
- `diagnostic.rs` — error reporting
- `emit_janet.rs` — Janet backend
- `emit_source.rs`, `printer.rs` — source emission modes

### Key docs

- `notes/status.md` — current priority, just completed, next up
- `notes/decisions/` — one file per DD#, bare frontmatter with status
- `notes/journal/` — full journal in 54 files
- `notes/tasks/` — backlog with bare frontmatter (status, depends, gates)
- `docs/GUIDE.md` — architecture overview (public-facing)
- `docs/SUBSET.md` — safe subset specification (public-facing)

## Build and test

Build system uses Plan 9 `mk` (not `make`). Top-level `mkfile` has
`build`, `test`, and `clean` targets.

### Spec tests

Spec tests live in `spec/*.sh`. Run with `cd spec && bash run.sh`.
Results accumulate in `spec.db` (SQLite, repo root). Query with:

```sql
-- Current status
SELECT status, COUNT(*) FROM test_status GROUP BY status;

-- Failing gates
SELECT id, status FROM test_status WHERE status='failing' ORDER BY id;

-- Flips between commits
SELECT t.id, r1.status AS was, r2.status AS now
FROM tests t
JOIN runs r1 ON r1.test_id=t.id AND r1.commit_hash='OLD'
JOIN runs r2 ON r2.test_id=t.id AND r2.commit_hash='NEW'
WHERE r1.status != r2.status;
```

### Rust tests

Tests live in `sash/tests/`:
- `codegen_tests.rs` (266) — codegen correctness
- `e2e_tests.rs` (138) — end-to-end compilation and execution
- `oracle_tests.rs` (59) — compare output against Bash
- `janet_tests.rs` (38) — Janet backend
- `brush_compat_tests.rs` (52) — Brush compatibility
- `taint_spec_tests.rs` (42) — taint/bless scenarios

## Code index

<!-- BEGIN indexnudge AGENTS.md block -->
## Code index database

This project has an **indexnudge** SQLite database at `.indexnudge-rust.db`
in the repo root containing every line of source, every symbol definition,
every call site, every import, and full-text search over the code.

**Use `sqlite3` against the index for ALL code navigation.** Do NOT use
`grep`, `sed`, `cat`, `head`, `tail`, `find`, `view`, or `glob` to read
source files — the database is faster, gives structural context, and
keeps line numbers stable for editing.

Find the database and discover the schema on your first read:

```bash
DB=.indexnudge-rust.db
sqlite3 "$DB" ".schema"
```

### Core query patterns


```bash
# Project overview
sqlite3 "$DB" "SELECT file, lines, language FROM files ORDER BY file;"

# Symbols in a file (functions, methods, types)
sqlite3 "$DB" "SELECT name, kind, start_line, end_line, signature
               FROM symbols WHERE file='src/foo.rs' ORDER BY start_line;"

# Find a symbol by name across the whole codebase
sqlite3 "$DB" "SELECT name, kind, file, start_line, end_line
               FROM symbols WHERE name LIKE '%Parser%';"

# Read specific lines (use start_line/end_line from symbols query)
sqlite3 "$DB" "SELECT line_num, content FROM lines
               WHERE file='src/foo.rs' AND line_num BETWEEN 120 AND 180;"

# Full-text search (FTS5 — supports phrase, prefix, AND/OR/NOT)
sqlite3 "$DB" "SELECT file, line_num, content FROM lines_fts
               WHERE content MATCH '\"render_template\"';"

# Call graph: who calls X? what does Y call?
sqlite3 "$DB" "SELECT caller_file, caller_symbol, line_num
               FROM calls WHERE callee='render_template';"
sqlite3 "$DB" "SELECT DISTINCT callee FROM calls
               WHERE caller_symbol='handle_request' ORDER BY callee;"

# Imports — what does this file pull in?
sqlite3 "$DB" "SELECT module, line_num FROM imports WHERE file='src/foo.rs';"
```

### Complexity hotspots

`symbols.cyclomatic` is the McCabe cyclomatic complexity of each
function/method (NULL for non-callables). Use it to find refactor
targets or check whether a change spiked complexity:

```bash
# Top complexity in the whole codebase
sqlite3 "$DB" "SELECT name, qualified_name, file, cyclomatic,
                      end_line - start_line + 1 AS lines
               FROM symbols
               WHERE kind IN ('function','method') AND cyclomatic IS NOT NULL
               ORDER BY cyclomatic DESC LIMIT 20;"

# Hotspots in one module
sqlite3 "$DB" "SELECT name, cyclomatic FROM symbols
               WHERE file LIKE 'src/parse/%' AND kind IN ('function','method')
               ORDER BY cyclomatic DESC LIMIT 10;"

# Distribution — how many functions sit in each bucket?
sqlite3 "$DB" "SELECT CASE WHEN cyclomatic <= 5  THEN '1-5'
                           WHEN cyclomatic <= 10 THEN '6-10'
                           WHEN cyclomatic <= 20 THEN '11-20'
                           ELSE '21+' END AS bucket,
                      COUNT(*) AS fns
               FROM symbols
               WHERE kind IN ('function','method') AND cyclomatic IS NOT NULL
               GROUP BY bucket;"
```

Counting rules (McCabe, tuned to match Mozilla `rust-code-analysis`
for cross-tool comparability):

  - **+1 base** per function/method
  - **+1 per** `if` (including `if let`), `while` (including
    `while let`), `for`, `loop`, `match` arm, `let-else`, `&&`/`||`
    operator, match arm guard (`pat if cond => ...`), Rust `?` operator
  - **+1 per** closure/`FuncLit` in the enclosing function's body —
    the closure body itself is a separate symbol with its own CC row
  - **NOT counted**: `|`-alternated patterns (`A | B | C => ...`)
    count as one decision, not three — McCabe original convention

Nested closures and inner functions are separate `symbols` rows; their
body branching does NOT contribute to the enclosing function's CC
(the `+1` for the closure expression itself accounts for the branch
the enclosing fn took to construct it). Nested closures may not appear
in `symbols` today; the +1 for them is still counted.

### Disambiguating call targets

`calls.callee` stores the short name as written at the call site
(`find`, `new`, `parse`). When multiple symbols share that short name,
use the `callee_candidates` view to enumerate possibilities and narrow
them with what you know about the context:

```bash
# All candidate targets for one call site
sqlite3 "$DB" "SELECT candidate_qname, def_file, def_parent
               FROM callee_candidates
               WHERE caller_symbol='handle_request' AND callee='render_template';"

# Filter to same-crate / same-directory candidates
sqlite3 "$DB" "SELECT candidate_qname, def_file FROM callee_candidates
               WHERE caller_symbol='handle_request' AND callee='render_template'
                 AND def_file LIKE 'src/%';"

# Use a parent type the calling fn's body already mentions
sqlite3 "$DB" "SELECT cc.candidate_qname, cc.def_parent
               FROM callee_candidates cc
               WHERE cc.caller_symbol='handle_request' AND cc.callee='render_template'
                 AND EXISTS (
                   SELECT 1 FROM lines l
                   WHERE l.file=cc.caller_file
                     AND l.symbol=cc.caller_symbol
                     AND l.content LIKE '%' || cc.def_parent || '%');"
```

The view returns zero rows for foreign/stdlib calls (no local symbol
to point at) and exactly one row when the short name is unique in the
codebase. Multiple rows mean ambiguity — narrow with WHERE clauses.

For Rust today, `calls.callee_qname` is always NULL — the indexer does
not resolve qualified names. The view falls back to "every local
symbol with a matching short name" and you narrow from there.


### Finding string-keyed dispatch

`string_literals` is a view that peels every double-quoted string off
each source line and classifies its surrounding context. Use it to find
sites that compare-against / match-against / format-with a specific
string — far more precise than `lines_fts MATCH '"value"'` which would
also hit comments, format args, and arbitrary mentions.

| context     | matches lines like                            |
|-------------|-----------------------------------------------|
| `compare`   | `if name == "x"` / `if name != "x"`           |
| `match_pat` | `"x" => ...` / `"x" | "y" => ...`            |
| `fmt`       | `format!`, `println!`, `panic!`, etc.         |
| `arg`       | everything else (function arguments, struct literals, etc.) |

```bash
# Every site that dispatches on one of N builtin names
sqlite3 "$DB" "SELECT file, line_num, symbol, value, context
               FROM string_literals
               WHERE value IN ('echo','true','false','printf','exec')
                 AND context IN ('compare','match_pat')
               ORDER BY file, line_num;"

# Where is literal 'X' used as a format-string vs a real arg?
sqlite3 "$DB" "SELECT context, COUNT(*) FROM string_literals
               WHERE value LIKE '%error%' GROUP BY context;"

# Find string-keyed dispatch outside the canonical from_name() functions
sqlite3 "$DB" "SELECT file, line_num, symbol, value FROM string_literals
               WHERE value IN ('echo','true','false')
                 AND context = 'match_pat'
                 AND symbol NOT LIKE '%::from_name';"
```

Limits: lines that begin with `//` are filtered to suppress comment
false positives; multi-line raw strings (`r#"..."#` in Rust, backtick
strings in Go) only appear up to the first newline; escaped `"`
inside a string can split it early. For the dispatch-discovery use
case these gaps are rarely material.

### Finding repeated code (extract-helper candidates)

`block_dupes` is a view that surfaces every 4-line block of code that
appears 2+ times across the codebase. Use it before extracting a helper
to make sure you find every callsite, not just the obvious ones.

```bash
# Top duplication hotspots
sqlite3 "$DB" "SELECT occurrences, substr(replace(shingle, char(10), ' / '), 1, 80) AS preview
               FROM block_dupes ORDER BY occurrences DESC LIMIT 20;"

# Sites of a specific duplicate (find the locations to refactor)
sqlite3 "$DB" "SELECT occurrences, sites FROM block_dupes
               WHERE shingle LIKE '%create_sized_stack_slot%' LIMIT 5;"

# Filter out comma-separated parameter-list noise — focus on real code
sqlite3 "$DB" "SELECT occurrences, substr(replace(shingle, char(10), ' / '), 1, 100) AS preview
               FROM block_dupes
               WHERE shingle NOT LIKE '%,%,%,%,%'
                 AND length(shingle) > 60
               ORDER BY occurrences DESC LIMIT 10;"
```

Limits:
  - **k=4 fixed.** For a different window size, write your own CTE
    inline against `lines` with `ROW_NUMBER()` over `(file ORDER BY
    line_num)`.
  - **No identifier renaming.** `let ptr = ...` and `let dest = ...`
    hash differently. In practice the dominant duplicates are
    literally identical (function signatures, test scaffolding, setup
    boilerplate), so this matters less than expected.
  - **Overlapping shingles** within one larger duplicated block all
    appear separately — merge with your own filter when needed.

### Schema reference

| Table         | Columns of interest                                                |
|---------------|--------------------------------------------------------------------|
| `lines`       | `file, line_num, content, symbol`                                  |
| `lines_fts`   | FTS5 over `(file, line_num, content, symbol)`                      |
| `symbols`     | `name, qualified_name, kind, file, start_line, end_line, parent, signature, cyclomatic` |
| `calls`       | `caller_file, caller_symbol, callee, callee_qname, kind, line_num`  |
| `callee_candidates` | view: `calls` ⨝ `symbols` — one row per (call, candidate symbol) |
| `string_literals` | view: every double-quoted string on every line, with context bucket |
| `block_dupes` | view: 4-line duplicated runs across the codebase (extract-helper candidates) |
| `defs`        | `symbol, file, line_num, var, kind`  *(variable/binding definitions)* |
| `imports`     | `file, module, line_num`                                           |
| `files`       | `file, lines, language, indexed_mtime`                             |
| `directories` | `path, file_count, total_lines`                                    |


### Freshness

The database is re-freshened on-demand by the `sqlite3` shim in
`indexnudge/bin-rust/`. If that shim is first on `PATH`, every shell
invocation of `sqlite3` re-parses changed files before querying.

**Non-shim callers bypass this.** Anything that links `libsqlite3`
directly — Python's `sqlite3` module, Rust `rusqlite`, Node
`better-sqlite3`, `sqlite-utils`, `litecli`, GUI browsers — opens the
DB file directly and sees whatever snapshot the last shim invocation
left behind.

| Caller                                | Triggers shim? |
|---------------------------------------|----------------|
| `bash$ sqlite3 .indexnudge-rust.db`   | yes            |
| `python3 -c "import sqlite3; ..."`    | **no**         |
| Rust `rusqlite::Connection::open()`   | **no**         |
| Node `better-sqlite3`                 | **no**         |
| `sqlite-utils`, `litecli`, GUI tools  | **no**         |

When you drive the index from a script, run `indexnudge --freshen`
explicitly first:

```bash
indexnudge --freshen     # incremental: only re-parses changed files in cwd
python3 my_helper.py        # now sees current state
```

Or wrap the shell call inside the script (`subprocess.run(["sqlite3", ...])`)
so the shim handles it. Within a single script run, one freshen at the
top is enough — line numbers stay stable until the script itself mutates
files.

### Editing

When you need to mutate a file, prefer tools that take **line-number
addresses** (e.g. `ed`, `sed -i` for targeted substitutions) — line
numbers from `symbols.start_line` and `symbols.end_line` pair naturally
with these. Avoid exact-string-match editors when the surrounding code
might appear more than once.

### What NOT to do

- ❌ `grep -r 'pattern' .` — use `lines_fts MATCH 'pattern'`
- ❌ `cat src/foo.rs` / `view src/foo.rs` — use `SELECT content FROM lines WHERE file=... AND line_num BETWEEN ...`
- ❌ `sed -n '80,160p' src/foo.rs` — same as above
- ❌ `find . -name '*.rs'` — use `SELECT file FROM files WHERE language='rust'`
- ❌ Routing around the shims with `/usr/bin/grep`, `command cat`, or `awk 'NR>=X && NR<=Y'`
<!-- END indexnudge AGENTS.md block -->

### Complexity index

`bash contrib/cyclo.sh sash/src --top N` ranks functions by CC.
The driver honors `.cyclo-allow` at the repo root — one
`path::function # rationale` per line filters trivially declarative
high-CC functions out of the report. `--show-allowed` displays what
was filtered. Use `--no-allow` to bypass the file entirely.

## Completion checklist

Every task ends with:
1. All tests pass (`mk test`)
2. Run specs: `cd spec && bash run.sh` — check spec.db
3. Update `notes/journal/` (new file or append to latest)
4. Update `docs/GUIDE.md` if new architecture introduced
5. Add deferred work to `notes/tasks/` (bare frontmatter)
6. Update `notes/status.md` with progress
7. Update this file if SOP changed
8. `git commit` with descriptive message
9. `git push`

## Bug policy

Zero tolerance. Finding a bug is a stop-everything event. Never work
around it. If you notice yourself routing around a behavior, STOP.
Investigate whether it's a bug.

## Taint system

Taint is definition-based, not name-based. There is no "clean list"
of variable names. A variable is clean or tainted based on where its
value came from:

- **Clean**: literals, arithmetic from clean operands, compiler-managed
  values ($#, $?, $$, $!, $LINENO), function args from clean callers,
  user function capture
- **Tainted**: $(getenv NAME), $(argv N), read from fd/pipe,
  external command capture

Env vars ($HOME, $PATH, etc.) are NOT special — they're just
`$(getenv NAME)`, which is tainted. The validator (SB5018) rejects
bare `$HOME` and directs to `$(getenv HOME)`.

## Learnings from 2026-05-11 session

- **Decision authority**: all important decisions go through DG.
  Don't narrow options or choose approaches without presenting them.
- **spec.db is at repo root**, not `spec/spec.db`. Run `cd spec && bash run.sh`
  to update it. Query via `test_status` view.
- **`pub(crate)` on internal modules** catches dead code. main.rs is
  a thin wrapper; all logic in lib.rs.
- **Dead code was hiding**: `is_special_var` (taint clean list) was
  dead code — never called. The `pub(crate)` change exposed 10+ dead
  items. Keep modules `pub(crate)` unless tests need them.
- **read -r rejected**: sash doesn't have `-r` flag. Converter strips
  it. Spec tests must not use it.
- **Pipeline builtins**: echo/true/false/: run inline in forked
  children. Other builtins (printf) resolved to absolute paths.
  Middle stages always fork.
- **Arithmetic sexpr**: tree-sitter can't parse `(name args...)`
  in arithmetic. Source preprocessor replaces with placeholders.
- **UTF-8**: byte-level source processing must use `utf8_char_len`,
  not `bytes[i] as char`.

## Attribution

Be explicit about who did what. DG directs the project and makes
design decisions. The AI implements. Don't credit the AI for DG's
design choices. When unsure, say so.

## Adversarial review

The project has completed one adversarial review (run-1, in
`audit/run-1/`). Schedule another after significant new code.
Validator bypass testing is the highest priority.
