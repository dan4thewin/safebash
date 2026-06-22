---
title: "audit / prune / rehabilitate the Rust test suites (sash/tests/) — significant bit-rot"
status: ready
priority: P2
kind: chore
depends: []
gates:
  - "every Rust test in sash/tests/ either passes deterministically or is deleted / rewritten — no test left in a known-failing state"
  - "the Rust suites no longer depend on shared in-process global compiler state (ARITH_MAP, SYSROOT, squashfs cache) leaking between test calls — full-run results match isolated-run results"
  - "tests that assert behavior we have deliberately dropped (heredocs, `echo -e`, `declare`, `let`, …) are removed, not left failing"
---

# Audit / prune / rehabilitate the Rust test suites

DG (2026-06-21): "the spec tests are the current source of truth, the other
tests seem to have significant bit-rot due to substantial code changes —
stop running them and file a task to audit/prune/rehabilitate them."

Until this lands, **the spec suite (`cd spec && bash run.sh`, `spec.db`
`test_status` view) is the only regression gate.** Do NOT use `mk test` /
`cargo test` to judge a change — the Rust suites produce misleading
failures.

## What's wrong

1. **Non-determinism from shared global state.** `codegen_tests` call
   `sash::codegen::compile_to_object()` **in-process**, which re-parses and
   runs the pipeline but bypasses `cli_main`'s front-end. The compiler holds
   mutable global state (DD#58 `ARITH_MAP`, `SYSROOT` / `SQUASHFS_CACHE`
   mutexes) that leaks between test calls, so the failing SET differs
   between a full run and isolated runs (verified: `pipeline_two_stage`,
   `cmdsub_with_args`, etc. fail in isolation but "pass" in some full runs).
   Either reset the globals per-compile or thread them through explicitly.

2. **Bit-rot: tests assert dropped behavior.** Many failures are tests for
   features deliberately removed in the hand-parser migration / DD sweep:
   heredocs (`<<` / `<<-`, dropped — `__HEREDOC__` → SB5002), `echo -e`
   (no escape interpretation), `declare`/`export` (SB5001), `let` (review).
   These should be deleted or rewritten to assert the *rejection*, not the
   old behavior.

3. **Coverage divergence from the binary.** Suites split between calling
   `compile_to_object()` directly (codegen) and shelling out to the `sash`
   binary (brush, e2e). The direct-call path skips the whole front-end
   (validate, taint, **DD#23 resolution**), so it can't catch front-end
   regressions. Decide a single canonical compile entry point for tests.

## Approach (suggested — DG to ratify scope)

- Inventory every `sash/tests/*.rs` failure; bucket as {dropped-feature,
  global-state-flake, real-gap}.
- Delete dropped-feature tests (or convert to rejection assertions).
- Fix the global-state leak (reset between compiles) so the suite is
  deterministic.
- Re-point in-process tests at a full-pipeline entry so they exercise the
  front-end.
- Re-enable as a gate only once green + deterministic.

## Notes
- This is the reason `notes/status.md` / the SOP gate on the spec suite.
- Counts at filing: native spec 2-fail baseline held; Rust suites ~150
  failing, dominated by the three buckets above.
