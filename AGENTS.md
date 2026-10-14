# AGENTS.md — Field Notes for AI Contributors

## What this project is

safebash is a compiler for a safe subset of Bash that produces
static native binaries. DG created the project and directs the
effort. The AI implements: writing Rust, designing IRs, writing
docs. Neither works alone — DG makes design decisions; the AI has
the stamina for incremental implementation.

The compiler works end-to-end: bash source → parse → validate →
lower → type check → transform → fold → codegen → link → static ELF.
**Byte-identical-to-Bash is NOT a goal, and has not been one for a long
time** (bash has too many indefensible quirks). Where sash and bash differ,
the question is whether sash's behaviour is *defensible*, not whether it
matches — and a divergence is settled by a DD, not by deferring to bash.
Several are deliberate and documented (DD#73 glob absolutization, DD#55
`wait`'s implicit-check status, `cap.12`'s `-p /dev/stdout`). Do not cite
"bash does X" as evidence that sash is wrong; cite a DD, or raise the
question. Bash remains a *reference* for the subset sash does implement, so
an unexplained divergence is still worth investigating — it just is not
automatically a bug. Also supports `--emit-c`, `--emit-ir`,
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
- design decisions in `notes/decisions/`, one file per DD — `ls notes/decisions/`
  for the count and the highest number in use
- adversarial audits in `audit/run-N/` (run-1: 5 models, 3 passes, remediation
  done)
- **tree-sitter**: not used by the main compiler (hand-written parser). The
  `sash-convert` crate uses **stock** `tree-sitter-bash 0.25` on bash input.
  (The old `~/src/tree-sitter-bash-safebash` fork drove the deleted
  main-compiler pipeline and is no longer part of the build.)

### Crate structure

`main.rs` is a thin wrapper calling `sash::cli_main()`.
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
- `transform/` — IR → SSA transformation
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
- `codegen/` — Cranelift code generation
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
- `notes/journal/` — full journal, one numbered file per session
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
**not** derived from a test run — curated DD clause text — which
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

**Both checks read the source, so neither sees a *retired* id — and reusing one
makes the regression gate compare your new spec against a stranger.** `runs` is
append-only by design, so a deleted spec keeps its history forever. Numbering a
new block at the greppable high-water mark is how three specs landed on
`61.13`-`61.15`, ids last run on 2026-09-27 at `deb8a26`: the collision checks
were clean (nothing current used them), but the sweep's companion query reported
**zero new ids** against a prediction of six, because each "new" id already had
a week-old row. The flips query was comparing a brand-new spec to an unrelated
retired one. Both happened to pass, so it was silent. Take the mark from the
database, not from `grep`:

```sh
sqlite3 spec.db "SELECT MAX(CAST(substr(test_id, instr(test_id,'.')+1) AS INTEGER))
                 FROM runs WHERE test_id LIKE 'native:<PREFIX>.%';"
```

For `61.x` that answered **22** where the source showed 12 — ten retired ids in
between. **Predicting the new-id count and getting zero is the tell**; it is a
finding, not bookkeeping.

**Predict the sweep total before running it.** N new specs in a native-only
file is +N; in a dual-mode file it is +2N. A green sweep whose total is not
what you predicted means specs collided, vanished, or were skipped -- the
collision above was found only that way.

**`run.sh`'s summary is authoritative again as of 2026-09-18 — but only
because the sweep now retires what it no longer produces.** The history is
worth knowing, because the failure mode is silent and it recurred twice.
`test_status` is `tests LEFT JOIN runs`, and `tests` was a *permanent*
registry: a spec you deleted kept its row and its last status forever. Since
`run.sh` prints its summary from that view, the gate's own total drifted
upward with every retirement. The sweep reported 5585 while exactly 5583
checks ran; the ghosts were `c:75.99` / `native:75.99`, deleted one commit
earlier and last genuinely run at `5cfeb24`.

`run.sh` now deletes registry rows for ids the sweep did not produce (DG chose
this over having the summary read the TSV). Three properties to preserve if
you touch it:

- **Mode-scoped.** Only ids whose mode prefix appears in this sweep are
  considered, so `SPEC_MODES="native"` cannot purge the whole `c:` cohort.
- **Capped** at `SPEC_PURGE_MAX` (default 25). A partial or aborted sweep is
  indistinguishable from a mass retirement by looking at the TSV, so over the
  cap it reports the cohort and deletes nothing. Raise it deliberately when a
  large retirement is real.
- **`runs` rows are kept.** They are the cross-commit history; dropping the
  `tests` row is by itself what clears the id from `test_status`.

The purge prints what it retired. **A retirement you did not expect is a
finding** — it means a spec vanished, not that the bookkeeping tidied itself.

The independent source-derived check is the **sum of every spec file's
`plan=`** (native runs every file; c-mode skips the two native-only ones).
It should now equal the summary exactly:

```sh
cd spec && nat=0; cm=0
for f in *.sh; do case $f in common.sh|run.sh) continue;; esac
  p=$(grep -m1 '^plan=' $f | sed 's/plan=//'); nat=$((nat+p))
  case $f in convert.sh|trace.sh) ;; *) cm=$((cm+p));; esac
done; echo "native=$nat c=$cm total=$((nat+cm))"
```

