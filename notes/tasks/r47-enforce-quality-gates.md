title: Install commit hooks for fmt + lint, report complexity + coverage
status: done
priority: P2
kind: work
source: https://github.com/dan4thewin/safebash/blob/f02477dd31e67a0fe6fe3c4a359935569c4715d8/notes/opus-47-review.md
resolved: 2026-06-05
resolution: .githooks/pre-commit runs cargo fmt --check + cargo clippy -D warnings as blocking gates, and a top-5 cyclomatic-complexity report (cyclo-allow-aware) as report-only. .githooks/pre-push runs llvm-cov coverage report (skip via SKIP_COVERAGE=1). Both exercised live during the Phase 11 wave.
landed: 2026-05-27
depends:
  - r47-decide-quality-gate-thresholds
gates:
  - pre-commit refuses commits with rustfmt diff
  - pre-commit refuses commits with clippy warnings
  - pre-commit prints current complexity top-10
  - pre-push prints current coverage summary
  - hooks live in repo (.githooks/) and are wired by `mk install-hooks`
  - one commit per hook so reverts are surgical

## Direction (DG, 2026-05-26)

> fmt and lint should be commit hooks, continue measuring complexity
> and coverage — report at commit time, thresholds to be enforced soon.

Two-phase: measure-and-display now, threshold-enforce later.

## Scope

| Gate | Hook | Behavior |
|---|---|---|
| `cargo fmt --check` | pre-commit | block on diff |
| `cargo clippy -- -D warnings` | pre-commit | block on warning |
| `contrib/cyclo.sh --top 10` | pre-commit | report only |
| `mk coverage` | pre-push | report only |

Coverage moves to pre-push because `cargo llvm-cov` takes minutes;
running it on every commit would push DG to bypass hooks. Pre-push
keeps it on the path that touches origin.

## Implementation

1. `.githooks/pre-commit` and `.githooks/pre-push` in the repo,
   executable.
2. `mk install-hooks` target sets `git config core.hooksPath .githooks`.
   Document in AGENTS.md so new clones know to run it.
3. Hooks honor `SKIP_HOOKS=1` for the genuinely-need-to-bypass case;
   `git commit --no-verify` still works as a hard escape hatch.
4. Initial baseline: run `cargo fmt` once across all crates so the
   pre-commit hook doesn't reject HEAD. One large mechanical commit.

## Out of scope (deferred)

- Threshold enforcement for complexity and coverage — when DG sets
  numbers, add `--over <N>` to `contrib/cyclo.sh` and `--fail-under-lines
  <N>` to `mk coverage`, then call from `mk check`.
- Reducing complexity itself — that is P1.
- Per-rule tests for `validate.rs` — that is P7.

## Risk

A pre-commit hook that auto-runs `cargo fmt` (mutating) saves keystrokes
but can surprise people. Going with `--check` (refuse, don't mutate);
the failure message says `run: cargo fmt`.
