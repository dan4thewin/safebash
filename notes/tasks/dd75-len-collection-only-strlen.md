---
title: "DD#75 §2 amendment: `len` is collection-only; add `strlen` for scalars"
status: ready
priority: P2
kind: feature
depends: []
gates:
  - "`len <array>` / `len <map>` → element/entry count (unchanged)"
  - "`len <scalar>` → compile error naming `strlen`"
  - "`strlen <scalar>` → byte length; `strlen <collection>` → compile error"
  - "both remain int-typed in `$( )` and inside `$(( ))`"
  - "the ~16 scalar `len` spec sites migrate to `strlen`"
---

## Decided 2026-09-10 (DG)

> "make len collection only, add strlen for scalar"

Amends DD#75 §2, which made `len` type-polymorphic on Go semantics.

## Why

A survey of every type-dispatching operation showed `len` is the **only** one
that crosses the scalar/collection boundary:

| operation | scalar | array | map |
|---|---|---|---|
| **`len`** | 5 (bytes) | 3 | 1 |
| `join` | SB4030 | ✅ | ✅ (values only — correct; DG 2026-09-11 ruled a splat yields values, not indices, so this row is ordinary collection polymorphism. See the DD#75 clarification; `join-map-reject` is closed as not-a-bug.) |
| subscript `x[i]` | segfault (filed) | ✅ | ✅ |
| `for v in @x` | — | ✅ | ✅ |
| `keys` | SB4030 | SB4030 | ✅ |

Polymorphism itself is common and fine: `join`, subscript and `for-in` all
span array *and* map. But they are polymorphic over a **coherent category** —
"a collection" — where the operation means the same thing and the type only
selects the traversal.

`len` is different: *bytes* for a string, *elements* for a collection. Two
different questions sharing one spelling, with nothing at the call site to say
which is being asked. `$(len x)` on a name declared far away is ambiguous to a
**reader** in a way `$(join x)` is not — the compiler knows via
`state.collections`, but that is not who the ambiguity hurts.

DD#75 §3 already made the opposite call for `keys`: map-only, on the reasoning
that an indexed array's keys are trivially `0..len-1`. Declining to overload a
spelling whose meaning shifts is the same principle; §2 is the outlier.

(Go can afford the overload because it is statically typed with the
declaration usually in view. This project has since pivoted toward
unambiguous/self-consistent over paste-from-elsewhere familiarity.)

## Scope

- ~115 `len` sites in `spec/`, of which **~16 have scalar operands** and
  become `strlen`: `arithmetic.sh` (3), `types.sh` (8), `commands.sh`,
  `convert.sh`, `errors.sh` (2), `var_modifiers.sh`.
- `strlen` joins the value-intrinsic family (`len`/`keys`/`join`/`match`) —
  `value_intrinsic_arity` = 1, int-typed, usable in `$(( ))`.
- The transform already distinguishes the cases (`ssa.rs` maps `len` →
  `arrlen` when `state.collections` contains the name), so this is largely
  splitting an existing dispatch into two named builtins rather than new
  analysis.
- `sash-convert` rewrites bash `${#x}` → `$(len x)` today (DD#75 §6); it must
  now choose `strlen` for a scalar.

## Note

DD#75 §2's "sole length spelling" argument is not lost — there is still
exactly one spelling per *question*, which is the stronger form of it.

## Addendum (2026-09-11) — absorb the splat-operand removal

DD#100 Part D settles that builtins take **bare names and no splats** in a
name operand, and defers the removal here rather than doing it separately,
since this task must decide `len`'s operand set anyway and would otherwise
churn the same sites twice.

Scope to absorb:

- Reject `len @a` (2 sites) and `join @a SEP` (17 sites) in `spec/` +
  `contrib/`. Measured today, `join a ","` and `join @a ","` are exact
  synonyms, so this is a pure spelling restriction.
- **Update SB5025's message.** It currently *recommends* the spelling being
  removed — "use `$(join @a)` for a string, `${a[i]}` for one" (recorded at
  `spec/glob_splat.sh:99`). That guidance must become `$(join a)`.
- `keys` already accepts bare names only (SB5062), so it needs no change.