That is trustworthy only because `plan` is itself verified against each file's
real `chk` count. Check that first, and treat any mismatch as a finding:

```sh
cd spec && for f in *.sh; do case $f in common.sh|run.sh) continue;; esac
  p=$(grep -m1 '^plan=' $f | sed 's/plan=//')
  n=$(grep -ho '\bchk [A-Za-z0-9._$]\+' $f | grep -v skip_chk | wc -l)
  [ "$p" != "$n" ] && echo "$f plan=$p chk=$n"; done
```

`coverage.sh` is the one legitimate mismatch: its variable-id loop runs three
ids behind a single `chk $_id` token. (`convert.sh` and `trace.sh` are the only
native-only files -- their combined `plan` is the exact native-minus-c
difference.)

**When you delete a spec, expect the next sweep to announce the retirement**
and the total to drop by one per mode. Before 2026-09-18 it did the opposite:
the id stayed and the total stayed high *forever*.

**A `ran=` you cannot account for is a finding, not a rounding error.**
`coverage.sh` reported `ran=323` against 321 greppable `chk` tokens. Chasing
that two-count gap is what surfaced both the id collision and the
`test_status` fault above.

**A spec file run standalone uses whatever `sash` is on `PATH`.** `common.sh`
invokes `${SASH:-sash}`; only `run.sh` prepends `target/release`. So
`bash subscript.sh` actually exercises the *installed* `~/.local/bin/sash`,
which can be days stale -- it will fail (or pass) for reasons that have nothing
to do with your tree. Always pass the binary explicitly:

```sh
cd spec && SASH=$(cd .. && pwd)/target/release/sash SASH_MODE=native bash subscript.sh
```

**`SASH=` is not enough for the sister tools, and this one produces a false
FAILURE.** `common.sh` *appends* `target/release` to `PATH`
(`PATH="$PATH:$regular_release"`), so `sash-convert` and `sash-expand` still
resolve to the installed copy, which `SASH=` does not override. On 2026-09-26
a standalone `convert.sh` reported **62 failures** against a tree whose only
change was in `sash` -- every one of them an artifact of an installed
`sash-convert` twelve days old. The same 62 appeared under a neuter, where they
read as signal. Prepend for any file that shells out to a helper:

```sh
cd spec && PATH=$(cd .. && pwd)/target/release:$PATH \
  SASH=$(cd .. && pwd)/target/release/sash SASH_MODE=native bash convert.sh
```

Prepending is harmless for every other file, so just do it always. **A
standalone failure count is not a finding until it survives that.**

**A failing spec has already recorded its own evidence — read the trace before
theorising.** `common.sh`'s `setup()` opens
`spec/.trace/$(date +%a)/<file>.<mode>` and redirects the whole spec file into
it under `set -x` with `PS4='+$LINENO: '`. (`exec 3>&1` is why `chk` writes
`ok`/`not ok` to fd 3 — fd 1 is the trace.) Every command, expansion and
assignment is there, line-numbered. The file is **appended** to, so several
runs on the same weekday coexist; take the timestamp from `spec.db`
(`SELECT committed_at FROM runs WHERE test_id=… ORDER BY rowid DESC`) and match
it against the `date` line each run writes at the top of its block. Retention
is about a day (`-mtime +1`), keyed by weekday name.

On 2026-10-14 I declared a five-spec failure an unreproducible transient and
filed a task saying the harness discards failure evidence. Both wrong: the
trace held the answer verbatim, and DG found it by asking "what about the
xtrace?". **Do not conclude a facility is missing without reading
`common.sh`.**

**An assertion must not be able to match the scaffolding.** Those five specs
asserted `$(grep -c SB <file>) = 0`. Their messages echo the `--sysroot` path,
the path is under the file's `mktemp -d`, and a mktemp suffix is ten random
alphanumerics — so when one came back as `/tmp/tmp.nkqYLkSBp3`, the grep
counted the **directory name**. Roughly 0.2% per sweep (9 positions × 1/62²),
in c mode only because each file *and mode* gets its own dir, and all five at
once because they share one. **It renders identically to a real regression.**
Match the full code shape, `grep -cE 'SB[0-9]{4}'`, never a bare `SB`; and
reproduce an intermittent failure deterministically (recreate the directory by
name) before and after the fix, or you have not fixed it.

**`plan=` inside a spec file is per-mode, not per-sweep.** It is +N even in a
dual-mode file -- the file runs once per mode and `plan` is compared against
that one run's index. Setting it to +2N gives a *passing* file that prints
`[plan=217, ran=211]`; `common.sh` notes the mismatch rather than failing on
it, so only the predicted total catches it.

