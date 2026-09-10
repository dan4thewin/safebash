---
title: Rust test suite rot — ~158 pre-existing failures at HEAD
status: done
superseded-by: audit-rust-test-suites
priority: P1
kind: bug
depends: []
gates:
  - mk test (cargo test --release in sash/) is green, OR
  - stale tests are migrated/removed and the remaining set passes
---

> **SUPERSEDED 2026-07-06** — consolidated into
> `notes/tasks/audit-rust-test-suites.md` (the canonical Rust-test-rot
> task). The unique findings here (SB4020→SB5020 stale codes, output-
> mismatch per-case triage, the per-suite count table, parallel flakiness)
> were folded into that task's "Absorbed detail" section. Kept for history.

# Rust test suite rot — pre-existing failures at HEAD

## Discovery (2026-06-16, during argv-trio)

`cargo test --release` (== `mk test`) in `sash/` is heavily red at HEAD
(a206fec), independent of the argv-trio work. Stable (single-threaded)
counts:

| suite              | passing | failing |
|--------------------|--------:|--------:|
| codegen_tests      |     186 |      71 |
| e2e_tests          |      93 |      31 |
| oracle_tests       |      45 |      18 |
| taint_spec_tests   |      17 |      25 |
| janet_tests        |      24 |      11 |
| brush_compat_tests |      25 |      27 |
| c_tests            |      62 |       5 |

~158 failures total. The argv-trio added **zero** net regressions
(codegen 72→71, e2e 31→31 after migrating the 8 flipped argv tests).

## Root causes (sampled)

- **Outdated syntax**: tests use `declare -a` / `declare -A`, rejected by
  SB5001 (DD#45). e.g. `array_append`, `array_length`, `assoc_array_*`.
- **Stale diagnostic codes**: many e2e/taint_spec tests
  `expect_diag("SB4020")` but the taint code is now **SB5020**. SB4020 is
  an old/renamed code. e.g. `neg_tainted_argv`, `neg_tainted_all_args`,
  and the taint_spec rejection block.
- **Old semantics**: arithmetic / output mismatches (e.g.
  `arith_cmd_compound_assign` expects 12, gets 15) — needs per-case
  triage (real codegen bug vs stale expectation).
- **brush_compat**: 27 fail because the brush corpus asserts bash-compat
  behaviour sash deliberately rejects ($HOME → SB5018, etc.). Tracked
  separately by `brush-rerun`.

## Note on flakiness

`codegen_tests` share a global `COUNTER` and `/tmp/sash_032_*` paths;
under parallel execution a few tests (`assoc_array_basic`,
`indexed_array_loop`) flip intermittently. Run with `--test-threads=1`
for a stable count.

## Proposed approach

1. Triage by class (declare-syntax, SB4020→SB5020, output-mismatch).
2. Mechanical migrations for the first two classes.
3. Per-case investigation for output mismatches (some may be real bugs —
   bug policy applies).
4. Consider whether the rust suite or the spec suite (`spec/*.sh`,
   `spec.db`) is the canonical gate; the spec suite is healthy (native
   clean) and is the de-facto gate today.

## See also
- `Makefile`/`mkfile` `test:V:` target
- `notes/tasks/brush-rerun.md`

## Closed 2026-09-10 (backlog prune)

Bookkeeping only — closed 2026-09-10. Successor `audit-rust-test-suites` is live.
