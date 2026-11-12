title: "Arith `${#a}` / `${#s}` read as 0 — parse-level length in $(( ))"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`$(( ${#a} + 1 ))` yields the array element count + 1 (a=(x y z) → 4)"
  - "`$(( ${#s} + 1 ))` yields the string byte length + 1"
  - "`$(( ${#m} ))` yields the map entry count"
  - "0 native/c/janet spec regressions; specs in arithmetic.sh"

## Resolution (2026-08-14, unattended)

Done. Added `ATok::LenOf(name)`: the arith lexer `scan_arith_braced_var`
(`parse/arith.rs`) detects `${#name}` (a `#` prefix + identifier) and emits
`LenOf`; the parser's `parse_prefix_atom` lowers it to
`ArithExpr::FuncCall { name: "len", args: [Var(name)] }` — the same shape the
`(len a)` sexpr takes — so the transform's int-intrinsic path (just added)
evaluates it (arrlen for a collection, strlen for a scalar). No new transform
arm. `${#a}`→count, `${#s}`→bytes, `${#m}`→entries. `${#}`/`$#` (empty name)
fall through to the ordinary var scan (argc), unaffected; `${x}` braced var
unaffected.

Specs `spec/arithmetic.sh` 58.14–58.16 (native,c; janet guarded — same janet
`len`-in-arith gap as 58.10–13, `janet-join-intrinsic-gaps`). native 0
failing; c 13 / janet 36 == baseline.

## Original notes


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
