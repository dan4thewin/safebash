---
title: "BUG: nested/parenthesized arith assignment silently drops its side-effect"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`(( (a=2) )); echo $a` prints 2 (bash), not 0"
  - "`(( 1 + (a=2) )); echo $a` prints 2"
  - "either the side-effect is emitted, or the nested-assignment form is a compile error — never a silent wrong value"
---

## Resolution (2026-08-28) — fork A (DG-chosen), statement-only

Implemented. A `(( … ))` **statement** now **hoists** an embedded assignment in
an unconditionally-evaluated position — the store takes effect and the value is
the assigned value, matching bash: `(( (a=2) ))`, `(( 1 + (a=2) ))`,
`(( a=2, (b=3) ))`, `(( (a+=5) ))`, `(( ((a=7)) ))`. Done in
`lower_arith_test_stmt` → `hoist_arith_mutations` (transform/statements.rs),
emitting each store via the standard `Assignment` path.

The consistent rule (SUBSET.md): a mutation belongs in a `(( … ))` **statement**
only. Everything else is a clean compile error (never a silent wrong value,
honoring gate 3):
- a mutation in a `$(( … ))` **value** → SB4020 (`check_arith_value_mutation`);
- a mutation in an `if`/`while`/`(( … ))` **condition** → SB5007
  (`validate_arith_in_condition` now tests `arith_has_mutation`, so
  `if (( (a=2) ))` / `while (( (i=i+1) <= 3 ))` join the existing
  `if (( x=5 ))` / `if (( x++ ))` rejects — conditions are pure tests);
- in a `(( … ))` statement, an embedded `++`/`--`, an `a[i]` mutation, or a
  mutation under `&&`/`||`/`?:` → SB4020 (`check_arith_test_placement` /
  `arith_unhoistable_reason`).

The pre-existing `(( x = y++ ))` reject (parser `reject_nested_mutation`) is
unchanged.

The now-unreachable value-path arms (`incdec_to_ssa`, `compound_assign_to_ssa`,
and the `Assign`/`CompoundAssign`/`IncDec` arms of `arith_to_ssa`) were removed
/ replaced with a documented `unreachable!` — the half-wired path that dropped
the store no longer exists. Specs `arithmetic.sh` 31.25–31.36 (7 statement
hoists, 5 rejects incl. 2 condition-context). native+c 0 regressions.

## Symptom (found via the coverage grind)

An arithmetic assignment that is NOT the top-level expression — wrapped in
parentheses, or an operand of a larger expression — silently computes its value
but **drops the store**. The variable keeps its old value; no error.

```
$ sash -c 'int a=0; (( a=2 ));      echo $a'   # 2  (correct)
$ sash -c 'int a=0; (( (a=2) ));    echo $a'   # 0  (WRONG — bash: 2)
$ sash -c 'int a=0; (( 1 + (a=2) )); echo $a'  # 0  (WRONG — bash: 2)
$ sash -c 'int a=0; int b=0; (( a=2, b=3 )); echo $a $b'  # 2 3 (correct)
```

Silent incorrectness — the worst failure mode.

## Likely cause

A top-level `(( a=2 ))` / comma-list lowers via the statement-mutation path
(which emits the store). A NESTED assignment (`ArithExpr::Assign` /
`CompoundAssign` / `PreIncDec` / `PostIncDec` inside `Paren` or a `BinOp`
operand) is lowered by the value path `arith_to_ssa`
(`transform/expr_handlers.rs`) via `incdec_to_ssa` / `compound_assign_to_ssa`,
which build an `ArithBinOp` **value** but never emit the SSA store for the
mutated variable. So the new value is computed and discarded.

This is exactly why coverage showed `incdec_to_ssa` / `compound_assign_to_ssa`
as never-run in the value path: the store they *should* drive is missing.

## Fix — approach fork (present to DG)

- **A. Emit the store** in the value path: when `arith_to_ssa` encounters an
  assignment/inc-dec, emit `state.define(var)` + an `Assign` inst for the new
  value (so the side-effect persists) AND return the value. Matches bash;
  makes nested assignments work.
- **B. Reject nested assignment** in arithmetic (only top-level / comma-list
  mutation allowed): a spanned SB error for an assignment that isn't a
  top-level comma element. Simpler, keeps the "arith mutation is a statement"
  discipline, but diverges from bash and loses a (rare) idiom.

DG decides the language surface: does sash support a nested arith assignment
(`(( x = (y = 1) + 2 ))`) or reject it? Either is fine — the silent wrong value
is not.

## Found

Coverage grind, probing inputs to reach the never-run `incdec_to_ssa` /
`compound_assign_to_ssa`. Third reachable-correctness bug the audit surfaced
(after the const-reassign panic and the intrinsic-arity fallthrough).
