# AGENTS.md — Field Notes for AI Contributors

## What this project is

safebash is a compiler for a safe subset of Bash that produces
static native binaries. DG created the project and directs the
effort. The AI implements: writing Rust, designing IRs, writing
docs. Neither works alone — DG makes design decisions; the AI has
the stamina for incremental experiments.

The compiler works end-to-end: bash source → parse → validate →
IR → SSA → Cranelift codegen → link → 16KB static ELF. Output is
byte-identical to Bash.

Read the journal prologue (`exp/JOURNAL.md`, first 40 lines) before
your first session. It sets the tone for everything.

## Current state

- **30 experiments** (001–026, plus 022a/023a/023b variants)
- **~520 tests** across all experiments
- **13 design decisions** documented in JOURNAL.md
- **1 adversarial audit** completed (5 models, 3 passes, remediation done)
- **sash-rt/** — pure Rust `#![no_std]` runtime crate (rustix, no libc)
- **Pipeline:** tree-sitter → validator (7 rules) → IR → SSA → Cranelift → ld

Key files:
- `exp/GUIDE.md` — Architecture overview
- `exp/JOURNAL.md` — Detailed experiment journal + design decisions
- `exp/SUBSET.md` — Safe subset specification
- `exp/TODO.md` — Backlog
- `exp/TIMELINE.md` — Chronological milestones
- `audit/run-1/AUDIT_TRIAGE.md` — Adversarial audit findings

## The experiment model

Work proceeds as numbered experiments in `exp/`. Each experiment:
- Lives in its own directory (`exp/NNN-short-name/`)
- Has an `mkfile` with a `test` target (Plan 9 `mk`, not `make`)
- Produces a working artifact that demonstrates one concept
- Is documented in `exp/JOURNAL.md`

Don't skip ahead. Don't combine experiments unless the scope is
clearly related. Each one should pass before the next begins.

## Completion checklist

Every task ends with:
1. All tests pass (`cd exp && mk testall`)
2. Update `exp/JOURNAL.md` (goals, actions, reasoning)
3. Update `exp/GUIDE.md` if new architecture introduced
4. Add deferred work to `exp/TODO.md`
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
