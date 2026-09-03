---
title: DD#88 — Capture-condition: `if x=$(cmd)` binds stdout and branches on the command's status
status: accepted
---

Settled with DG (2026-09-02) and implemented the same session. A scoped,
shipped subset of the `multi-assignment-and-capture-trio` decision — enough to
make the long-standing `if ! out=$(cmd) { … }` idiom work — deferring the
general multi-value `return` / `(stdout, status, stderr)` trio to that task.

## The gap

Capturing a command's stdout **and** branching on its exit status had no
working form (see the `multi-assignment-and-capture-trio` motivation table):
`if ! out=$(cmd)` was SB4025; the "sequence it" workaround aborts on failure;
DD#55's `out=$(cmd) || …` idiom was itself SB4025. Root cause: DD#71 treats an
assignment as a value binding with "no meaningful exit status" — true for
`x=foo`, but `x=$(cmd)` carries the command's status, which the idiom needs.

## The decision

In a **boolean context** (an `if`/`while`/`until` condition, or the operand of
`!`), an assignment whose RHS is a single **external** command substitution —
`x=$(/path/cmd)` or a pipeline `x=$(a | b)` — is a **capture-condition**:

- the **condition truth value is the command's actual exit status** (`test =
  status == 0`); a nonzero exit does **not** abort (the status is consumed —
  DD#55's "binding the status" rule);
- the command's **stdout is captured into the target** (tainted external
  output — bless before use, DD#61);
- `$?` is available as the **first expression of the handling branch** (the
  existing DD#55 handling-position rule), so `if x=$(cmd) { … } else { echo $? }`
  reads the real failing code in the else branch;
- `!`, `else`, and `$?`-first-in-branch behave **identically to a plain command
  condition** — in particular the negated spelling `if ! x=$(cmd) { … }`
  replaces `$?` with the negated boolean (0/1), losing the magnitude, exactly
  like `if ! cmd`. To read the numeric code in-branch, use the non-negated
  `if x=$(cmd) { use $x } else { echo "failed: $?" }` form.

### Scope (DG-chosen, 2026-09-02)

- **RHS shape:** a single external `$(cmd)` OR a pipeline `$(a | b)` of external
  stages. Excluded (stay SB4025): a user-function capture `$(f)` (functions
  convey status via `return` — DD#74 predicates), an intrinsic `$(getenv X)`, a
  concatenation `foo$(cmd)`, a multi-statement `$(a; b)`, and any literal RHS.
- **Untyped only:** `if int n=$(cmd)` (typed declaration) is NOT a
  capture-condition — declarations stay SB4025.
- **`while`/`until` included:** the same form works as a loop condition.
- **stdout on failure:** left as-is (whatever partial bytes were captured); the
  target is meant to be used on success. A truly-unused captured target is a
  dead variable (SB5050) — if you do not need the stdout, use the
  non-capturing `if ! cmd { … }` form.
- **`$?` after:** no new rule — the existing DD#55 handling-position machinery
  applies (a capture-condition assignment provides `$?` like a command).

## Why this shape

The narrowed semantics are the union of two mechanisms that already exist, so
the implementation is mostly wiring, not new machinery:

1. `x=$(- cmd)` — captures stdout, suppresses the abort, stores the status.
2. `while read x` — an operation that binds a variable AND yields a condition
   truth value.

## Implementation

- **transform** (`transform/stmt_handlers.rs::try_capture_condition`): in
  `state.in_condition > 0`, a `CmdSubst` (single external) RHS lowers to a
  **suppressed** capture `Assign` + a `SetTestFromStatus` sigil; a
  `CmdSubstBody` that is a single `Pipeline` captures via `capture_body_into`
  with `state.suppressed` bumped (inner pipeline unchecked) + the sigil.
- **SSA**: new sigil `BuiltinCmd::SetTestFromStatus` (`__set_test_from_status`)
  — `test_result = ($? == 0)`, the sibling of `NegateTestResult`. A suppressed
  pipeline sets `test_result` itself; the sigil makes the single-command path
  uniform.
- **codegen**: native (`emit_set_test_from_status`) and c (`__test_result =
  (__exit_status == 0) ? 1 : 0`). Pipelines honor `state.suppressed` for their
  `checked` flag (`handle_regular_pipeline`).
- **validate** (`is_capture_condition_rhs`): the SB4025 carve-out uses a
  structural signal — post-resolution an external command's head is an absolute
  path (`/`), while functions / intrinsics / builtins stay bare — so it matches
  the transform's `CmdSubst`/pipeline shape without a function table. The
  capture-condition also registers as a `$?`-provider in the SB5012 state
  machine.
- **janet**: not implemented — janet emits from the IR (not the SSA sigils) and
  its `$(cmd)` capture is a stub. Positive-behavior specs are gated `native,c`;
  filed `janet-capture-condition`. The rejects (SB4025/SB5050/SB5020) are
  backend-independent and pass in all modes.

## SB codes

No new codes. Reuses SB4025 (non-capture value binding in boolean context),
SB5050 (dead captured target), SB5020 (tainted target used without bless),
SB5012 (`$?` outside a handling position).

## Tests

`spec/capture_cond.sh` 88.1–88.14: success/failure branches, `$?`-in-else real
code, negated form, `$?`-first-in-then, pipeline + pipefail status, `while`
form, taint bless (SB5020), dead-var (SB5050), and the exclusions (user-fn /
concat / intrinsic / literal → SB4025). journal 236.

## Deferred

The general feature — multiple-value `return`, multi-target binding, and the
external `(stdout, status, stderr)` trio — remains in
`multi-assignment-and-capture-trio`. This DD is the external-command
capture-condition subset only.
