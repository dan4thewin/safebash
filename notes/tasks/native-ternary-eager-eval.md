title: "native: arithmetic ternary eagerly evaluates both branches (side effects leak)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`int x=0; echo $(( x>0 ? 10/x : 42 ))` prints `42` with NO `division by 0` on stderr (matches bash and the c backend)"
  - "the untaken branch's side effects (div/mod-by-zero diagnostic, overflow trap) do not fire"
  - "oracle_tests / a spec pins native == bash for a side-effecting untaken ternary branch"

## RESOLVED 2026-08-27 (journal 193)

Fixed in the same session it was filed. `codegen/values.rs::lower_ternary_value`
(string) and the int-context `SsaValue::Ternary` arm (extracted to
`lower_ternary_int`) now lower with real control flow: `brif` on the condition
into then/else blocks, each lowering only its own branch, merging the result
through a block param (phi) — mirroring `codegen/arithmetic.rs`'s div-by-zero
merge. Both backends (native + c, journal 192) are now lazy and match bash.
Regression pin: spec coverage.sh cov.57. All gates met.

## Symptom

Native's arithmetic ternary is **eager** — it computes *both* branches, so a
div/mod-by-zero (or overflow) in the *untaken* branch still fires its runtime
diagnostic. bash (and, after journal 192, the c backend) short-circuit like
C's `?:`.

```
$ bash -c 'x=0; echo $(( x>0 ? 10/x : 42 ))'
42                        # lazy: 10/x never evaluated

$ ./sash -c 'int x=0; echo $(( x>0 ? 10/x : 42 ))' && ./a.out
bash: division by 0 (error token is "0")   # <-- native, spurious (stderr)
42
```

stdout matches (`42`), but native emits a stderr diagnostic bash never
produces → an **oracle divergence** for side-effecting untaken branches.

## Cause

`codegen/values.rs::lower_ternary_value` (~806-849) is straight-line
(cyclomatic = 1): it lowers `true_val` and `false_val` unconditionally and
selects between the two results (a `select`, not a branch). Any side-effecting
statement a branch emits (the div-by-zero check, the checked-arith overflow
trap) therefore runs regardless of the condition.

## Fix

Lower the ternary with real control flow, mirroring `if`: brif on the
condition into a then-block and else-block, each computing only its own branch,
merging at a block that takes the result as a block parameter (phi). The c
backend already does the lazy form (emit_c `emit_ternary_as_int_c`, an
if/else into a temp — journal 192); bring native to parity.

## Discovered

While coverage-grinding the ternary (journal 192). The c backend's ternary was
separately *broken* (empty output — no `lower_value_as_str` arm) and fixed in
the same session; this native laziness gap is the remaining half. spec
coverage.sh cov.49-56 deliberately use side-effect-free branches so they pass
on both backends until this is fixed.

## Sibling found and fixed (2026-09-25, journal 409)

Arithmetic `&&` / `||` had the **identical bug** and survived this task by
sitting in a different function. `emit_arith_op` dispatched them to
`emit_logical_and_or`, whose own doc comment said "Short-circuit-free
(operands are already evaluated)" — so `(( d != 0 && 10 / d > 1 ))` with
`d=0` aborted where bash prints `0`, in both backends.

Fixed the same way and at the same layer: intercept in the `ArithBinOp` arm
of `lower_value_as_int` and lower the right operand inside a branch
(`lower_logical_short_circuit` native, `emit_logical_as_int_c` for c).
Specs `31.129`-`31.136`.

**The lesson for anything else of this class**: C short-circuiting the
*expression* is not enough, because `lower_value_as_int` emits the guards as
**statements** ahead of it. Any future lazy construct has to lower the operand
inside the block, not just guard the operator.

Both backends' eager emitters were then unreachable and were deleted in favour
of `unreachable!` (DG's call). `lift_int_func_calls` still hoists function
calls out of both `Ternary` and `ArithBinOp` — that is **correct**, because
bash's command substitution inside arithmetic is eager too (verified).
