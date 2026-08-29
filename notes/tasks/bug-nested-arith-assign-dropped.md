---
title: "BUG: nested/parenthesized arith assignment silently drops its side-effect"
status: pending
priority: P1
kind: bug
depends: []
gates:
  - "`(( (a=2) )); echo $a` prints 2 (bash), not 0"
  - "`(( 1 + (a=2) )); echo $a` prints 2"
  - "either the side-effect is emitted, or the nested-assignment form is a compile error — never a silent wrong value"
---

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
