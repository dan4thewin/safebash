---
title: "audit / prune / rehabilitate the Rust test suites (sash/tests/) — significant bit-rot"
status: deferred
priority: P2
kind: chore
depends: []
gates:
  - "every Rust test in sash/tests/ either passes deterministically or is deleted / rewritten — no test left in a known-failing state"
  - "the Rust suites no longer depend on shared in-process global compiler state (ARITH_MAP, SYSROOT, squashfs cache) leaking between test calls — full-run results match isolated-run results"
  - "tests that assert behavior we have deliberately dropped (heredocs, `echo -e`, `declare`, `let`, …) are removed, not left failing"
  - "`sash-rt-tests` links and runs (drop dead `sash_streq` / `sash_arith_eval` externs) and is added to `mk test` so the extern-`C` runtime unit tests can't rot unnoticed"
---

# Audit / prune / rehabilitate the Rust test suites

> **DEFERRED to the end (DG, 2026-07-07).** This is the LAST task — pick it
> up only once the language sweep and the c/janet backend work have settled,
> so the suites are rehabilitated against a stable surface instead of being
> re-fixed as behavior keeps changing. The concrete plan below (the
> frontmatter **gates** and the **Approach** section) is **provisional**:
> when this is finally started, first re-inventory the failures fresh, then
> **revisit the specific actions with DG** — what to delete vs. rewrite,
> whether/how to re-enable `mk test` as a gate, the global-state-leak fix,
> and the `sash-rt-tests` rehab — rather than executing the plan recorded
> here as-is. Nothing here is ratified until that revisit.

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

## Absorbed detail (consolidated 2026-07-06)

This task is the single canonical Rust-test-rot item; it absorbed
`rust-test-suite-rot` and `stale-rust-test-suites` (both now superseded).
Their unique findings:

### Additional bit-rot classes (from rust-test-suite-rot)
- **Stale diagnostic codes.** Many e2e / taint_spec tests
  `expect_diag("SB4020")`, but the taint reject code is now **SB5020**
  (SB4020 was renamed). Mechanical migration. e.g. `neg_tainted_argv`,
  `neg_tainted_all_args`, and the taint_spec rejection block.
- **Output mismatches need per-case triage.** Some failures are arithmetic
  / output mismatches (e.g. `arith_cmd_compound_assign` expects 12, gets
  15) that could be **real codegen bugs**, not stale expectations — bug
  policy applies; do not delete without investigating.
- **Parallel flakiness.** `codegen_tests` share a global `COUNTER` and
  `/tmp/sash_032_*` paths; run `--test-threads=1` for a stable count (a
  facet of the global-state leak in bucket 1 above).

### `sash-rt-tests` link breakage (from stale-rust-test-suites)
- `sash-rt-tests/tests/unit_tests.rs` declares `extern` `sash_streq` and
  `sash_arith_eval`, which **no longer exist** in `sash-rt/src/lib.rs`, so
  the test binary fails to link. The harness is a workspace member but
  **not** on `mk test`'s path, so it rotted unnoticed. It is the only place
  that unit-tests the `extern "C"` runtime in isolation (the DD#63
  `sash_regex_match_into` / backref tests were written against it), so it's
  worth rehabilitating and wiring into `mk test`.

### Failure counts drift across commits — re-inventory at fix time
Snapshots (single-threaded, illustrative, not a target):
- 2026-06-16 `59d7123`: codegen 186/71, e2e 93/31, oracle 45/18,
  taint_spec 17/25, janet 24/11, brush 25/27, c 62/5 — ~158 total.
- 2026-06-25 `5498ca3`: codegen 202/54, e2e 101/23, oracle 51/12,
  janet 26/9, taint_spec 18/24; `sash-rt-tests` won't link; brush not
  installed.
The set shifts as the compiler changes, so inventory fresh when the work
starts rather than trusting these numbers.

- brush_compat failures (bash-compat the subset deliberately rejects) stay
  tracked separately by `brush-rerun`.

### Feature-removal debt to sweep at fix time (tests of dropped constructs)
As language surface is removed, benched Rust tests that exercised the dropped
form now fail (they compile a construct sash rejects). Convert to the sash
idiom or make them expect the reject. Known:
- **here-strings `<<<`** (removed by DD#83, → SB4050): `codegen_tests.rs:2078`
  (`cat <<< "hello world"`), `e2e_tests.rs:840,943` (`read x <<< …`),
  `taint_spec_tests.rs:329,456` (`read <<< …`), `brush_compat_tests.rs:467`
  (`while read … done <<<"$content"`). Rewrite to the pipe idiom
  (`echo $x | read v`, `printf … | cmd`, `cmd | while read`) — the spec suite
  already covers the pipe forms (taint.sh 24.6).
- the 5 true/false body-semantics tests noted earlier (explicit-`return` idiom).

## Measured 2026-09-10 (backlog prune)

Ran every suite at HEAD. The "~158 failures" estimate is about right, and
there is a worse problem underneath it.

| suite | result |
|---|---|
| `codegen_tests` | **HANGS** — killed at 600s |
| `e2e_tests` | 76 passed, **54 failed** |
| `taint_spec_tests` | 16 passed, **26 failed** |
| `brush_compat_tests` | 24 passed, **28 failed** |
| `oracle_tests` | 46 passed, **17 failed** |
| `janet_tests` | 19 passed, **16 failed**, 3 ignored |
| | **141 failures** + a hanging suite |

### The hang is the same bug I fixed in the spec harness

`codegen_tests` stalls on `read_delim_colon`, `test_read_multi_vars` and
`test_read_three_vars` — all **read** tests using spellings DD#99 retired. The
runner (`tests/codegen_tests.rs:31`) is:

```rust
let output = cmd.output().expect("run");
```

No `Stdio::null()` on stdin, so a binary that waits on stdin blocks the whole
suite — exactly the defect fixed in `spec/common.sh` on 2026-09-09 (commit
e2aed41), where the fix was `</dev/null` plus a `timeout` on every child.

**Fix that first.** Until the runner cannot hang, the suite cannot even report
what is wrong with it, and `cargo test` is unusable as a gate.

### Context

`spec/` (4599 passing, 0 regressions) is the authoritative suite and is
healthy; AGENTS.md already records the cargo suites as stale. The question
this task should answer is not "make 141 tests pass" but **which of these
suites still earn their place** now that spec/ covers the same ground — some
assert behaviour DD#99, DD#86 and DD#91 deliberately changed, so a failure
there is the test being wrong, not the compiler.