**`commit_hash` is not a sweep identity, and selecting a baseline by it is how
the regression check went vacuous.** `runs` records the git HEAD sha, so every
sweep run against an uncommitted working tree records the *same* hash. Pick the
baseline with `MAX(rowid) … WHERE commit_hash = 'BASE'` and, the moment BASE is
the hash you are still sitting on, that selects **the run you just did** and
compares it against itself. It reports zero flips and it cannot report anything
else. On 2026-09-26 three sweeps shared `cc0b7a9` — 19538 rows at one hash,
which is 6510 + 6510 + 6518 — and the check passed without looking at anything.

The tell is cheap, so look before you compare. Any hash with more rows than
distinct ids covers multiple sweeps:

```sql
SELECT commit_hash, COUNT(*) AS rows, COUNT(DISTINCT test_id) AS ids,
       ROUND(COUNT(*) * 1.0 / COUNT(DISTINCT test_id), 2) AS sweeps_at_this_hash
FROM runs GROUP BY commit_hash ORDER BY MAX(rowid) DESC LIMIT 5;
```

More rows than distinct ids means more than one sweep at that hash. Keep the
`* 1.0` — integer division reported 26056 rows over 6522 ids as `3` when it is
very nearly four sweeps.

**Compare by row order, not by hash.** Sweeps are never run in parallel, so
each one is a contiguous block of `runs` rowids and a test's previous result is
simply its previous row. The query below is the one to use: its baseline is
`rowid < c.rid`, **strictly** earlier, so it is structurally incapable of
self-comparing no matter how dirty the tree is.

Query with:

```sql
-- Current status
SELECT status, COUNT(*) FROM test_status GROUP BY status;

-- Failing gates
SELECT id, status FROM test_status WHERE status='failing' ORDER BY id;

-- Regression check: this sweep vs the one before it.
-- `cur` joins `tests`, so ids the purge retired cannot raise a phantom flip
-- out of two stale historical rows; `prev` is strictly earlier by rowid, so
-- this can never compare a run against itself.
WITH cur AS (
  SELECT r.test_id, r.status, r.rowid AS rid
  FROM runs r JOIN tests t ON t.id = r.test_id
  WHERE r.rowid = (SELECT MAX(rowid) FROM runs WHERE test_id = r.test_id)
),
prev AS (
  SELECT c.test_id, c.status AS now,
         (SELECT status FROM runs
          WHERE test_id = c.test_id AND rowid < c.rid
          ORDER BY rowid DESC LIMIT 1) AS was
  FROM cur c
)
SELECT test_id, was, now FROM prev
WHERE was IS NOT NULL AND was <> now ORDER BY test_id;

-- Companion: ids with no earlier row at all are new this sweep. Predict this
-- number before you run it; an id you did not add is a finding.
WITH cur AS (
  SELECT r.test_id, r.rowid AS rid
  FROM runs r JOIN tests t ON t.id = r.test_id
  WHERE r.rowid = (SELECT MAX(rowid) FROM runs WHERE test_id = r.test_id)
)
SELECT test_id FROM cur c
WHERE NOT EXISTS (SELECT 1 FROM runs WHERE test_id = c.test_id AND rowid < c.rid)
ORDER BY test_id;

-- Against a named earlier commit, when you do want one: take the FIRST sweep
-- at that hash, never the last, or a shared hash hands you your own run.
WITH base AS (
  SELECT test_id, status FROM runs r
  WHERE commit_hash = 'BASE'
    AND rowid = (SELECT MIN(rowid) FROM runs
                 WHERE test_id = r.test_id AND commit_hash = 'BASE')
)
SELECT b.test_id, b.status AS was, t.status AS now
FROM base b JOIN test_status t ON t.id = b.test_id
WHERE b.status != t.status
ORDER BY b.test_id;
```

**The regression check is a gate, so prove it can fail.** A zero-flip result
is only meaningful if the query is capable of a non-zero one. This costs one
query — comparing each test's *first* recorded status against its current one
should report hundreds:

```sql
WITH cur AS (
  SELECT r.test_id, r.status FROM runs r JOIN tests t ON t.id = r.test_id
  WHERE r.rowid = (SELECT MAX(rowid) FROM runs WHERE test_id = r.test_id)
)
SELECT COUNT(*) FROM cur c
WHERE (SELECT status FROM runs WHERE test_id = c.test_id
       ORDER BY rowid ASC LIMIT 1) <> c.status;
```

It returned 231 on 2026-09-26. **A zero here means the comparison machinery is
broken, not that the tree is clean.**

One caveat on the row-order query: with `SPEC_MODES="native"` the `c:` ids are
neither produced nor purged, so their "current" row is from an older sweep and
their "previous" is older still. Run full sweeps for the gate, or filter by
mode prefix.


Use plain `sqlite3` for **every** database, including `spec.db`. The
indexnudge shim only re-parses the source index when the database it is
handed is the index itself; for anything else it is a pass-through to the
real `sqlite3`. Do **not** reach for `/home/linuxbrew/.linuxbrew/bin/sqlite3`
to "bypass" it — that advice was wrong, and copying it onto
`.indexnudge-rust.db` silently serves stale line numbers.

`runs` is append-only and grows without bound, so both queries above depend on
the indexes in `schema.sql`. Without them the regression query takes about
four minutes; with them, milliseconds. They cost tens of MB of local disk.

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

