# AGENTS.md — Field Notes for AI Contributors

## What this project is

safebash is a compiler for a safe subset of Bash. It produces signed
native artifacts verified at execution time. DG created the project
and directs the effort. The AI implements: writing Rust, designing
IRs, writing docs. Neither works alone — DG makes design decisions;
the AI has the stamina for incremental experiments.

Read the journal prologue (`exp/JOURNAL.md`, first 40 lines) before
your first session. It sets the tone for everything.

## The experiment model

Work proceeds as numbered experiments in `exp/`. Each experiment:
- Lives in its own directory (`exp/NNN-short-name/`)
- Has an `mkfile` with a `test` target (Plan 9 `mk`, not `make`)
- Produces a working artifact that demonstrates one concept
- Is documented in `exp/JOURNAL.md`

Don't skip ahead. Don't combine experiments. Each one should pass
before the next begins.

## Completion checklist

Every task ends with:
1. All tests pass (`cd exp && mk testall`)
2. Update `exp/JOURNAL.md` (goals, actions, reasoning)
3. Update `exp/GUIDE.md` if new architecture introduced
4. Add deferred work to `exp/TODO.md`
5. Update `.github/copilot-instructions.md` if SOP changed
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

## Adversarial review (future practice)

Starting at phase 3 (IR generation), run an adversarial review pass
on significant changes. Frame it as: "find bugs, find ways to bypass
the safe subset, find logic errors in the validator." This is about
credibility and catching blind spots — a security tool written by AI
must demonstrate independent scrutiny. See journal entry 2026-03-28
for rationale. This does NOT replace the Janet oracle — Janet
validates semantics; adversarial review validates implementation.
