title: "sash-rt-tests overflow_tests: itoa_min_i64_panics is stale (i64::MIN no longer panics)"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "overflow_tests.rs `itoa_min_i64_panics` reflects current behavior — sash_itoa(i64::MIN) formats correctly (via unsigned_abs, journal 120) rather than panicking"
  - "sash-rt-tests overflow_tests suite green"

## Symptom

`sash-rt-tests/tests/overflow_tests.rs::itoa_min_i64_panics` is `#[should_panic]`
but `sash_itoa(i64::MIN)` no longer panics — the runtime arith-overflow fix
(journal 120) formats `i64::MIN` via `unsigned_abs()` instead of trapping. So
the test "did not panic as expected" and fails.

Surfaced 2026-08-21 while restoring unit_tests.rs (which hadn't compiled since
ae9a9fa, masking that overflow_tests also runs). Pre-existing, unrelated to the
strip-runtime removal that surfaced it.

## Fix

Replace the `#[should_panic]` expectation with an assertion that
`sash_itoa(i64::MIN)` returns "-9223372036854775808" (the correct string).
Verify the rest of overflow_tests reflects the current trap/format policy.
