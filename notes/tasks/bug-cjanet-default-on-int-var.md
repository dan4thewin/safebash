title: "BUG: `${int_var:-default}` broken in c and janet backends (native ok)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`int x=5; int n=${x:-0}; echo $n` prints 5 in c and janet (native already does)"

## RESOLVED (2026-08-29, DG identity-element semantic)

Fixed in all three backends. DG ratified the semantic: **`:-` triggers the
default on the *identity element* of the subject's type** — `""` for strings
(existing), **`0` for ints** (new). So `${int_x:-D}` ≡ `x == 0 ? D : x` (DD#62
"meaningful zero" preserved — `:-` merely *chooses* 0 as its trigger, op-local).
This was ALSO a native behavior change: native previously returned `0` for
`int x=0; echo ${x:-99}` (it lowered int→string "0", tested string-emptiness =
length 1 → non-empty → never defaulted). All three backends applied
string-emptiness to an int; now all test `int == 0`.

Also DG (same day): **the default `D` must be an int literal** (decimal /
`0x`/`0o`/`0b`) when the subject is int — same discipline as the arithmetic
default (DD#77 D4). A string / interpolation / empty default is rejected at
type_check (**SB4030**), so the disposition stays homogeneously int. Unifies
with the arith-context rule (one mental model).

Implementation:
- **transform** (`transform/expressions.rs`, `source_carrying_value` `:-` arm):
  an int source lowers to `SsaValue::Ternary { cond: ArithBinOp{src, Eq, 0},
  true_val: default, false_val: src }` — reuses the ternary lowering, so
  **native + c** get it for free (they emit from SSA). `source_is_int` detects
  an int scalar (`type_map`) or int cell (`Subscript.is_integer_value`).
- **janet** (`emit_janet.rs`, `emit_pe_default`): scalar int branch emits
  `(if (= name 0) (string default) (string name))` (janet emits from the IR, not
  SSA, so the SSA ternary never reaches it). Both arms stringified so the result
  consumes uniformly under the int-context `scan-number` wrapper and a bare
  string context (a harmless, exact round-trip in the janet backend only).
- **type_check** (`type_check.rs`, new fourth pass `check_int_value_defaults`):
  a value-context `${int_x:-D}` with a non-literal `D` → SB4030. Treats `$(( ))`
  as opaque (the arith pass `check_arith_default_literals` owns those — no
  duplicate diagnostic).

Specs: coverage.sh cov.78/80/81 (positives, all modes), cov.82/83/84 (rejects,
all modes), cov.85 (int-cell positive, native+c — int arrays are a pre-existing
janet gap, see `janet-int-array-and-cell-default`). 0 native/c/janet regressions.

## KNOWN REMAINING (separate, filed)

Janet int **arrays** are broken independent of `:-` (`int array a=(4 5 6)` emits
`(scan-number @["4" "5" "6"])` — `scan-number` on an array literal → nil), so an
int-array **cell** `:-` (`${a[i]:-N}`) also fails in janet. Native + c handle it.
Filed → `janet-int-array-and-cell-default`.

## Symptom (original)


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
