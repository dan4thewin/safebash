---
title: "ICE: a user-function capture `$(f)` as a `[[ ]]` test operand"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`fn f { echo x; }` + `if [[ $(f) = x ]]; then …` compiles (or is rejected with a diagnostic) — never panics"
  - "same for an `-> int` function: `if [[ $(f) = 1 ]]`"
  - "the equivalent outside a test (`v=$(f)`) keeps working"
---

## Found

2026-09-08, while probing whether `&&`/`||`/`!` behave identically inside and
outside `[[ ]]` (journal 254). Both spellings panicked:

```
fn side { echo SIDE; }
if [[ $(side) = SIDE ]]; then echo T; fi
```

```
thread 'main' panicked at sash/src/codegen/values.rs:81:13:
ICE: FuncCall should have been lifted during transform: side
```

An `-> int` function hits the sibling arm at `values.rs:84`
(`IntFuncCall should have been lifted during transform`).

**Pre-existing**, not from the DD#93/DD#94/DD#74 work: `codegen/values.rs` and
`transform/expressions.rs` were both last touched 2026-09-04, and the ICE
reproduces at that state.

## Scope

| form | result |
|---|---|
| `v=$(f)` (assignment) | ok |
| `if [[ $(f) = x ]]` (test operand) | **ICE** |
| `if [[ $(f) = 1 ]]`, `f` is `-> int` | **ICE** (values.rs:84) |
| `if [[ $(printf x) = x ]]` (external) | clean SB5020 (tainted) |

So it is specifically a **user-function** capture in a test operand; the
external case is caught by taint first, which is why this has gone unnoticed.

## Cause

`values.rs:77` states the invariant:

> DD#81: a `$(f)` on a defined-so-far function is always lifted to a captured
> temp during transform (a forward/undefined name resolves external instead),
> so a bare FuncCall never reaches codegen.

The lifting happens on the assignment and command paths
(`lift_int_func_calls` / `lift_int_func_args`). The **test** path does not do
it: `test_to_ssa` → `expr_to_value` yields a `FuncCall`/`IntFuncCall` straight
into `SsaTest`, and nothing lifts it before codegen. The invariant is real; the
test path just never upheld it.

## Fix sketch

Lift captures out of test operands the way the other paths do — in
`test_to_ssa` (`transform/expressions.rs`), or in the callers that build an
`SsaTest`, hoisting each `FuncCall`/`IntFuncCall` operand into a captured temp
first and referencing the temp in the test. Note `test_to_ssa` currently takes
`&VersionState` and returns an `SsaTest` with no block to emit into, so the
lift has to happen at the call sites (which do have `ssa`/`block`), or the
signature has to grow one — `push_attest_arith_convert` in `stmt_handlers.rs`
is a recent example of the latter shape.

Whatever the fix, a capture in a test is also a **taint** question: an external
`$(…)` there is SB5020, and a user function's stdout should presumably follow
the same rule as `v=$(f)` (DD#24 user-function capture is clean).

## Guard

Add specs for both the string and `-> int` function cases, and keep the
external-cmdsub SB5020 case alongside them so the two paths stay distinguished.

## Resolved

2026-09-08 (journal 257). Fixed by `lift_test_captures`
(`transform/functions.rs`), applied at the one site that builds an `SsaTest`
with a block in hand (`try_test_arg_capture`). It reuses `lift_int_func_calls`,
so all three kinds — `FuncCall`, `IntFuncCall` and `CmdSubstBody` — are hoisted
into a captured temp exactly as the assignment path does.

DG's call on the open question in the gates: **make it work**, consistent with
`v=$(f)`, rather than reject.

Specs `functions.sh` ice.1–ice.6 (string fn, `-> int` fn, multi-statement body,
unary operand, nested inside `&&`, and a `while` condition where the capture
re-evaluates each iteration). Native and c pass; janet fails on a *separate*
pre-existing defect, filed as `janet-command-substitution-capture-broken`.
