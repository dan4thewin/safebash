---
title: "Trace the arith-mutation emitters that survive DD#109 uncovered"
status: ready
priority: P3
kind: cleanup
depends: []
gates:
  - "each of ar_incdec / ar_subscript_assign / hoist_mutation_node is either spec-covered or removed/ICEd"
---

# The arith-mutation cluster: reachable, or lowered away in the parser?

DD#109 renders the IR for every compiled program, which took `emit_ir.rs` from
33.7% to 93.4% lines. Two emitters did not move: `ar_incdec` (`:arith-post-incdec`
/ `:arith-pre-incdec`) and `ar_subscript_assign` (`:arith-subscript-assign`).

## Why they are probably unreachable

`parse/arith.rs:lower_arith_mutation_structured` rewrites `ArithExpr::PostIncDec`,
`PreIncDec` and `SubscriptAssign` into other statement forms **inside the
parser**, so those variants never appear in the `Program` that
`compile_to_object` hands to `emit_ir`.

Evidence:

- `int array a=(7 13 21); (( a[0] = 99 ))` is spec-covered
  (`spec/arithmetic.sh:482`) and `--emit-ir` shows only `:arith-literal`.
- `(( i++ ))` emits `:arith-binop :arith-literal :arith-var`.
- `$(( i++ ))` is rejected — SB4020, mutation in expression context.
- `++i` is rejected — SB4020, prefix form.
- `transform/expr_handlers.rs:376` already carries an `unreachable!` for
  exactly these three variants.

## Why this is not just an emit_ir question

`transform/statements.rs:hoist_mutation_node` is the **second-largest single
coverage gap in the compiler** (28 uncovered lines) and matches on the same
three variants; `hoist_arith_mutations` (11 uncovered lines) calls it. If the
parser really does lower them all away, that whole hoisting path is dead too,
and it is a much bigger removal than two IR emitters.

## What to do

Trace it the way journal 307 traced `lower_negated_stmt`: a temporary
`eprintln!` in each of `ar_incdec`, `ar_subscript_assign`, `hoist_mutation_node`
and `hoist_arith_mutations`, then the **whole spec suite**, not hand-picked
probes — the suite already contains the interesting arithmetic
(`spec/arithmetic.sh` nsx.1–3, the `a[i]++` and `a[i] += n` cases).

Then per DG's rule: reachable-untested → spec; dead → remove; impossible →
ICE. Note that if `hoist_mutation_node` is dead, `ArithExpr`'s three mutation
variants may be removable outright, which would also drop the
`transform/expr_handlers.rs:376` `unreachable!` and several `validate.rs`
helpers (`arith_has_incdec`, `arith_has_subscript_assign`).
