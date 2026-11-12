title: "Bug: the C backend prints i64::MIN as the bare literal `-9223372036854775808`, which C reads as unary minus on an unsigned constant"
status: open
priority: P4
kind: bug
depends: []
gates:
  - "the emitted C for m1, a9, m2 and m3 compiles under `gcc -std=c11 -Wall` with no \"integer constant is so large that it is unsigned\" warning"
  - "every site that renders an i64 into C source goes through one helper, which spells i64::MIN `(-9223372036854775807L - 1)`"
  - "m1, a9, m2 and m3 print the same values in both modes as today; a spec pins each one"

## Found

2026-10-27, while porting F1+F4 (journal 537), which made `return -1` a
status value. The output is right in both backends, so nothing a user sees is
wrong today. The emitted C is wrong all the same.

| Id | Program | Output (both modes) | Emitted C |
|---|---|---|---|
| m1 | `int n=-9223372036854775808; echo "[$n]"` | `[-9223372036854775808]` | `n = -9223372036854775808;` |
| a9 | `fn f { return -9223372036854775808 }; f; echo "[$?]"` | `[-9223372036854775808]` | `__exit_status = -9223372036854775808;` |
| m2 | `int x=1; (( x += -9223372036854775808 )); echo "[$x]"` | `[-9223372036854775807]` | `i64 __t0_r = (-9223372036854775808);` |
| m3 | `int x=1; x=$(( x * -9223372036854775808 )); echo "[$x]"` | `[-9223372036854775808]` | the same as m2 |

C has no negative literals. `-9223372036854775808` is unary minus applied to
`9223372036854775808`, which fits no signed type, so gcc warns "integer
constant is so large that it is unsigned" at every one of these sites. It then
negates an unsigned value and converts the result to `i64`. That conversion is
implementation-defined, and gcc defines it as modulo, which is why the value
comes out right. The C backend is the differential oracle (AGENTS.md), so it
should not depend on a gcc extension. The native backend is unaffected.

Probes: `files/port-f1f4-probes.tar.gz`, directory `r4/` (a9.sh, m1.sh, with
the emitted a9.c and m1.c) and `r4/mn/` (m2, m3, and m4, a control that
overflows correctly).

## Cause

`lower_value_as_int` (emit_c.rs, `SsaValue::Literal | SsaValue::Expr`) renders
a parsed i64 with `n.to_string()`. Its `ArithExpr` arm passes
`operand.to_string()` to `emit_arith_op_c` the same way. Enumerate every other
site that writes an i64 into C source before fixing.

## Fix direction

One helper renders an i64 for C: `(-9223372036854775807L - 1)` for i64::MIN,
and the decimal text otherwise. (`i64` is `long` in the emitted prelude, so the
suffix is `L`.) Route every site through it, and add a spec per row.
