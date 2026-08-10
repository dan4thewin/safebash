---
title: "Intrinsics and lengths in arithmetic: `$(( (len a) + 1 ))`, `$(( ${#a[@]} + 1 ))`"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`int n=$(( (len a) + 1 ))` yields count+1 (currently silently 0 → 1) — the DD#58 arith sexpr `(len a)` must evaluate the intrinsic, not inline_int_func's no-body→0 path"
  - "`$(( ${#a[@]} + 1 ))` / `$(( ${#s} + 1 ))` yield length+1 (currently 0 inside arith)"
  - "int-returning intrinsics in arith: len, argc, epochseconds, dtoi, signum work; string intrinsics (toupper/join/basename/…) → a clean compile error in arith"
  - "`echo $(( <funccall> ))` works (currently rejects — arith result in a string/echo context)"
  - "0 native spec regressions; specs across native/c/janet"
---

# Intrinsics / lengths don't evaluate inside arithmetic

## Discovery

DG (2026-08-04), after choosing Option C for `$(int_func)` capture:
"definitely want `$(( (len a) + 1 ))` to work." Part 2 of that work (Part 1 —
the `$()`/`$(())` unification for int **functions** — shipped separately).

## The gaps (verified)

Inside a `$(( … ))` expression, int-valued **intrinsics** and **length**
forms read as 0:

| form | now | want |
|---|---|---|
| `int n=$(( (len a) + 1 ))` (a=(x y z)) | 1 (len→0) | 4 |
| `$(( ${#a[@]} + 1 ))` | 1 | 4 |
| `$(( ${#s} + 1 ))` | (0-based) | strlen+1 |
| `echo $(( (get_port) + 1 ))` | rejects | 8081 |

`int n=$(( (get_port) + 1 ))` (a **user** `-> int` fn) already works — user
int-funcs materialize in arith (Part 1). Only intrinsics/lengths and the
`echo $(( … ))` (arith-in-arg) context remain.

## Root cause

- `funccall_to_ssa` (`transform/expr_handlers.rs`) lowers the DD#58 sexpr
  `(len a)` to `IntFuncCall{len, …}`; `inline_int_func`
  (`transform/functions.rs`) finds no user body for `len` and returns
  `Expr("0")`. Fix: when the name is an int-returning intrinsic, evaluate it
  (mirror `try_native_intrinsic` / `try_len_intrinsic`) and materialize into
  a temp so the surrounding arith uses the value.
- `${#a[@]}` / `${#s}` (ParamExpansion arrlen/strlen) aren't materialized as
  arith operands either — same lift-into-temp treatment.
- `echo $(( … ))` with a funccall rejects — a separate arith-in-string-context
  gap.

## Related

- `length-operator-int-typing.md` (int-*typing* of `${#…}`) — sibling; this
  task is the arith *evaluation*.
- Part 1 (`$()`/`$(())` unification for int functions) shipped 2026-08-04.