`mk rt-cov` prints the current baseline; journal 320 recorded the first one.

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

What remains is the in-crate unit tests, run by `mk test`:
- `sash` — parser and codegen units, in `#[cfg(test)]` modules
- `sash-convert` · `sash-expand`

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
Build it with `mk cov-db` after a **`mk cov-union`** — it reads
`target/coverage/union.lcov`, not the spec-only `branch.lcov`, because the
100% target is the union of spec- and unit-test reachability and a spec-only
ranking lists the should-never-run ICE guards as if they were work. That
directory survives `mk cov-reset` (which removes only `target/release`) —
**the lcov cannot be regenerated afterwards**, because `cov-reset` drops the
covmap along with the instrumented binary. The summary table names thin
*files*; only the lcov names thin *lines*.

**Run `mk cov-stale` before you rank.** Every row in `coverage.db` is keyed
by `(file, line)`, so a single commit that renumbers a measured file silently
re-points every cold line onto whatever now occupies it. Measured once, after
18 such commits: **184 of 519 cold lines (35%) landed on a blank line, a
comment, or a bare brace** — text that cannot carry a DA record at all, and
`validate.rs:2056` read as cold code when it is the enum variant `Unknown,`.
The loader records the HEAD commit and each measured file's worktree blob, so
the check is mechanical and exits nonzero when the data has moved. It also
refuses to attribute an lcov older than the commit it is loaded at, because a
confident wrong provenance is worse than none.

**Aggregates survive staleness; rankings do not.** `519/27538` does not care
which line is which, so quoting overall coverage from a stale db is fine. The
cold-line ranking is the only reason to have the db, and it is exactly the
part that rots. This replaced a prose "do not read it" warning that failed the
way prose warnings fail (journal 428).

**Function coverage has two independent sources and they share no code path.**
The headline figure is `cargo llvm-cov report`'s, printed by `mk cov`,
computed from the covmap. `coverage.db`'s is the lcov's, and was wrong by 36
points for three days without anything noticing. Quote the former; use the
latter only for ranking.

**And a WARM line is not coverage.** The converse failure is silent and the
coverage tooling cannot see it at all: a rule can be raised, reached, and
asserted by nothing. Measured 2026-10-13, SB5027's line had six hits and three
specs ran programs that raise it — but all three asserted only `-n $err`, and
those programs trip SB5020 *first*, which satisfies that on its own. Deleting
the rule outright would have left the whole suite green. A co-fired diagnostic
masks the one you meant to pin, so assert the **code**, never bare `-n $err`.

The cheap census — a code is covered only if a spec *names* it. **Filter
comment lines on both sides**, or the output has a permanent nonzero baseline
and stops being a gate:

```sh
SRC="grep -rhE 'SB[0-9]{4}' sash/src/ | grep -vE '^[[:space:]]*//' \
     | grep -oE 'SB[0-9]{4}' | sort -u"
SPEC="grep -hE 'SB[0-9]{4}' spec/*.sh | grep -vE '^[[:space:]]*#' \
      | grep -oE 'SB[0-9]{4}' | sort -u"
comm -23 <(eval "$SRC") <(eval "$SPEC")   # raised, asserted by nothing
comm -13 <(eval "$SRC") <(eval "$SPEC")   # asserted, raised by nothing
```

Both are empty as of 2026-10-14, so **any** line in either is a finding.

Three things about the pattern, each learned by getting it wrong first:

- **Match the code anywhere, not as `"SB[0-9]{4}"`.** The quoted form only sees
  codes passed as a standalone argument to a `Diagnostic` constructor, and
  **17 of 153** are not — they are interpolated into a hand-rolled
  `format!("{}:{}: error [SB5055] …")`. A rule emitted that way and asserted by
  nothing is *invisible*. Same format-string blind spot this file already warns
  about for message text.
- **Filter comments, or you measure the wrong thing.** Unfiltered, the forward
  direction reports three retired codes that survive only in explanatory
  comments, and — worse — the *spec* side counts a code named in a `#` comment
  as asserted. Filtering the spec side is what surfaced SB4017 / SB4021 /
  SB4023 / SB4024.
- **Run it in both directions.** `comm -13` is how the format-string blind spot
  was found at all, and it caught a `validate.rs` doc comment asserting "a
  separate rule (SB4042, type_check)" — a code DD#80 *considered and deferred*,
  so no SB4042 has ever existed.

**A bare `-n $err` is not an assertion.** It is satisfied by any diagnostic, so
a spec written for one rule passes after that rule is deleted, as long as the
program still fails for some other reason. That is how SB5027 hid behind a
co-fired SB5020, and how `55.13`/`55.14` sat as the only positives for SB4016
and SB4017 without naming either. Assert the **code**.

**Then prove a fix by reversion**: stub the rule's condition to `false` (or
misspell its code), rebuild, and count the red specs. Five, where the same stub
had previously turned one — that number is the result, not the green run. It
also tells you when a new spec is a *rival*: stubbing SB4017 turned `55.14` red
too, which is how a just-written positive was found to be a duplicate of a
spec that had been there all along.

