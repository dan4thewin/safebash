---
title: "capture-condition `if x=$(cmd)` shipped (DD#88); trio still deferred"
date: 2026-09-02
---

# Capture-condition: `if x=$(cmd)` / `while x=$(cmd)` (DD#88)

DG asked "what would it take to enable `if ! x=$(cmd) {`", clarifying: "the
actual exit status of the cmd is wanted, and the stdout need only survive if
the cmd succeeded." That narrowing turned the open-ended
`multi-assignment-and-capture-trio` decision into a small, shippable subset —
DG chose to implement it now (bounding the scope via a form).

## Semantics (DD#88)

In a boolean context, an assignment whose RHS is a single **external** `$(cmd)`
(or pipeline `$(a | b)`) is a **capture-condition**: the condition truth value
is the command's **actual exit status** (no abort), and its stdout is captured
into the target (tainted, bless before use). `$?` is available as the first
action of the handling branch (DD#55), so:

```
if x=$(/bin/curl -sf "$url") { bless x [[ -n $x ]]; use $x }
else { /bin/echo "fetch failed: $?" }        # $? = curl's real exit code
```

`!`, `else`, and `$?`-first-in-branch are identical to a plain command
condition — so the negated `if ! x=$(cmd) { … }` loses `$?`'s magnitude (0/1),
exactly like `if ! cmd`; the non-negated `… else { echo $? }` form carries the
real code. A truly-unused captured target is a dead variable (SB5050) — if you
don't need the stdout, use the non-capturing `if ! cmd`.

## Why it was small

The narrowed form is the union of two existing mechanisms: `x=$(- cmd)`
(capture + suppress abort + store status) and `while read x` (bind a var AND
yield a condition status). So it is mostly wiring:

- **SSA**: new sigil `SetTestFromStatus` (`test = $? == 0`), sibling of
  `NegateTestResult`.
- **transform** (`try_capture_condition`): a `CmdSubst` RHS in a condition →
  suppressed capture `Assign` + the sigil; a single-`Pipeline` `CmdSubstBody`
  → `capture_body_into` with `state.suppressed` bumped (inner pipeline
  unchecked, so it sets `$?` to the aggregate) + the sigil.
- **codegen**: native `emit_set_test_from_status`, c
  `__test_result = (__exit_status==0)`; `handle_regular_pipeline` now honors
  `state.suppressed` for its `checked` flag.
- **validate**: SB4025 carve-out via a structural signal — post-resolution an
  external head is an absolute path (`/`), functions/intrinsics stay bare — so
  it matches the transform without a function table. The capture-condition also
  registers as a `$?`-provider in the SB5012 state machine.

## Scope (DG-chosen)

External `$(cmd)` / `$(a|b)` pipeline; untyped only (no `int n=$(cmd)`); `while`
included; stdout left as-is on failure. Excluded (stay SB4025): user-function
capture `$(f)`, intrinsic `$(getenv X)`, concat `foo$(cmd)`, multi-statement
`$(a; b)`, literal RHS.

## `$?` clarification

DG corrected my initial framing: DD#55 already permits `$?` as the first action
of a handling branch, so there was **no new `$?` decision** — the
capture-condition just honors that rule. Verified: `if cmd { } else { echo $? }`
reads the real code; `$?` after a second command in the branch stays SB5012.

## Backends / tests

native + c fully; janet gated (`janet-capture-condition` filed — janet emits
from the IR, not the SSA sigil, and its `$(cmd)` capture is a stub). The
rejects are backend-independent (all modes). `spec/capture_cond.sh` 88.1–88.14.
3-mode sweep vs HEAD: native 0 regressions, c 0, janet 0 (+14/+14/+6).

## Files

- `sash/src/ssa.rs` — `BuiltinCmd::SetTestFromStatus`.
- `sash/src/codegen/dispatch.rs` — `emit_set_test_from_status` + arm.
- `sash/src/emit_c.rs` — c sigil arm.
- `sash/src/transform/stmt_handlers.rs` — `try_capture_condition`,
  `push_set_test_from_status`, `is_single_pipeline`; pipeline `checked` honors
  `state.suppressed`.
- `sash/src/validate.rs` — `is_capture_condition_rhs` (SB4025 carve-out),
  `is_capture_condition_stmt` ($?-provider).
- `spec/capture_cond.sh`, `notes/decisions/DD88-capture-condition.md`,
  `notes/tasks/janet-capture-condition.md`.
