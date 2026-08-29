---
title: "BUG: `${int_var:-default}` broken in c and janet backends (native ok)"
status: pending
priority: P2
kind: bug
depends: []
gates:
  - "`int x=5; int n=${x:-0}; echo $n` prints 5 in c and janet (native already does)"
---

## Symptom

A `:-` (or `-`) default applied to an **int** variable miscompiles on the c and
janet backends; native is correct.

```
$ sash -c 'int x=5; int n=${x:-0}; echo $n'        # native: 5  ✓
```

- **c**: `--emit-c` emits `n = sash_atoi_or(x_ptr, x_len, 0);` but `x_ptr` /
  `x_len` don't exist — an int var is stored as an `i64`, not a string
  ptr/len. gcc: `error: 'x_ptr' undeclared`.
- **janet**: emits `(if (= (length x) 0) "0" x)` — `(length x)` on a number
  errors at runtime (int has no length).

Both backends assume the `:-` operand is a **string**; they don't handle the
int-typed case, where the "is-empty" test and the value are an integer.

## Cause (likely)

The `:-` lowering in `emit_c.rs` (`emit_pe_default` / the param-expansion
default path) and `emit_janet.rs` (`emit_pe_default`) use the string
ptr/len / `(length …)` shape unconditionally. For an int operand they should
use the int slot (and a `== 0`/"unset" test appropriate to the int
representation, matching native `parameter_expansion_type`'s `:-` type-preserve
semantics).

## Found

Coverage grind (2026-08-29), cov.78: a spec driving type_check
`parameter_expansion_type` (`:-` type-preserve arm) passed native but failed
c+janet, surfacing this. cov.78 is guarded `in_modes native` meanwhile.