**A cold line is a hypothesis, not a finding.** Confirm it by execution before
building any work on it. `mk cov-probe-build` once, then
`contrib/cov-probe.sh FILE:LINE 'snippet'` answers "does this snippet reach
that arm" in under a second — note it reports *branch* records, so aim it at
the condition, not at a statement in the body. Grepping a diagnostic's text is
**not** a substitute: a message built with `format!("… {} …", type_name(t))`
renders the same sentence a literal grep will never find, so a passing spec
that asserts the message can belong to an entirely different renderer. That is
exactly what hid `check_scalar_assignment`'s cold arm (journal 397).

**A diagnostic CODE is not a site, and `$err == *SB4014*` does not pin one.**
`SB4014` is raised at **eleven** places in `parse/`; `SB4020` at several. A
spec asserting the bare code passes if *any* of them fires, so "chk 93.19
covers this arm" was a conclusion about the wrong site — the arm was cold the
whole time, and a unit test was retired against that non-evidence (journal
475). Before accepting a spec as cover for a specific line, check how many
sites share the code (`grep -rn '"SB4014"' sash/src/`); if more than one,
the spec must assert the distinguishing **message text**, not the code. The
same reasoning applies to a unit test offered as cover: `atok_to_text`'s
40-arm test asserted the renderer in isolation and would have passed while
the call site formatted with `{:?}` — which *was* the original bug. Prefer
the spec that reads the message the user actually gets.

**The fastest reachability probe is to make the arm `panic!` and rebuild — but
a negative result needs a positive control.** `cargo build` can no-op, a patch
can fail to apply, and a `grep` for the marker can swallow it; all three look
identical to "the arm was not reached". Run something that *must* reach the arm
first (its unit test, usually) and confirm the panic fires, then run the real
source. Journal 474: the first run of exactly this experiment produced silence
from an unverified binary and was worth nothing.

**Surface behaviour agreeing with your hypothesis is not evidence for the
mechanism.** `$(f)` compiling clean and "`cmd_subst_taint`'s `user_functions`
arm returns Clean" are different claims; the first is true and the second is
false, and ICE-ing on the strength of the first would have crashed every
program using `$(f)`. Probe the arm, not the outcome.

**For many candidates at once, use `contrib/cov-batch.sh DIR` instead** — it
runs a whole corpus of candidate programs once, renders one lcov, and reports
every line that is cold in `coverage.db` and warm now. Per-line probing costs
an lcov render *per line* and re-runs the corpus each time; this is one cycle
for twenty spellings. `contrib/cov-batch.sh --spec DIR` runs spec **files**
under the harness instead, which is the check that closes the loop: the
snippet that warmed the line and the spec you actually wrote are different
programs, and only the second one counts. Both need `mk cov-probe-build`
first and `mk cov-reset` after.

**An empty `cov-batch` report is ambiguous until you prove the tool can report
a positive.** "Nothing warmed" and "the corpus never ran usefully" print
identically, and the first one is the interesting answer only once the second
is ruled out. Put one candidate in the corpus you can *reason* reaches a cold
line, and confirm it lands, before concluding that twelve others missed.
Measured 2026-10-12: twelve map-literal programs warmed none of the three cold
`ExprKind::AssocArray` arms — the real answer, since every caller decomposes
the literal first — but it only became a finding after a two-program control
run came back with both of its lines. That control run is also what surfaced
the two genuine SB4037 gaps.

**Rank by function, then read the whole function.** `first_expansion` has two
cold lines in two unrelated arms, so a ranking by per-function count sorted it
below every function with three in one — and the two arms were cold for
different reasons again. Counting finds clusters; it does not find families.

**A second axis: intersect cold lines with SB raise sites.** Ranking by
function finds clusters; this finds *lies*. A cold walker arm needs a probe
before you know what it is, but a cold `parse_error(.., "SB4014", ..)` labels
itself — someone thought a user could reach it, so either they were right and
no spec says so (category 1), or they were wrong and the diagnostic advertises
a condition the language cannot produce. SQLite has no regex, so use `GLOB`:

```sql
SELECT file, line_num, content FROM lines
WHERE file LIKE 'sash/src/%'
  AND content GLOB '*SB[0-9][0-9][0-9][0-9]*'
  AND trim(content) NOT GLOB '//*';
```

Drop the test modules and the `.expect(` / `unreachable!(` / `panic!(` /
`assert` lines, then join against
`SELECT file||'|'||line FROM cov_lines WHERE count = 0`. Measured 2026-10-14 at
98.23% line coverage: **277 raise sites, 148 distinct codes, 56 codes with more
than one site, 3 cold** — one genuine gap and two unreachable diagnostics,
which is a good hit rate for one query. The 56-codes-with-multiple-sites figure
is the other product of the same extraction, and it is what tells you whether a
spec may assert a bare code (see "A diagnostic CODE is not a site").

