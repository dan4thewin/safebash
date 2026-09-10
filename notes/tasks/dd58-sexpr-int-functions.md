title: "DD#58: implement sexpr int-function calls in $(( ))"
status: done
severity: feature
depends: []
gates: [58.1, 58.2, 58.3, 58.4, 58.5, 58.6, 58.7, 58.8, 55.12, 16.2]

Implement DD#58 integer function capture via s-expression syntax
inside `$(( ))` and `(( ))`.

## What to implement

1. **Sexpr parser in arithmetic context**: Inside `$(( ))` and `(( ))`,
   `(name args...)` where `name` is a known `-> int` function becomes
   a function call. `(expr)` where the first token is not a known
   function is arithmetic grouping (unchanged).

2. **$(f) type error**: When `$(f)` appears and `f` is a known
   `-> int` function, emit compile error — `$(...)` captures stdout
   but `-> int` functions produce no stdout.

3. **Name collision check**: Function names and variable names must
   not collide. Emit compile error if both `fn x` and `x=...` exist.

4. **Runtime codegen**: `$((f))` and `((x = f))` need to emit real
   function calls in Cranelift, or extend the inline expansion to
   capture return values in arithmetic context.

## Key files
- `sash/src/transform.rs` — sexpr parsing in arithmetic expressions
- `sash/src/codegen.rs` — Cranelift function call emission
- `sash/src/validator.rs` or `sash/src/transform.rs` — name collision, $(f) type error

## Spec tests
- 58.1-58.8 in `spec/types.sh`
- 55.12 in `spec/errors.sh`
- 16.2 in `spec/types.sh` (updated to DD#58 syntax)

## Amendment (2026-08-15) — point 2 superseded by Option C

Point 2 above ("`$(f)` type error" for a `-> int` function) was **superseded**
by Option C (commit `a8ac707`): `$(f)` is return-type-polymorphic and captures a
`-> int` function's return value. See the amendment banners in DD#58 / DD#17.
The sexpr `$(( (f) … ))` remains available; both surfaces work for every int
producer (user `-> int` fn or int intrinsic).
