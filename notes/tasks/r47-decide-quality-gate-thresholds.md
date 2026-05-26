title: Decide enforcement thresholds for fmt, coverage, complexity
status: done
priority: D3
kind: decision
source: opus-47-review
resolved: 2026-05-26
resolution: fmt + lint = commit hooks (enforced). Complexity + coverage = report at commit time, thresholds enforced soon.
depends: []
gates:
  - DD or amendment

## Resolution (DG, 2026-05-26)

> fmt and lint should be commit hooks, continue measuring complexity
> and coverage — report at commit time, thresholds to be enforced soon.

Enforcement model:

| Gate | Where | Action | Threshold |
|---|---|---|---|
| fmt | pre-commit hook | block on diff | zero (run `cargo fmt` to baseline) |
| lint | pre-commit hook | block on warning | `clippy -D warnings` (already enforced via `mk lint`) |
| complexity | pre-commit hook | report only | none yet, soon |
| coverage | pre-push hook (slow) | report only | none yet, soon |

Implementation in
[r47-enforce-quality-gates](r47-enforce-quality-gates).

`mk fmt lint coverage complexity` exist (since `a3f70a2`) but only
`lint` (`clippy -D warnings`) is enforced. The others are measurements.
The baselines from `a3f70a2`:

- fmt: 1137 diffs (never closed to zero)
- coverage: 71.3% line coverage (no floor)
- complexity: avg 8.7, max 197 (now 88 — `tokenize_arith`)

## What needs to be decided

For each gate, pick one of: enforce now / enforce at a threshold /
measure-only / drop.

1. **fmt**: run `cargo fmt` once to close 1137 diffs to zero, then
   enforce `--check`? Or keep as advisory?
2. **Coverage floor**: what %? 75? 80? 85? Apply to `sash/src/` only or
   include `sash-rt/`, `sash-convert/`?
3. **Complexity ceiling**: what number? After P1 lands, max should be
   ≤50. Enforce `--over 50` in `mk check`?
4. **`mk check` membership**: today `check:V: lint test complexity`.
   Should it include `fmt` and `coverage`?

## Why DG should decide

These are values calls (how strict, how soon) and partly schedule calls
(some can't enforce until P1/related work lands). The numbers also
shape AI behavior — a 50-CC ceiling stops the kind of below-50 noise
extractions called out in the review.

## Inputs for the decision

- Current `mkfile` (top-level)
- `bash contrib/cyclo.sh sash/src --top 25` (current state)
- `notes/opus-47-review.md` — sections #2 and #3
- The fact that mass-formatting is a single noisy commit but pays back
  on every future PR

## Output

A short note (DD or AGENTS.md amendment) listing each gate, its
threshold, when it becomes enforced. Gates `r47-enforce-quality-gates`
(P2) and `r47-set-extraction-floor-cc50` (P3).