**A cold line can also be a measurement fault, and merging more profraws is
what causes it.** LLVM does not give every region a physical counter — it
keeps the minimum and derives the rest as *expressions*, and a loop body's
count is routinely derived from the back-edge. A process that calls
`std::process::exit` from inside a loop increments the counters on the way in
and never the one on the way round, so its contribution to the expression is
inconsistent. Summed over a sweep's profraws the error accumulates, and once
it exceeds the true count the line reports **zero**. Measured 2026-10-06:
`lib.rs:216` is executed by `cli.6`, reads 1 from that run's own profraw, and
reads 0 from the 45-profraw merge — passing through `18446744073709551614`,
which is −2, on the way.

So when a cold line contradicts a spec you can show exercises it, **believe
the spec and export one profraw**. A single process cannot underflow:

```sh
LP=$(rustc +nightly --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin
$LP/llvm-profdata merge -sparse -o /tmp/one.profdata target/safebash-<pid>-*.profraw
$LP/llvm-cov export -format=lcov -instr-profile=/tmp/one.profdata \
    target/release/sash | grep -A99999 '^SF:.*lib.rs$' | grep '^DA:216,'
```

Scanning an lcov for counts above `9000000000000000000` finds the cases that
underflowed without landing back on a plausible number; a clean scan does not
clear the file, because the arithmetic can pass through zero. The whole
470-line cold cohort was checked this way and exactly one line was wrong, so
this is rare — but it is silent, and it costs an hour if you do not know the
shape. All 22 `process::exit` sites are in `lib.rs`; nearly all fire at top
level after a pass returns, which is why it is rare. `parse_heap_size` is the
one that exits mid-loop with a caller's frame live.

**A cold non-ICE line is one of four things** (DG's rule, 2026-10-04: *every
line warm by spec except ICEs first, then a unit test for the final ICE
lines*). The four decide **which artefact** warms a line, not whether it is
work — that rule is an order of operations, and categories 2-4 are all
reachable by a direct call even though no program reaches them. Reading "no
spec can warm it" as "exempt" is what let five functions carry a
do-not-re-open stamp for several sessions (journal 496); the 100% target is
the union of spec and unit-test reachability, so every category below lands
in the total:

1. a genuine spec gap — write the spec;
2. a **validate-shadowed** arm in a later pass, for a form the first gate
   already rejects (the compound-pipeline stage is SB4022; the pipeline
   recursion diagnostic is SB5037). No spec can warm these;

   **Measure the shadow; do not assume it.** This bullet used to claim
   "`detect_array_iter`'s five arms are all SB5026 / SB5073 / SB5009".
   Probed 2026-10-04, that is **false**: its `_ => None` is warm — `for x in
   a b c` compiles and runs — and only two of the arms are genuinely shadowed
   (a non-`keys` command substitution is SB5009, a zero-argument `keys` is
   SB4045). A function does not land in this category wholesale; each arm
   earns it separately, by a probe.

   Note also what a shadowed arm here *returns*: `None`, meaning "not an array
   iteration". That is a classification, not a refusal, so if SB5009 were ever
   relaxed the form would compile down the wrong path in silence. Whether an
   arm like that should stay an answer or become an ICE is the open question
   below;
