title: Stop extracting functions below CC 50
status: done
priority: P3
kind: process
source: opus-47-review
depends: []
resolved: 2026-06-07
resolution: Superseded by the cognitive-complexity rubric (journal 108) and the enforced complexity gate (commit c51e296). The gate currently triggers at CC > 16.8 (3× mean), strictly tighter than the CC > 50 floor this task proposed; the grandfather-on-modify mechanic plus the cog-vs-cc delta-aware framing also handles the original concern (don't extract for noise) more precisely than a single numeric floor would. Phase 11/12 refactors validated the new framework in practice. The task's intent — codify a discipline so 4.6-style noise commits stop happening — is fulfilled by the gate + rubric.
gates:
  - one-line addition to AGENTS.md or contrib/cyclo.sh comments

Recent commits (`9eff657`, `7e9baec`, `8cb2f13`) extracted individual
match arms from already-moderate functions into named methods. This
trades line count for indirection without changing the underlying
decision tree. Below ~CC 50 the extractions become diff noise that
hurts review.

## Action

Document the rule in two places:

1. **AGENTS.md** — add a sentence under "Style":
   > Refactor extractions are valuable above CC 50 and noisy below it.
   > Do not extract a `match` arm into a named function unless the
   > function it lives in is over the ceiling, or the arm has multiple
   > callers.

2. **`contrib/cyclo.sh`** — header comment that documents the
   convention so future readers see it next to the tool that measures
   it.

## Why P3 (cheap, no decision needed)

This is process discipline, not code. Independent of every other task.
The cost of *not* writing it down is more 4.6-style noise commits.

## Out of scope

- Reverting any of the existing below-50 extractions. They aren't
  bugs; they're just clutter the next pass shouldn't repeat.
- A hard `--max-cc 50` gate — that's P2 (depends on D3).
