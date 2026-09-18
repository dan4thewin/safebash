# AGENTS.md — Field Notes for AI Contributors

## What this project is

safebash is a compiler for a safe subset of Bash that produces
static native binaries. DG created the project and directs the
effort. The AI implements: writing Rust, designing IRs, writing
docs. Neither works alone — DG makes design decisions; the AI has
the stamina for incremental implementation.

The compiler works end-to-end: bash source → parse → validate →
lower → type check → transform → fold → codegen → link → static ELF.
Output is byte-identical to Bash. Also supports `--emit-c`, `--emit-ir`,
`--manifest`, `--extract`, `--trace`, `--no-taint`, `--heap-size` and
`--sysroot`; `sash --help` lists the full set. (`--janet`/`--janet-map` are
**gone** — the Janet backend was removed by DD#104 in favour of the
`--emit-ir` seam.) (`--emit-source`/`--annotate`/`--nolint` are **sash-convert**
flags — `sash` rejects them, DD#57.)

Read the journal prologue (`notes/journal/001-prologue.md`) before
your first session. It sets the tone for everything.

## Decision authority

DG tasks the AI with implementing and expects all important
decisions to be discussed before acting — design, architecture,
implementation strategy, or anything else that matters. The AI
does not get to decide what's "just implementation" and skip the
conversation. When in doubt, present options with tradeoffs.

See `notes/parser-migration-postmortem.md` for the canonical
examples of what this rule prevents — six addenda where DG had
to correct the AI for narrowing options or justifying deferrals
before DG saw the choice.

### When to ask vs. proceed (decision tree)

DG flagged both failure modes: **under-asking** (deciding a
genuine fork unilaterally, narrowing options — the postmortem
above) and **over-asking** (re-confirming what a source of truth
already settles; too much hand-holding). The operative test:
**ask only when a decision is BOTH underdetermined AND
consequential/irreversible.** Otherwise decide and report. Walk
it in order:

0. **No genuine decision** (routine implementation, one clear
   path) → proceed.
1. **Settled by a source of truth** — user input, a stated
   principle, a DD, a documented convention, or an established
   codebase pattern → apply it. Apply a stated principle to
   **all** its consequences; do not narrow or reinterpret it
   into a smaller decision so you can defer the rest. ("A string
   cell behaves like a string variable" settles every operator
   and both contexts — not just the case that was asked about.)
2. **It's a bug** → fixing is mandatory (see Bug policy); never
   ask *whether* to fix. You may still ask about a fix *approach*
   that is itself a consequential fork (step 4).
3. **A fork exists but is reversible / low-consequence** (an
   internal representation, a staging order, a test layout) →
   decide, proceed, and **report** the choice + rationale so DG
   can veto.
4. **A fork exists that is consequential OR hard to reverse**
   (design, architecture, behavior or language surface, an
   irreversible cleanup) → **ask**: one batched message with
   options + tradeoffs + a recommendation. Do not drip sequential
   questions.

**Always ask (overrides the above):** before changing something
that might be deliberate (check `notes/decisions/` first), or
before any destructive/irreversible operation (history rewrite,
mass delete, force-push).

## First thing every session

Read `notes/status.md`. It tells you the current priority and
what was just completed.

Read `notes/decisions/` before changing any behavior that might
be deliberate. These are the authoritative source for what's
intentional vs. what's a gap.

If the task involves a non-trivial decision, apply the decision
tree under **Decision authority**: a fork that is consequential
*and* underdetermined (design choice, architecture pivot,
behavior change, irreversible cleanup) → list options + tradeoffs
and ask DG before acting; a reversible / low-consequence one →
decide and report. Defaulting to "I'll just implement option A"
on a consequential fork violates Decision authority.

## Current state

- **sash/** — the compiler (Rust, Cranelift backend)
- **sash-rt/** — pure Rust `#![no_std]` runtime crate (rustix, no libc)
- **sash-convert/** — bash → sash-native converter
- **sash-expand/** — expansion utilities
- **sash-getopt/** — option-parsing helper (`mk sash-getopt-test`)
- **docs/** — GUIDE, REVIEW, TIMELINE, sash.1
- **notes/** — iwe knowledge graph: journal, decisions, tasks, status, architecture
- **audit/** — adversarial audit findings and remediation
- **contrib/** — contributed scripts/tools
- **suites/** — test suites
- **exp/** — git submodule → safebash-exp (historical experiments, not needed for build)
- **spec.db** — SQLite database of all spec test results (repo root,
  **untracked and gitignored** — a local development artifact, not in git)
- **97 design decisions** in `notes/decisions/` (one file per DD, numbered up to DD#109)
- **1 adversarial audit** completed (5 models, 3 passes, remediation done)
- **tree-sitter**: not used by the main compiler (hand-written parser). The
  `sash-convert` crate uses **stock** `tree-sitter-bash 0.25` on bash input.
  (The old `~/src/tree-sitter-bash-safebash` fork drove the deleted
  main-compiler pipeline and is no longer part of the build.)

### Crate structure

`main.rs` is a thin 4-line wrapper calling `sash::cli_main()`.
All compiler logic lives in `lib.rs`. Internal modules use
`pub(crate)` — Rust warns on dead code. Modules used by
integration tests are `pub`.

### Pipeline

hand-written parser (logos lexer + recursive descent) → Program (AST,
defined in `ir.rs`) → validate (rules + taint + positionals + argv-splat)
→ type check → transform (AST→SSA) → fold → codegen (Cranelift) → link →
static ELF

The compiler has **no tree-sitter dependency and no CST** — the parser
(`parse/`) yields the AST directly (the codebase calls this AST its
"IR"). tree-sitter survives only in the separate `sash-convert` crate
(bash→sash-native conversion). (Removed in commit 1c0e64a, which deleted
`lower/`, `rules/`, the standalone taint pass, and the logos-CST glue.)

DD#58 arithmetic sexpr: `(name args...)` inside `$(( ))` is parsed
natively by the arith parser (`parse/arith.rs` → `ArithExpr::FuncCall`);
the transform resolves the call. (The old pre-tree-sitter source
preprocessor and global `ARITH_MAP` are gone.)

### Key source files (`sash/src/`)

- `lib.rs` — entry point, cli_main
- `parse/` — hand-written lexer + recursive-descent parser → Program/AST
  - `mod.rs` — parse module root
  - `logos_tokens.rs` — logos `Tok` enum (lexer tokens)
  - `lexer.rs` — Scanner (token stream: peek/peek_n/next)
  - `parser.rs` — recursive descent → Program; `parse()`, statements/decls
  - `expr.rs` — words, strings, parameter/command expansion, subscripts
  - `arith.rs` — `(( ))`/`$(( ))` lexer+parser, DD#58 sexpr FuncCall
  - `error.rs` — parse_error helpers
  - `tests.rs` — parser unit tests
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
  - `stmt_handlers.rs` / `cmd_handlers.rs` / `expr_handlers.rs` — the
    per-kind arms split out of the dispatchers above
  - `capture_purity.rs` — picks the buffer vs memfd capture path per site
- `resolve.rs` — DD#23 command resolution (absolute paths, manifest)
- `ssa.rs` — SSA representation (and printing)
- `codegen/` — Cranelift code generation (12 files)
  - `entry.rs` — compile_to_object, compile_ssa, lower_inst dispatch,
    `embed_sash_source` (the `.sash_source` ELF section)
  - `types.rs` — VarSlot, ExternFuncs, InstCtx
  - `arithmetic.rs` — emit_arith_op
  - `commands.rs` — lower_echo, lower_exec (fork/exec path)
  - `dispatch.rs` — the `[[ … { … } ]]` dispatch form (DD#66)
  - `setup.rs` — prologue, runtime init, extern declarations
  - `tests.rs` — [[ ]] test expressions
  - `values.rs` / `value_handlers.rs` — lower_value, lower_value_as_int
  - `inst_handlers.rs` — per-instruction arms
  - `redirects.rs` — fd save/restore
- `validate.rs` — IR validation: structural rules + taint (`check_taint`) +
  positionals + argv-splat (absorbed the old `rules/`, `taint.rs`,
  `validator.rs`, `rule.rs`)
- `ssa_taint.rs` — SSA taint verification
- `regex_validate.rs` — DD#39/#63 regex validation
- `diagnostic.rs` — error reporting
- `emit_c.rs` — C backend (`--emit-c`), the differential oracle: the spec
  suite runs twice, `SASH_MODE=native` and `SASH_MODE=c`, and a divergence
  localises a bug to one backend
- `emit_ir.rs` — `--emit-ir`, the IR as inert Janet data (DD#104)

**Gone, despite what older notes say:** `emit_janet.rs` (the in-tree Janet
backend, removed by DD#104 in favour of the `--emit-ir` seam),
`emit_source.rs` (the `.sash_source` text now comes from
`codegen/entry.rs::embed_sash_source`), and `oracle.rs`. There is no
`printer.rs`, `ssa_printer.rs`, `lower.rs`, `validator.rs`, `rule.rs`,
`rules/`, `taint.rs`, or `known_externals.rs` under `sash/src` either —
`known_externals.rs` belongs to `sash-convert`.

### Key docs

- `notes/status.md` — current priority, just completed, next up
- `notes/decisions/` — one file per DD#, bare frontmatter with status
- `notes/journal/` — full journal (310 files and counting)
- `notes/tasks/` — backlog with bare frontmatter (status, depends, gates)
- `docs/GUIDE.md` — architecture overview (public-facing)
- the safe subset is defined by the DDs in `notes/decisions/` (SUBSET.md was
  removed 2026-08-29 — it had drifted from the DDs and was misleading)

## Build and test

Build system uses Plan 9 `mk` (not `make`). Top-level `mkfile` has
`build`, `test`, and `clean` targets.

**`sash-rt` is a SEPARATE cargo workspace.** Build the runtime with
`(cd sash-rt && cargo build --release)` — `cargo build -p sash-rt` from the
repo root fails ("package ID `sash-rt` did not match any packages"). Its
output is `sash-rt/target/release/libsash_rt.a`; the c/native backends link
it via `SASH_RT_LIB`. Rebuild it after touching `sash-rt/src` before an
`--emit-c` link, or gcc reports undefined references to new runtime symbols.

### Spec tests

Spec tests live in `spec/*.sh`. Run with `cd spec && bash run.sh`.
Results accumulate in `spec.db` (SQLite, repo root).

**`spec.db` is untracked and gitignored.** It is a local artifact: every run
rewrites it wholesale, so versioning it added a fresh 20 MB blob per commit
(9.0 GB of pack across 455 commits) for data that is only meaningful on the
machine that produced it. It was purged from history on 2026-09-10. Do not
re-add it, and do not reach for `git show <rev>:spec.db` — that no longer
exists. **The database is its own history**: `runs` records a `commit_hash`
per test per run, so cross-commit comparison is a plain SQL query (below).

**The database is reproducible from two tracked files.** `run.sh` applies
`spec/schema.sql` (tables, views, indexes) and loads `spec/clauses.tsv`
before recording anything, both idempotently, so a fresh clone with no
`spec.db` at all builds one correctly. `clauses` is the one part that is
**not** derived from a test run — 231 rows of curated DD clause text — which
is why it lives in a tracked TSV rather than being regenerated. If you add or
edit a clause, re-export it:

```sh
sqlite3 -noheader -separator "$(printf '\t')" spec.db \
  "SELECT id, dd, text FROM clauses
   ORDER BY dd, CAST(substr(id, instr(id,'.')+1) AS INTEGER), id;" \
  > spec/clauses.tsv
```

Clause text must not contain tabs or newlines (the TSV has no quoting).

**`chk` ids are a flat GLOBAL namespace across every spec file.** `chk N`
records `$SASH_MODE:N`, so the same `N` in two files (e.g. `53.1` in both
`convert.sh` and `taint.sh`) is one test_id — whichever file runs later
(alphabetical) overwrites the other, silently if both pass. Before adding
tests, `grep -rn "chk <prefix>\." spec/*.sh` to pick a collision-free block
(each file tends to own a DD-numbered range, e.g. convert.sh = `57.x`).

**Reading a range of the file is not that check.** An `sx.12` added after the
`sx.1`-`sx.11` block collided with an `sx.12` further down the same file, and
both passed, so nothing reported it. Run the mechanical check after adding:

```sh
grep -ho '\bchk [A-Za-z0-9._]*' spec/*.sh | grep -v skip_chk | sort | uniq -d
```

Empty means clean. (`skip_chk` is excluded because the `chk`/`skip_chk` pairs
in `compiler.sh` and `misc.sh` are mode guards -- exactly one branch fires.)

**That check is blind to ids that reach `chk` through a variable.** The loop
at the end of `coverage.sh` assigns `cov.306`-`cov.308` in a `case` and calls
`chk $_id`; grepping for `chk <literal>` never sees them, so they look free.
Numbering a new block at `cov.306` collided with all three, and because the
loop runs last it silently overwrote the new specs -- both sets passed, so
nothing reported it. Run the second check too:

```sh
comm -12 <(grep -hoE '_id=[A-Za-z0-9._]+' spec/*.sh | sed 's/_id=//' | sort -u) \
         <(grep -ho '\bchk [A-Za-z0-9._]\+' spec/*.sh | sed 's/chk //' | sort -u)
```

Empty means clean. There is exactly one variable-id site in the suite today.

**Predict the sweep total before running it.** N new specs in a native-only
file is +N; in a dual-mode file it is +2N. A green sweep whose total is not
what you predicted means specs collided, vanished, or were skipped -- the
collision above was found only that way.

**Predict against `run.sh`'s output, NOT `SELECT COUNT(*) FROM test_status`.**
`test_status` is `tests LEFT JOIN runs`, and `tests` is a permanent registry:
a spec you delete keeps its row and its last status forever, so the view
reports retired tests as passing indefinitely. This matched exactly for weeks
only because every sweep until then had purely *added* specs. The authoritative
current total is `run.sh`'s own `passing`/`skipped` lines. The source-derived
ground truth, which agrees with it, is the weighted `chk` count:

```sh
cd spec && tot=0; for f in *.sh; do case "$f" in common.sh|run.sh) continue;; esac
  n=$(grep -o '\bchk [A-Za-z0-9._]*' "$f" | wc -l); [ "$n" = 0 ] && continue
  grep -qE 'SASH_MODE.*!=.*native.*exit' "$f" && m=1 || m=2
  tot=$((tot + n*m)); done; echo "$tot"
```

(`convert.sh` and `trace.sh` are the only native-only files -- 191 tests, the
exact native-minus-c difference.) Note this undercounts a file by one per
variable-id `chk`, which is the discrepancy that exposed the collision above.

**A `ran=` you cannot account for is a finding, not a rounding error.**
`coverage.sh` reported `ran=323` against 321 greppable `chk` tokens. Chasing
that two-count gap is what surfaced both the id collision and the
`test_status` fault above.

**`plan=` inside a spec file is per-mode, not per-sweep.** It is +N even in a
dual-mode file -- the file runs once per mode and `plan` is compared against
that one run's index. Setting it to +2N gives a *passing* file that prints
`[plan=217, ran=211]`; `common.sh` notes the mismatch rather than failing on
it, so only the predicted total catches it.

Query with:

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

-- Regression check: current state vs a baseline commit.
-- This replaces the old `git show HEAD:spec.db` + ATTACH recipe.
-- Find candidate baselines first:
--   SELECT DISTINCT commit_hash, MAX(committed_at) FROM runs
--   GROUP BY commit_hash ORDER BY 2 DESC LIMIT 5;
WITH base AS (
  SELECT test_id, status FROM runs r
  WHERE commit_hash = 'BASE'
    AND rowid = (SELECT MAX(rowid) FROM runs
                 WHERE test_id = r.test_id AND commit_hash = 'BASE')
)
SELECT b.test_id, b.status AS was, t.status AS now
FROM base b JOIN test_status t ON t.id = b.test_id
WHERE b.status != t.status
ORDER BY b.test_id;
```

Use plain `sqlite3` for **every** database, including `spec.db`. The
indexnudge shim only re-parses the source index when the database it is
handed is the index itself; for anything else it is a pass-through to the
real `sqlite3`. Do **not** reach for `/home/linuxbrew/.linuxbrew/bin/sqlite3`
to "bypass" it — that advice was wrong, and copying it onto
`.indexnudge-rust.db` silently serves stale line numbers.

`runs` is append-only and past 1.4M rows, so both queries above depend on
the indexes in `schema.sql`. Without them the regression query takes about
four minutes; with them, milliseconds. They cost ~95 MB of local disk.

### Runtime coverage (sash-rt)

`sash-rt` has its own coverage driver — `contrib/rt-coverage.sh`, wrapped as
`mk rt-cov` / `mk rt-cov-branch` / `mk rt-cov-reset`. `mk cov` deliberately
ignores `sash-rt`.

It works by linking seven objects out of rustc's `libprofiler_builtins` and
draining `__llvm_profile_write_buffer()` over rustix at exit
(`sash-rt/src/coverage.rs`, feature `coverage`). Three things matter:

- the profiler objects must come from the **rustc sysroot**, not brew clang,
  or the `.profraw` header version and the counter layout disagree;
- the report runs against a `--whole-archive` reference binary, because `sash`
  links with `--gc-sections` and each compiled program's covmap is a *subset*;
- the compiler must be **uninstrumented** (`mk cov-reset` first), or its own
  profraws land in the same pool.

Compiled programs write a `.profraw` only when `SASH_RT_PROFILE_DIR` is set —
deliberately not `LLVM_PROFILE_FILE`, which cargo-llvm-cov uses for the
compiler. Run `mk rt-cov-reset` afterwards: until you do, every binary `sash`
produces links an instrumented runtime.

Baseline (journal 320): **81.51% lines, 67.83% branches, 89.58% functions**.

### Rust tests

**`sash/tests/` no longer exists.** All 546 integration tests were deleted
in the 2026-09-14 test redux (journal 304) after each was classified with
per-test evidence: 210 asserted surfaces we have deliberately retired, 272
were already covered by concrete spec ids, 45 were bash-differentials with
no independent assertion, and only 4 were genuine gaps — those became 20
new spec tests. `sash-rt-tests` went the same way, for the same reason.

The decisive finding: **the harness was fail-open.** `run_bash_native`
called `sash::codegen::compile_to_object` directly, so `validate` never
ran, and 49 of its 159 passing tests (31%) compiled programs the real
compiler rejects. `echo -n` is rejected by SB5059 yet still honoured by
codegen, which is how `echo_n_multiple_args` "passed" a bash-differential
for so long. **If you build a new harness, drive the real CLI binary — never
`compile_to_object`.** A test that skips a pass tests a compiler we do not
ship.

What remains is 77 in-crate unit tests, run by `mk test`:
- `sash` (30) — parser and codegen units, in `#[cfg(test)]` modules
- `sash-convert` (27) · `sash-expand` (20)

**The spec suite in `spec/` is the authoritative gate**, and now the only
one for end-to-end behaviour. Add coverage there, not in a new Rust suite.

### Coverage

`mk cov` measures what the **spec suite** reaches in the compiler. `mk
cov-branch` adds branch coverage (nightly-only — `-Zcoverage-options=branch`
is unstable; the shipped build stays stable). `mk cov-unit` folds in the
in-crate unit tests, which is off by default because it hides exactly the
should-never-run assertions worth finding. **`mk cov-reset` afterwards** —
see below.

All the logic lives in `contrib/coverage.sh`. Do not hand-roll a coverage
run; three separate things silently measure *nothing* and still print a
clean report:

1. **`cargo llvm-cov clean --workspace` does not invalidate
   `target/release`.** The old fingerprint is reused, the sweep runs against
   an uninstrumented binary and drops **0 profraws**. `rm -rf target/release`
   is the only reliable force.
2. **`spec/run.sh` reads `SASH_RELEASE_DIR`, not `SASH`.** The old
   `mk cov-spec` exported `SASH=`, so it measured whatever was already in
   `target/release`.
3. **`$PWD` inside a mkfile recipe is a *mk* variable**, not a shell one, so
   the old `LLVM_PROFILE_FILE` path expanded to a bare relative name.

The script gates every run on a **canary**: one `sash` invocation must drop a
profraw or it aborts. Keep that property in anything you add here — a gate
that cannot fail is not a gate.

**`mk build` does NOT undo a coverage run.** cargo-llvm-cov passes
`-Cinstrument-coverage` through a `RUSTC_WRAPPER`, so cargo's fingerprint
never sees it and the instrumented artifacts look fresh. The leftover binary
is slower and drops `default_*.profraw` wherever it runs. Use `mk cov-reset`.

`coverage.db` (repo root, **untracked and gitignored**, same status as
`spec.db`) holds lcov line/function/branch data so uncovered lines can be
joined against the indexnudge `symbols` table and ranked **by function**.
Build it with `mk cov-db` after a `mk cov-branch`; it reads the lcov that
`contrib/coverage.sh --lcov` writes to `target/coverage/`. That directory
survives `mk cov-reset` (which removes only `target/release`) — **the lcov
cannot be regenerated afterwards**, because `cov-reset` drops the covmap
along with the instrumented binary. The summary table names thin *files*;
only the lcov names thin *lines*.

```sql
ATTACH 'coverage.db' AS cov;
SELECT s.file, s.name, COUNT(*) AS uncov
FROM cov.cov_lines cl
JOIN symbols s ON s.file=cl.file AND cl.line BETWEEN s.start_line AND s.end_line
WHERE cl.count=0 AND s.kind IN ('function','method')
GROUP BY s.file, s.name, s.start_line ORDER BY uncov DESC;
```

**`sash-rt` is not measured by any of this.** It is `#![no_std]` with no
libc, so instrumentation compiles but nothing writes a profraw at exit.

**`mk build` must build every workspace member.** It once omitted
`sash-expand`, which only ever worked because `mk test` builds it as a side
effect of `cargo test`. The moment something removed `target/release` — which
`mk cov` and `mk cov-reset` both do by design — spec `54.1` failed, and it
failed *obscurely*: the spec locates the binary with `which sash-expand`, so a
missing one becomes an empty path and the error is a compile failure about a
config file. If a spec shells out to a helper binary, `mk build` has to
produce it.

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

Counting rules (matches Mozilla `rust-code-analysis` per-function
`cyclomatic.sum` — verified against the
[Mozilla source](https://github.com/mozilla/rust-code-analysis/blob/main/src/metrics/cyclomatic.rs)):

  - **+1 base** per function/method
  - **+1 per** `if` (including `if let`), `while` (including
    `while let`), `for`, `loop`, `match` arm, `&&`, `||`, Rust `?`
    operator
  - **+1 per** closure / nested-fn inside the function's body, AND
    its inner decisions accumulate too (Mozilla's `.sum` merges
    child-space CCs into the parent)

**Explicitly NOT counted** (Mozilla doesn't either, and several were
tested empirically and rejected):

  - `matches!` macro invocations (tree-sitter sees a macro, doesn't
    expand it)
  - `let-else` declarations
  - match-arm guards (`pat if cond => ...`)
  - `|`-alternated patterns (`A | B | C => ...` is one decision)
  - The `match` / `switch` keyword itself (the arms are what counts)

If you've used `rust-code-analysis-cli -m -p path -O json` and read
`metrics.cyclomatic.sum` from each `kind: function` space, those
numbers match the values returned here.

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

### Complexity index — known residual

Use `symbols.cyclomatic` and `symbols.cognitive` from the indexnudge
DB as the canonical complexity sources (counting rules documented in
the indexnudge block above). Both metrics are calibrated against
Mozilla `rust-code-analysis-cli` (cyclomatic.sum, cognitive.sum).

**Two-metric convention.** McCabe cyclomatic alone is a poor signal:
a 48-arm pure dispatcher and a 4-level nested-conditional function
both score around 48, but only one is actually complex. Cognitive
Complexity (Campbell, SonarSource 2018) discounts wide-but-shallow
matches and adds nesting penalties, so it disambiguates the two.

The pre-commit hook reports both columns side-by-side, ranked by
`MAX(cyclomatic, cognitive)`. Read them together:

| pattern               | shape                                     | what to do |
|-----------------------|-------------------------------------------|-----------|
| high cc, low cog      | dispatcher (match on enum, lex table)     | accept as-is — no refactor needed |
| low cc, high cog      | hidden nesting (cc structurally blind)    | investigate — cc was masking it |
| high both             | genuine refactor target                   | refactor |
| double-failure at 2×  | cc > 2×mean AND cog > 2×mean — the gate cohort | priority refactor |

The `// cyclo-allow:` annotation convention is retired (commit
2026-06-05). Cognitive scoring eliminates the need for per-function
suppressions on dispatchers; the original annotation set also had a
known mis-application problem (12 of 51 annotations were hiding real
complexity per cognitive). Do **not** re-introduce the convention.

**Enforced gate (pre-commit).** Adopts the rubric directly:

```
REJECT if (cognitive - cyclomatic) > -15
       AND cognitive > 2
       AND (cyclomatic > 3 × mean_cc OR cognitive > 3 × mean_cog)
       AND file in sash/src/
       AND function is touched by this commit's staged hunks
       (i.e. grandfather-on-modify)
```

Reading: "if a function this commit touches is not a dispatcher AND
it's above 3× the codebase mean on either axis, reject." Dispatchers
are exempted by construction — no manual annotation needed.

**There are two dispatcher exemptions, because one test could not see
both shapes.** The delta test (`≤ −15`) catches a *wide* dispatcher —
`to_tok` at cc 46 / cog 1. It cannot catch a *flat* one: at cog 2 a
delta of −15 requires cc 17, so a twelve-arm walker sat in a **dead
zone between cc 12 and 16**, rejected by the gate while this very table
said "high cc, low cog — accept as-is". **DG closed it 2026-09-16:
accept as-is for cog ≤ 2.** Cognitive is the metric that measures
whether something is actually hard to read, and at cog ≤ 2 it is not,
whatever the arm count. The exemption freed 7 functions (all cc 12–15,
cog 1–2: `next_token`, `child_bodies`, `child_bodies_mut`,
`assign_op_for`, `simple_escape`, `emit_command_body`, `emit_arith_op`)
and left 48 gated.

The means recalculate per commit (a ratchet: as the codebase
refactors down, the bar tightens). **Do not hard-code them anywhere** —
measured 2026-09-13 over `sash/src` they are **cc 4.04 / cog 2.05** across
1735 functions, so the 3× bar sits at **12.1 / 6.2**, materially tighter
than the numbers older notes quote. Re-measure rather than quote:

```sh
sqlite3 .indexnudge-rust.db "SELECT AVG(cyclomatic), AVG(cognitive)
  FROM symbols WHERE kind IN ('function','method')
  AND cyclomatic IS NOT NULL AND file LIKE 'sash/src/%';"
```
 Pre-existing violators remain
silent until edited, at which point they must come down to spec.
**No file is exempt.** (The previous `emit_janet.rs` carve-out was
retired 2026-06-06: deferral was entrenching the violation rather
than driving it down.)

**The gate holds at 3× (DG, 2026-09-12).** The earlier plan was to
tighten to 2× then 1.5× "as the cohort clears". Measured, the cohort
is not clearing and the tightening does not survive contact: 80
functions violate at 3×, **357 at 2×, 518 at 1.5×** — a third of the
codebase in violation at the first step. Since the gate is
grandfather-on-modify, the 80 are fixed exactly when someone touches
them, which is the mechanism working as designed rather than a
backlog to burn down. Do not re-propose tightening without fresh
numbers showing the cohort has actually shrunk. Full rubric and
pattern library: `notes/journal/108-cog-cc-rubric.md`.

Bypass for one commit: `SKIP_COMPLEXITY_GATE=1 git commit ...`
(prefer fixing the function or amending the surrounding code if at
all possible; bypass should be rare and never routine).

**Cyclomatic residuals.** Numbers match Mozilla exactly for 11 of 14
spot-checked hotspots. Three functions in `sash/src/transform/`
under-report due to a tree-sitter-rust grammar version skew vs.
Mozilla's pinned grammar (MatchArm/MatchArm2 split):

| function                                  | indexnudge | Mozilla | delta |
|-------------------------------------------|-----------:|--------:|------:|
| `transform/statements::lower_one`         |         55 |      57 |    −2 |
| `transform/control_flow::lower_case`      |         40 |      44 |    −4 |
| `transform/control_flow::lower_for`       |         35 |      44 |    −9 |

When setting absolute thresholds, allow ~10 slop on big-match-on-
CST-kind functions in `sash/src/transform/`, or run
`rust-code-analysis-cli -m -p sash/src -O json` and read
`metrics.cyclomatic.sum` for those three specifically.

**Cognitive residuals.** Inner `function_item` nested inside outer
`function_item` reports `depth = 0` in indexnudge but `depth = 1` in
Mozilla — per-fn divergence of +1/+2 at most, and item-level nested
fns are rare in Rust. No known residuals beyond that and the same
tree-sitter grammar skew that affects cyclomatic.

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

**Step 1 is achievable again as of 2026-09-14.** `mk test` had been red for
months — first because `cargo test --test codegen_tests` hung forever (the
harness bypassed `validate`, so fixtures using retired surfaces compiled to
binaries that never terminated), and latterly because a doctest failed. Both
causes are gone: the hanging suites were deleted in the test redux, and the
doctest was a 4-space-indented code block in `validate::check_taint`'s doc
comment holding *sash* source, which rustdoc dutifully compiled as Rust.

Steps 1 and 2 are **both** gates now. Step 2 (the spec suite) is still the
one that carries the behavioural weight — 4611 tests against 77 units — so
when the two disagree about whether something is covered, believe the specs.
Keep running anything long under `timeout`.

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
  to update it. Query via `test_status` view. (It is untracked as of
  2026-09-10 — see the Spec tests section.)
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
