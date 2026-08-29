# 204 — Fix P1 bug: const reassignment panic (SB4044, fork C)

First of the two bugs the coverage grind surfaced. `const x=1; x=2` panicked
the compiler (`thread 'main' panicked at ssa.rs:926`, rc 101) — a reachable
`panic!` on ordinary parseable input. DG approved **fork C** (both a validate
check and a define-site backstop).

## Cause

`VersionState::define` (ssa.rs) enforced readonly by `panic!`. `const` →
`DeclKeyword::Readonly` → `readonly_vars`; validate had **no** const-
reassignment check, so `const x=1; x=2` passed validation, reached transform,
and the second `define("x")` panicked. `define()` is the mutation funnel (plain
assign, `+=`, arith `(( ))`, `read`, …) so the panic was reachable many ways.

## Fix (SB4044, defense-in-depth)

- **B — validate.rs `check_const_reassignment`**: a flat pass (matching the
  transform's flat `readonly_vars` model) collecting `const` names in document
  order and flagging, with a **spanned** SB4044: a plain/append/indexed
  assignment or a re-declaration of a const. Statement-level arith mutations
  (`(( x=5 ))`, `(( x++ ))`) lower to `Assignment` nodes, so the Assignment arm
  catches those spanned too. Walks all blocks + nested bodies via
  `child_bodies()`.
- **A — ssa.rs `define()` backstop**: the `panic!` becomes a spanless SB4044
  pushed to `state.diagnostics` (fall through, no panic). Catches the remaining
  channels — verified `read`/`mapfile` into a const both hit it.

## Verified

- Rejected (SB4044, no panic): `x=2`, `x+=2`, `(( x++ ))`, `(( x=5 ))`,
  `const x=2`, nested-in-`if`, `read x`, `mapfile x`.
- Not a false positive: `const x=1; echo $x`, a different var `z=2`.
- `readonly` keyword is separately rejected (SB5001, use `const`) — unchanged.
- Specs `spec/types.sh` 35.3-35.9. Full 3-mode sweep: **0 regressions, 21 new
  passing** (7×3).

Adjacent edge noted, not this bug: `argv [0] x` into a const isn't SB4044-caught
but independently runtime-aborts via the intrinsic-arity fallthrough (bug #2) —
entangled with that, not a silent const violation.

## Why it mattered

Exactly the reachable-panic the coverage audit is meant to surface: ssa.rs:926
was a "missed" line; probing it revealed the panic. Unexamined code hid a P1.
