## 214 — spec measurement + reporting robustness (plan counts, test_status summary)

**On `hand-parser`.** DG fixed the coverage measurement; I fixed the reporting.

### Background (the coverage-measurement bug this fixes)

My grind measurement was unreliable: the instrumented `sash` runs each spec in a
`mktemp -d` cwd that's `rm`'d on exit (common.sh `setup`), so its profraw was
lost — the never-run list reflected only the Rust tests, hugely over-reporting
"never-run." I nearly deleted live code (`scan_radix_literal`); an `eprintln`
probe caught it (it's hit by `$((0xff))`, tested by `arith_literals.sh` nl.x1).

### DG's fix (measurement)

- `mkfile`: split the coverage targets — `cov-clean` / `cov-unit` / `cov-spec` /
  `cov-rpt` / `cov-full` / `cov-spec-only`. `cov-spec` passes the instrumented
  binary explicitly via `SASH="…/llvm-cov-target/release/sash"` and an absolute
  `LLVM_PROFILE_FILE`.
- `common.sh`: `run_sash` now invokes `${SASH:-sash}` so the coverage harness
  runs the instrumented compiler (whose profraw is captured) rather than a
  PATH-resolved `sash`.

### My fix (reporting robustness — DG-directed "fix the plan counts and make the
report of spec success more robust")

- **run.sh summary** now reads the **`test_status`** view (latest run per
  `test_id`), not `count(*) … WHERE commit_hash=$sha`. The old query inflated
  permanently: every run appends a row per test, so one broken/partial run left
  ~1200 stale `failing` rows that persisted even after later runs passed them
  (I saw `native|failing|1217` while the real count was 0). It also lists the
  currently-failing test ids (deduped, actionable).
- **common.sh `tally`** is now fail-based: only an actual `not ok` FAILs a run.
  A `plan != index` mismatch is a visible NOTE (`[plan=N, ran=M]`), not a
  failure — `plan` legitimately diverges per mode (`in_modes` guards) and drifts
  when checks are added, and the per-test spec.db records are the source of
  truth. Stops stale plans masquerading as FAILED.
- **Plan counts corrected**: compiler.sh 9→12, operand_words.sh 10→13,
  wordsplit.sh 70→72 (stale — checks added without bumping); errors.sh balanced
  (`skip_chk 55.30/55.31` added so c/janet match native's 91, the two
  `in_modes native` checks now skip explicitly in other modes).

### Note (environment)

`native:54.1` (misc.sh) needs the `sash-expand` binary on PATH; it wasn't built
in this environment, so `$(which sash-expand)` was empty and the config path ran
as a bogus command (rc 127). Built `sash-expand` → 54.1 green. This was an
environment gap, **not** a code regression.

### Verification

Full 3-mode sweep: **0 regressions** native/c/janet vs HEAD. The robust summary
now reports the truth — native 0 failing / c 0 failing / janet 36 failing
(pre-existing baseline). Affected spec files show clean PASSED in every mode
(errors.sh janet correctly FAILs on its 3 pre-existing baseline failures).
