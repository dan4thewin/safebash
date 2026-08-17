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
  - "no janet regression"
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