3. a **totality** arm, present only because the no-wildcard convention
   requires it (`is_taint_source`'s `AssocArray`/`Test`, and friends). Do not
   delete these — the convention has caught three bugs in three sessions;
4. an **ICE** arm — a unit test, and only after (1) is done for that function.

(2) and (3) get a unit test too, after (1). Unlike an ICE arm, they do real
work, so the test must assert the work is **right** rather than merely run the
line: `walk_arith_subscripts`' `SubscriptAssign` arm differs from its
`Subscript` sibling only in descending into `value`, so that is what its test
pins. An unexecuted *correct-looking* arm is the shape that let `SsaInst::Test`
escape the SSA taint pass for its whole life.

Classify before mining. Ranking raw cold-line counts puts (2)-(4) at the head,
because rustfmt spreads a `panic!(…)` or an `Err(…)` across five lines while a
real gap is often one.

**The cohort, measured 2026-10-04** at 98.39% line / 98.90% function coverage —
which is the point, because the aggregate looks finished and the cohort is not.
Of 413 cold lines: **79** are punctuation or comment (text that cannot carry a
DA record), **44** are already an ICE, and **290 deliver an answer**. Those 290
are the whole of categories (1)-(3), unclassified. By file: `validate.rs` 37,
`parse/parser.rs` 30, `emit_c.rs` 29, `transform/stmt_handlers.rs` 25,
`type_check.rs` 24, `transform/expr_handlers.rs` 19,
`transform/control_flow.rs` 14. Reproduce with `contrib/`-free SQL over
`coverage.db` joined to the index, looking back six lines for an open
`unreachable!(`/`panic!(` so rustfmt's continuation lines are not miscounted as
answers.

**Writing an ICE test (category 4).** The ICE *is* the claim:
`unreachable!("SB4045 rejects a zero-argument argv")` asserts something about
a *different* pass, and nothing in the type system holds it. The spec suite
can never test it, because the spec suite can only write programs the
validator accepts. So call the function **directly** with the shape the
earlier pass rejects, and assert the ICE text names the rule that guarantees
it. A test that stops panicking is the finding, not a cleanup.

**An ICE is a regression guard, not a defense** (DG, 2026-10-04: *"the panics
are guards against regression — they never need to be the sole defense, as a
regression would be something we fix"*). This kills the objection that keeps
resurfacing — "but a parser/validator conventionally errs rather than panics,
and a future change that forgets the invariant will now crash." The invariant's
*enforcer* is the defense; the ICE only has to notice when the enforcer stops
being one. A regression is not a condition to degrade gracefully under, so the
ICE's quality as a user-facing message is not a consideration. And the
"conventionally errs" rule is about **user** input — an arm unreachable by user
input is outside its scope. Do not re-litigate this when converting a loud
`Err` that guards a compiler invariant; convert it.

Put the module in the **same file** (`#[cfg(test)] mod invariant_tests`) —
most of these functions are private. Use `#[should_panic(expected = …)]` for a
single site; `std::panic::catch_unwind(AssertUnwindSafe(…))` plus a payload
downcast when you want to loop over several shapes or assert on the message.
`sash/src/transform/stmt_handlers.rs` and `sash/src/ir.rs` are the two worked
examples.

**Counting ICE tests: grep undercounts.** Measured 2026-10-05, `sash` has
**78** unit tests of which **65 are ICE assertions** and 13 are ordinary. (It
was 80/68/12 on 2026-10-04; the duplicate-backstop elimination below retired
three ICE tests, and the three ordinary ones written to replace them were
deleted in turn when their arm was.) A grep for `catch_unwind`
inside test bodies undercounts, because some tests reach it through a shared
helper (`stmt_handlers::ice_on`, `emit_c::ices`, `emit_c::empty_args_ice`), so
any census has to resolve same-file helpers before it means anything — a census
written for this entry missed `empty_args_ice` and reported 66/14.

**An ICE must catch a regression ORTHOGONALLY, or have no safe alternative.**
(DG, 2026-10-05: *"if the backstop is essentially an identical test to the
original then why is it more trustworthy; whereas if it has an orthogonal way
to catch a problem, that is perhaps when it would be most valuable"*.) Two
tests, applied in order:

1. **Is it orthogonal?** The mechanical check is the ICE's own *parameter
   type*. If the parameter still holds the exact AST the named rule inspected
   (`&ExprKind`, `&Command`, `&[Expr]`) **and** that rule is a `validate.rs` /
   `type_check.rs` gate, nothing happened in between: the panic is a second
   copy of one assertion, no more trustworthy than the gate and free to drift
   from the rule it quotes. If the parameter is `SsaValue` / `SsaInst` /
   `&str` / a rendered `String`, **or** the named rule is a *parse* rule (so
   the data went tokens → AST), it is orthogonal — it also covers bugs in the
   lowering, which the gate structurally cannot see. Measured across all 107
   ICE sites: only 20 name an SB rule at all, and of those 11 were duplicates.
2. **If it is a duplicate, what does declining cost?** Prefer deleting the
   panic — but only when `None`/the fallback is a *safe* answer. `detect_array_iter`'s
   `None` routes to the word-list path the gate already vetted, so the four
   guards went. `handle_argv`'s does not: `try_builtin` returns
   `Option<BlockId>` where `None` means "not a builtin", so a declined builtin
   compiles clean and dies at runtime with `sash: bug: bare command passed to
   sash_exec` — measured, eight `argv` spellings reached it. There the panic is
   the only way to say "malformed", and it stays. **Cite the consequence, not
   the gate name**: a comment that only says "SB4045 guarantees this" is
   restating the duplicate.

A third outcome exists — **the answer is knowable, so give it** — but it only
applies when the arm becomes *reachable*. `len @a` / `join @a` ICE'd on a splat
operand; replacing the panic with the correct answer (`pe.name`, since DD#100
Part D retired `@a` as an exact synonym of the bare name) removed the duplicate
assertion but left the arm unreachable, because SB5062 rejects every splat
spelling. **DG rejected that (2026-10-06): an unreachable arm handling a
rejected spelling is worse than either option, because it documents the
spelling as supported.** The arm was deleted outright. Reach for "give it the
answer" only where something can actually arrive; otherwise delete, and let the
gate's diagnostic be the single place the rule is stated. See "Cold arms: ICE
or answer?".

**A unit test asserting behaviour a spec already pins is a rival — delete it.**
The in-crate `#[cfg(test)]` modules were never covered by journal 304's redux
(that did `sash/tests/`), so they accumulated 30 of them. Before writing one,
check whether the spec suite already asserts the claim; before keeping one,
check the same. The survivors pin things a spec structurally cannot see:
cross-API invariants (peek-then-next == next), pass separation, desugaring
AST-equality, ICE-message rendering, and the ICEs themselves.

**Some ICE arms cannot be reached by a unit test either**, because the
*same function* already covers the case — `arith_to_ssa`'s trailing arm is
unreachable because the match names twelve `ArithExpr` variants and the leaf
helper called first returns `Some` for the other four. Do not chase these.
Make the arm name its variants instead of `_` (so a new variant is a compile
error rather than a runtime ICE) and say so in the doc comment.

```sql
ATTACH 'coverage.db' AS cov;
SELECT s.file, s.name, COUNT(*) AS uncov
FROM cov.cov_lines cl
JOIN symbols s ON s.file=cl.file AND cl.line BETWEEN s.start_line AND s.end_line
WHERE cl.count=0 AND s.kind IN ('function','method')
GROUP BY s.file, s.name, s.start_line ORDER BY uncov DESC;
```

**Lines defined inside a `#[cfg(test)]` module are excluded from
`coverage.db`.** `covdb.py` scans for `#[cfg(test)]` + `mod NAME {`, closing
at the next column-0 `}`, and drops their DA/BRDA/FN records — 1256 lines,
182 functions as of 2026-10-11, and `mk cov-db` prints the counts so a scan
that matched nothing is visible. Unit tests still **warm** production lines,
which is the entire point of `mk cov-union`; only lines *defined* in test code
are dropped. Without this, test-only helpers rank as production work:
`ice_text` sat in the queue with four cold lines on the strength of a downcast
arm no ICE payload takes.

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
they drift down as the codebase refactors, so any figure written here is
wrong by the time it is read. Re-measure rather than quote:

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

**Residuals vs. Mozilla.** Both metrics are close to
`rust-code-analysis-cli` but not identical. The large majority of functions
agree exactly. Of those that do not, the divergence is a few points and has a
consistent direction: **indexnudge never reports a cyclomatic score above
Mozilla's** — it is low or equal, always — and cognitive is usually low too,
occasionally high on deeply nested parsers. Treat an indexnudge number as a
floor for cyclomatic.

**Do not quote a residual table — re-measure.** An earlier version of this
section named three `sash/src/transform/` functions as under-reporting by
2-9 points against a "big match on CST kind" shape. That is obsolete: the CST
was deleted in the parser migration (commit `1c0e64a`), those functions are a
fraction of their former size, and re-measured 2026-09-24 all three agree with
Mozilla **exactly**. The stale numbers caused a real over-estimate of 3-20x
when costing work in that directory.

The known structural cause that remains: an inner `function_item` nested
inside an outer one reports `depth = 0` in indexnudge and `depth = 1` in
Mozilla, so cognitive can differ by a point or two. Item-level nested fns are
rare in Rust.

To re-measure:

```sh
cargo install rust-code-analysis-cli --locked   # --locked is REQUIRED
rust-code-analysis-cli -m -p sash/src/<file>.rs -O json
```

and read `metrics.cyclomatic.sum` / `metrics.cognitive.sum` from each
`"kind": "function"` space. **Without `--locked` the install fails** — the
unpinned resolve pulls two incompatible `tree_sitter::Language` types and dies
with E0308.

When setting an absolute threshold, allow a few points of slop rather than
trusting either tool to the unit. The enforced gate uses the indexnudge
numbers on both sides of its comparison, so the residual does not affect it.

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
one that carries the behavioural weight — it outnumbers the unit tests by
roughly two orders of magnitude — so when the two disagree about whether
something is covered, believe the specs. Keep running anything long under
`timeout`.

## Bug policy

Zero tolerance. Finding a bug is a stop-everything event. Never work
around it. If you notice yourself routing around a behavior, STOP.
Investigate whether it's a bug.

## Cold arms: ICE or answer?

Declared-total walkers (`do not add a wildcard`, enforced by
`contrib/totality-check.py`) need an arm per variant, including shapes no
source can currently produce. Those arms show up as cold lines every time the
coverage queue is ranked. The test for what the body should say is:

> **Unreachable by construction → `unreachable!()`. Merely unexercised →
> the correct arm.**

An arm cold because a *compiler invariant* forbids it can only be reached by
breaking that invariant — which requires editing the compiler's own source, so
the ICE fires in the opener's spec sweep and never reaches a user. A correct
recursion there is an **unverified claim**: nothing exercises it, so the route
opens in silence. That is the same failure shape as the `_ => {}` that let
`SsaInst::Test` and `RedirectPush` escape the SSA taint pass for its whole
life. For a taint walker, silent-and-maybe-wrong is the worst option.

An arm cold merely because no spec hits it would crash a real user. Leave it,
and pin the answer with a unit test.

**Coverage cannot tell the two apart** — only enumerating construction sites
can, and the enumeration must be of the *producer*, not the transform you
think removes it. Journal 439: `collect_expr_var_name`'s redirect arms looked
dead via `spill_scalar_redirect_targets`, and the spill has two call sites —
but `cmd_handlers.rs:296` builds an `SsaInst::RedirectPush` that goes through
neither. Checking the spill's callers said "unreachable"; checking
`RedirectPush`'s said "reachable". Three of eight candidates failed that way.

A green sweep is **corroboration, not proof** — "merely unexercised" is silent
by definition. Pin a converted arm with `#[should_panic(expected = …)]`: the
panic *is* the claim that the shape is still unreachable.

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
