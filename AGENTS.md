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

Read the journal prologue (`doc/JOURNAL.md`, first 40 lines) before
your first session. It sets the tone for everything.

## Current state

- **sash/** — the compiler (Rust, Cranelift backend)
- **sash-rt/** — pure Rust `#![no_std]` runtime crate (rustix, no libc)
- **sash-expand/** — expansion utilities
- **sash-rt-tests/** — runtime test harness
- **doc/** — JOURNAL, GUIDE, SUBSET, TODO, TIMELINE
- **audit/** — adversarial audit findings and remediation
- **contrib/** — contributed scripts/tools
- **suites/** — test suites
- **exp/** — git submodule → safebash-exp (30 historical experiments, not needed for build)
- **457 tests** (247 codegen, 114 e2e, 37 janet, 59 oracle)
- **45 design decisions** documented in `doc/JOURNAL.md`
- **1 adversarial audit** completed (5 models, 3 passes, remediation done)
- **Tree-sitter fork** at `~/src/tree-sitter-bash-safebash` with custom grammar
  (int, const, array, map, bless, fn, `${var:s/re/str/}`)

### Pipeline

tree-sitter CST → validate (rules + taint) → lower (CST→IR, coercions)
→ type check → transform (IR→SSA) → fold → codegen (Cranelift) → link → static ELF

### Key source files (`sash/src/`)

- `main.rs`, `lib.rs` — entry points
- `lower.rs` — CST → IR lowering with coercions
- `ir.rs` — IR definitions
- `type_check.rs` — type checking pass
- `transform.rs` — IR → SSA transformation
- `ssa.rs`, `ssa_printer.rs` — SSA representation and printing
- `codegen.rs` — Cranelift code generation
- `taint.rs` — taint analysis
- `validator.rs`, `rule.rs` — validation framework
- `rules/` — `mod.rs`, `dynamic_execution.rs`, `indirect_expansion.rs`,
  `export_rejected.rs`, `heredoc_cmdsub.rs`
- `diagnostic.rs` — error reporting
- `emit_janet.rs` — Janet backend
- `emit_source.rs`, `printer.rs` — source emission modes
- `known_externals.rs` — allowlisted external commands

### Key docs

- `doc/GUIDE.md` — Architecture overview
- `doc/JOURNAL.md` — Detailed journal + design decisions (through DD#45)
- `doc/SUBSET.md` — Safe subset specification
- `doc/TODO.md` — Backlog
- `doc/TIMELINE.md` — Chronological milestones
- `audit/run-1/AUDIT_TRIAGE.md` — Adversarial audit findings

## Build and test

Build system uses Plan 9 `mk` (not `make`). Top-level `mkfile` has
`build`, `test`, and `clean` targets.

Tests live in `sash/tests/`:
- `codegen_tests.rs` (247) — codegen correctness
- `e2e_tests.rs` (114) — end-to-end compilation and execution
- `oracle_tests.rs` (59) — compare output against Bash
- `janet_tests.rs` (37) — Janet backend

## Completion checklist

Every task ends with:
1. All tests pass (`mk test`)
2. Update `doc/JOURNAL.md` (goals, actions, reasoning)
3. Update `doc/GUIDE.md` if new architecture introduced
4. Add deferred work to `doc/TODO.md`
5. Update this file if SOP changed
6. `git commit` with descriptive message
7. `git push`

## Bug policy

Zero tolerance. Finding a bug is a stop-everything event. Never work
around it. If you notice yourself routing around a behavior, STOP.
Investigate whether it's a bug.

## Attribution

Be explicit about who did what. DG directs the project and makes
design decisions. The AI implements. Don't credit the AI for DG's
design choices. When unsure, say so.

## Adversarial review

The project has completed one adversarial review (run-1, in
`audit/run-1/`). The process: write focused prompts, run across
5+ diverse LLM models, triage and deduplicate findings, generate
remediation plans, reconcile, implement fixes.

Schedule another adversarial review after significant new code
(the adversarial-audit skill in DG's dotfiles describes the full
process). Validator bypass testing is the highest priority — find
Bash scripts that achieve dangerous effects while passing the
validator without diagnostics.
