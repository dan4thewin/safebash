---
title: "Intrinsics and lengths in arithmetic: `$(( (len a) + 1 ))`, `$(( ${#a} + 1 ))`"
status: partial
priority: P2
kind: bug
depends: []
gates:
  - "[done] `int n=$(( (len a) + 1 ))` yields count+1 — the DD#58 arith sexpr `(len a)` evaluates the intrinsic (len array/map→count, string→bytes)"
  - "[done] `$(( ${#a} + 1 ))` / `$(( ${#s} + 1 ))` yield length+1 (arith-braced-length: ATok::LenOf → the len FuncCall shape)"
  - "[deferred] string intrinsics (toupper/join/basename/…) in arith → a clean compile error"
  - "[was already ok] `echo $(( <funccall> ))` compiles (arith-in-arg)"
  - "[done] 0 native/c spec regressions (janet len-in-arith guarded — janet-join-intrinsic-gaps)"
---

# Intrinsics / lengths don't evaluate inside arithmetic

## Resolution status (2026-08-14, unattended — partial)

**Gate 1 done (the DG demo case):** `(len a)` as a `$(( ))` sexpr now
evaluates. `funccall_to_ssa` (`transform/expr_handlers.rs`) intercepts an
int-returning intrinsic (`is_arith_int_intrinsic` — `len` today),
reconstructs a `Command` from the sexpr args
(`reconstruct_intrinsic_command`) and reuses `try_native_intrinsic` →
the `arrlen`/`strlen` `ParamExpansion`, instead of the user-int-func path
(`inline_int_func`) that returned 0. `(len a)`→count, `(len s)`→bytes,
`(len m)`→entries; nested `(len a)+(len b)` and user `-> int` fns still
work. Specs `spec/arithmetic.sh` 58.10–58.13, green in native/c/janet.

**Deferred (own task `arith-braced-length`):** `$(( ${#a} + 1 ))` and
`$(( ${#s} + 1 ))` still read 0 — a **parse-level** gap: the arith lexer
`scan_arith_braced_var` (`parse/arith.rs`) reads `${#a}` as a variable
named `#a`. Fixing needs a length-aware arith token → `ArithExpr` →
`arith_to_ssa` producing the `arrlen`/`strlen` `ParamExpansion` (dispatch
on `state.collections`, like `try_len_intrinsic`). Multi-layer; deferred
from the unattended pass.

**Deferred:** a string intrinsic (`toupper`/`join`/…) in an arith sexpr
should be a clean compile error (today it flows to `IntFuncCall`→0). Add
to `is_arith_int_intrinsic`'s siblings — a reject arm for known
string intrinsics.

## Original notes


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
