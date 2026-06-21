---
title: "BUG — janet backend: (( )) used as an if/while condition always takes the true branch"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "janet: `if (( 7 < 3 )); then …; else …; fi` runs the else branch (and all comparison/false (( )) conditions branch correctly)"
  - "drop the in_modes native,c guard on spec arithmetic.sh 31.17/31.18"
---

# BUG — janet `(( ))` condition always true

## Resolution (2026-06-21) — done

Fixed end-to-end so janet `(( ))` conditions, while-loops, and counters
match native/c. Three coupled fixes in `emit_janet.rs`:

1. **condition** — `emit_condition_inline` gained a `StmtKind::ArithTest`
   arm: `(not= 0 <arith>)` (was falling to `_ => "true"`).
2. **1/0 integers** — `arith_binop_to_janet` wraps comparisons
   (`(if (< a b) 1 0)`) and logicals (`(if (and (not= 0 a) (not= 0 b)) 1 0)`)
   so arith yields bash-style 1/0 ints (janet comparisons are bools and
   janet treats 0 as truthy). Fixes `echo $(( a < b ))` too.
3. **int assignment** — `emit_assignment` keeps an int-typed target numeric
   (`(set i (+ i 1))`, not string-wrapped) so `(( i++ ))` counters work; and
   **string `[[ ]]` tests stringify int operands** (`emit_test_string_operand`)
   so `[[ $i != 0 ]]` with an int `i` compares "0" to "0" (else it loops
   forever — int 0 ≠ string "0").

Un-guarded arithmetic.sh 31.17/31.18 and argv.sh av.h3. Spec: janet
−8 failing (cf.w1–w7 while-loops, cf.i3) with zero regressions in any
mode. `_stmt_uses_args` ArithTest detection restored (refactored to use
`direct_exprs`/`child_bodies`, well under the complexity gate).

## Symptom (janet backend only)

```
if (( 7 < 3 )); then echo then; else echo else; fi   # janet prints "then"
```

All `(( ))` comparison conditions (`<`, `<=`, `>`, `==`, …) take the
`then`/loop branch even when false. native and c are correct.

## Likely cause

The janet backend (`emit_janet.rs`) evaluates a `(( ))` condition by
value-truthiness and/or doesn't thread the comparison result into the
branch the way native does (native: `LetExit` → `test_result_var` →
`Terminator::Branch`). A janet comparison yields `true`/`false` (or the
arith value), and the if/while lowering likely isn't testing it as a
0/non-0 status. Related to `bug-andor-backend-chain-status` (janet runs
`&&`/`||` chains by value-truthiness, not exit status) — same family.

Also note `echo $(( a < b ))` prints empty in janet (comparison in value
context), which may share a root.

## Discovery

Found 2026-06-20 adding `<=`/`>=` regression spec tests (arithmetic.sh
31.15–31.18) after fixing the `(( <= ))`/`(( >= ))` mis-lex bug. The
parse fix is correct (native + c pass all four; janet passes the
true-branch cases 31.15/31.16). 31.17/31.18 (false branch) are guarded
`in_modes native,c` pending this fix.

## Root cause (confirmed 2026-06-21)

`emit_condition_inline` (emit_janet.rs) only handles a `[[ … ]]` test
command and a generic `Command` (→ `(zero? (os/execute …))`). Every
other condition statement — including `StmtKind::ArithTest` (the `(( ))`
form) — falls to `_ => write!(out, "true")`. So `if (( … ))` always
emits `(if true …)`, unconditionally taking the then/loop branch.

The proper fix is two-part, because janet's truthiness differs from
bash's:

1. Add a `StmtKind::ArithTest(a)` arm to `emit_condition_inline` that
   emits a janet boolean for "arith result is non-zero".
2. janet comparison ops currently yield janet **bools** (`(< a b)` →
   `true`/`false`) and `0` is **truthy** in janet — so neither
   `(not= 0 X)` (wrong for bool X) nor bare truthiness (wrong for int 0)
   works uniformly. Make arith comparisons/logicals yield bash-style
   1/0 **integers** (e.g. `(if (< a b) 1 0)`, and `&&`/`||` via
   `(not= 0 …)` operands), then the condition is simply `(not= 0 X)`.
   This also fixes `echo $(( a < b ))` printing empty and the
   `bug-andor-backend-chain-status` family.

Also un-guard spec av.h3 (argv.sh) once fixed.

NB: when fixing this, also restore positional detection for bare/condition
`(( ))` in the janet arg-scan — add a `StmtKind::ArithTest(a) => _arith_uses_args(a)`
arm to `_stmt_uses_args` (reverted 2026-06-21 only to keep that pre-existing
cc=21 dispatcher grandfathered; the assignment path via `_expr_uses_args`
already covers `x=$(( $# ))`). Reduce `_stmt_uses_args` complexity at the
same time.
