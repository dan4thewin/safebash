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

A code index lives at `.indexnudge-rust.db` (SQLite, refreshed by
the `indexnudge` shims on each invocation). Use it instead of
reading whole files:

```sql
SELECT file, line_start, line_end FROM symbols
WHERE name = 'function_name' AND file LIKE 'sash/src/%';

SELECT file, line_num, content FROM lines
WHERE content LIKE '%keyword%' AND file LIKE 'sash/src/%';
```

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
