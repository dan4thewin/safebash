---
title: "EFFORT: janet backend parity (bundled, deferred)"
status: deferred
priority: P3
kind: effort
depends: []
gates:
  - "the bundled janet-primary tasks below are resumed together as one effort, not surfaced as individual priorities"
  - "each sub-task's own gates are met and its guarded specs un-skipped for janet"
---

# Janet backend parity — bundled effort

DG (2026-08-10): "all the janet-related tasks should bundle into a new
effort for later." The janet backend is the illustration/oracle-B backend;
its remaining gaps are not per-item priorities. They are grouped here and
deferred as one unit, to be picked up in a dedicated janet session.

## Bundled sub-tasks (janet-primary, deferred)

- `janet-getpid-class-intrinsics` (P3) — implement getenv/argc/getpid/
  getppid/getcwd/epochseconds/random natively in janet instead of exec'ing
  them; un-guard functions.sh 42.x, taint.sh getenv.
- `janet-predicate-function-gaps` (P3) — `!` in a condition lowers to
  `true` (emit_condition_inline `_ => "true"`); un-skip functions.sh 74.3;
  pre-existing function-spec failures (17.1, 18.1–18.3, 42.1, 48.2).
- `janet-scalar-zero-value` (P3) — typed-decl no-init zero value; bare
  known-but-unassigned scalar must not env-fallback (DD#53); un-guard
  undeclared.sh ud.5/ud.6/ud.11.

## Explicitly NOT bundled (dual-backend — flag to DG)

These touch the **C** oracle backend too, so they are oracle-parity work,
not purely janet. Left as separate tasks pending DG's call on whether they
belong here or with the C oracle / W3 abort-channel work:

- `c-janet-dd55-abort-channel` (partial) — DD#55 implicit-exit abort absent
  in BOTH c and janet (relates to blocked todo `w3-finding-cjanet`).
- `dd55-abort-channel-remaining` (ready)
- `c-janet-compound-pipeline-stage` (ready)
- `c-janet-fn-int-discard` (ready)

## Resume

When resumed: work the three bundled sub-tasks together, re-run the full
native/c/janet sweep, and un-skip the guarded specs listed in each. The
`janet-parity-gate` (done) tracks the janet pass/fail ratchet.
