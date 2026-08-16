---
title: "janet backend: int-valued collections (int array / int map)"
status: ready
priority: P3
kind: backend-parity
depends: []
gates:
  - "`int array a=(…)` / `int map m=(…)` run in janet mode (currently: janet treats the collection variable as a scalar int → `scan-number` on the table)"
  - "arithmetic.sh ivc.1-8,11 pass in janet mode (widen the `in_modes native,c` guard to include janet)"
---

# Problem

DD#76 int-valued collections work native + c, but janet fails at runtime:

    error: bad slot #0, expected string … got <array>
      in scan-number …

Root cause: emit_janet works from the IR and keys variable handling off
`ctx.is_int(name)`. An `int array`/`int map` declares the collection *name*
with `VarType::Int`, so `is_int("a")` is true and the emitter treats the
collection variable as a scalar int (wrapping reads/writes in `scan-number`,
which chokes on the janet array/table value).

This is the janet analog of the native/c fix (a collection-valued assign is a
table, not a scalar) — but janet's `is_int` conflation is pervasive (assignment,
declaration, subscript read/int-context) rather than a single slot-allocation
site.

# Fix sketch

- Track int-valued collections distinctly from int scalars: `is_int(name)`
  should be **false** for a collection variable (it is a janet array/table),
  while `${a[i]}` on an int collection reads an **int** element.
- `emit_expr_subscript` (emit_janet.rs ~1788) must yield an int for an int
  collection in arithmetic context (this overlaps the existing
  `janet-arith-subscript-read` gap — janet's arith value path doesn't read
  embedded subscripts at all).
- Declaration path (~346/374): an `int array`/`int map` should emit a janet
  array/table (like the string collection path) and record int-element-ness in
  a side set, not mark the whole name int.

# Test

Widen the `if in_modes native,c` guard around ivc.1-8,11 in
spec/arithmetic.sh to include janet once green. ivc.9/ivc.10 (compile-rejects)
already run in every mode.
