---
title: "janet: arith VALUE path doesn't read an embedded `${…}` expansion (reads 0)"
status: ready
priority: P3
kind: bug
depends: []
effort: effort-janet-backend-parity
gates:
  - "janet `$(( ${a[i]} + 1 ))` / `(( x = ${m[k]} + 1 ))` read the cell (native+c already do); un-guard arithmetic.sh asx.1-4, asx.6 for janet"
  - "janet `$(( ${s:-9} + 1 ))` reads the DefaultValue (native+c do); un-guard arithmetic.sh pex.1-3 for janet (DD#77 A)"
  - "janet arith `:-`/`:?` disposition (DD#77 C2): `$(( ${s:-N} ))` / `${s:?M}` apply the strict-int trigger (native+c do); un-guard arithmetic.sh dsp.1-5 for janet"
  - "no janet regression"
  - "janet bare `a[i]` reads + lvalue (`$(( a[i]+1 ))`, `(( a[i]++ ))`, DD#79 Stage 2): the arith VALUE path reads the cell (native+c do); un-guard arithmetic.sh bsx.1-5 for janet"
---

## Symptom

Any `${…}` expansion imported into arithmetic (`ArithExpr::Expansion`) reads
**0** on the janet backend in VALUE context, but works in CONDITION context.
Covers subscripts `${a[i]}`/`${m[k]}` AND (DD#77 A) operator-bearing param
expansions `${s:-N}`/`${s:?…}`:

```
$(( ${a[0]} + 1 ))                 janet → 1   (should be 11)
$(( ${s:-9} + 1 ))  (s=5)          janet → 1   (should be 6)
if (( ${a[0]} > 5 )); then …       janet → yes (correct — reads 10)
```

native + c read both correctly (arithmetic.sh asx.1-6, pex.1-4 pass native,c).

## Context

Found wiring `ArithExpr::Expansion` (arith-subscript-values, part 1): the arith
parser now imports `${a[i]}`/`${m[k]}` as a normal-parse `Subscript` Expr, lowered
via the shared `expr_to_value` (SsaValue::Subscript → table read). native/c pick
this up in both the value and condition arith paths; **janet's value-arith
lowering (`emit_expr_as_int` / the `$(( ))` path in emit_janet) does not evaluate
the embedded subscript** — likely it treats the Expansion/subscript operand as a
plain var read → 0. The condition path (`emit_arith_test` / `emit_condition_inline`)
does handle it, so asx.5 passes.

This is the same family as the pre-existing c/janet dynamic-subscript value-read
gap the nonbrace-dynamic-subscript task noted (33.5/33.6). Part of the janet
backend-parity effort; fold into `janet-rt-primitive-library` (a faithful
`sash/table-get` primitive + a value-path that lowers the subscript operand).

spec arithmetic.sh asx.1-4, asx.6 are guarded `in_modes native,c`; asx.5
(condition) runs all modes. Widen once janet's value-arith reads the subscript.

### Extended (2026-08-19): cell dispositions in arith

`$(( ${a[i]:-0} + 1 ))` / `$(( ${a[i]:?msg} ))` (DD#77 string-array-cell
disposition) also read 0 on janet's arith VALUE path — same gap. Word-context
`${a[i]:-N}`/`${a[i]:?msg}` work on janet (emit_pe_default/emit_pe_require read
the cell via `emit_expr_subscript`). spec arithmetic.sh scx.12-14 are guarded
`in_modes native,c`; scx.7-11 (word) run all modes. Widen scx.12-14 once
janet's value-arith reads the subscript.

## Progress (2026-08-21, arith-disposition-hex-octal-default)

Added the missing `ArithExpr::Expansion` arm to `emit_janet.rs`
`arith_expr_to_janet`: a `${…}` embedded as an arith operand now emits
`(_num <inline>)` instead of an empty string (which produced `(+  0)` → 0).
This is the structural half of the second gate — janet now *reads* the embedded
expansion and coerces it (decimal + `0x`/`0o`/`0b` via `_num`). What remains is
janet's `:-` disposition *value* semantics diverging from native on a set-but-
non-int subject (`s=x`): native uses the default, janet uses `_num(s)`=0. So the
d4.3–d4.6 value assertions stay guarded to native,c. Un-guarding pex.1-3 / dsp.*
needs that semantic aligned.
