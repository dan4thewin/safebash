---
title: "Int-type the `${#…}` length operator: `int n=${#s}` and `int n=${#a[@]}`"
status: done
superseded_by: dd75-len-polymorphic (DD#75 §2)
priority: P2
kind: bug
depends: []
gates:
  - "`int n=${#s}` (string length) compiles — `${#var}` length is int-typed, not string (was SB4030)"
  - "`int n=${#a[@]}` (array cardinality) compiles — int-typed"
  - "does NOT mis-type the prefix-strip `${var#pat}` (operator `#` WITH an operand) as int — only the length form (no operand / `[@]`) is int"
  - "0 native spec regressions; add specs (types.sh) pinning `int n=${#s}` and `int n=${#a[@]}`"
---

# `${#…}` length is typed string, so `int n=${#s}` is wrongly rejected

## Discovery

Found 2026-08-04 while fixing `$(len a)` int-typing (DG's demo bug). `$(len
arr)` and `epochseconds` were fixed by adding them to
`is_int_returning_intrinsic` (type_check.rs), but the **`${#…}` length
operator goes through a different type path** and is still typed string:

```
int n=${#s}       # SB4030 — but ${#s} is a length (int)
int n=${#a[@]}    # SB4030 — array cardinality (int)
echo ${#s}        # → 5  (runtime is fine; only the *type* is wrong)
```

Same user-facing frustration as `$(len a)` ("can't assign a length to an
int"), different code path.

## Root cause

`parameter_expansion_type` (`sash/src/type_check.rs`) types the `#`-length
operator as `String`:

```rust
fn parameter_expansion_type(pe: &ParameterExpansion, env: &TypeEnv) -> VarType {
    let Some(op) = pe.operator.as_deref() else { return env.var_type(&pe.name); };
    if op == ":-" || op == "-" { env.var_type(&pe.name) } else { VarType::String }
}
```

The length form falls into the `else → String` bucket.

## Fix (sketch)

Return `VarType::Int` for the length form. **Care:** the operator `#` is
overloaded — `${#s}` (length, prefix `#`, no operand) vs `${s#pat}`
(prefix-strip, infix `#`, has an operand). Distinguish by the operand /
however the parser tags the length form (check how `${#s}` and `${#a[@]}`
set `ParameterExpansion { operator, operand }` — likely `operator=Some("#")`
with `operand=None` for length). Only the length form is int; strip stays
string.

## Relation

Sibling of the shipped `is_int_returning_intrinsic` fix (commit adding
`len`/`epochseconds`). Together they complete "a length/count expression is
int-typed" across both spellings (`$(len …)` and `${#…}`).

## Closed 2026-09-10 (backlog prune)

Bookkeeping only — closed 2026-09-10. Superseded by DD#75 §2 (polymorphic `len`); `${#…}` is retired (SB4038).
