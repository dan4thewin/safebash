---
title: "Arith `${#a}` / `${#s}` read as 0 — parse-level length in $(( ))"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`$(( ${#a} + 1 ))` yields the array element count + 1 (a=(x y z) → 4)"
  - "`$(( ${#s} + 1 ))` yields the string byte length + 1"
  - "`$(( ${#m} ))` yields the map entry count"
  - "0 native/c/janet spec regressions; specs in arithmetic.sh"
---

# `${#a}` / `${#s}` don't evaluate inside `$(( ))`

Split from `intrinsics-lengths-in-arithmetic` (2026-08-14). The sexpr form
`$(( (len a) + 1 ))` was fixed there (gate 1); this is the **braced-length**
form, which is a distinct **parse-level** gap.

## Root cause

The arith lexer `scan_arith_braced_var` (`sash/src/parse/arith.rs`) reads the
whole `${…}` body as a variable name — so `${#a}` becomes `DollarVar("#a")`,
which `arith_to_ssa` looks up as an (undeclared) variable → `0`. The `#`
length prefix is never recognized.

Verified: `$(( ${#a} ))` = 0, `$(( ${#s} ))` = 0 (should be count / byte len).

## Fix sketch

- Arith lexer: in `scan_arith_braced_var`, detect a leading `#` → emit a
  length token carrying the inner name (e.g. `ATok::LenOf("a")`), not a
  `DollarVar`.
- `ArithExpr`: add a variant (or reuse `FuncCall("len", [Var(name)])` to
  funnel through the just-added `funccall_to_ssa` int-intrinsic path).
- `arith_to_ssa`: produce the `arrlen`/`strlen` `ParamExpansion` dispatched
  on `state.collections` (mirror `try_len_intrinsic`).

Reusing the `FuncCall("len", …)` funnel is likely the smallest change — the
lexer rewrites `${#a}` into the same shape the `(len a)` sexpr already takes,
so the transform side needs no new arm.

## Related

- `intrinsics-lengths-in-arithmetic` (gate 1 — the `(len a)` sexpr — done).
- DD#75 §2 (`${#x}` is Int sugar for `$(len x)`; the non-arith typing is done).
