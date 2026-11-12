title: "Three partial renderers for one idea: quoting an arith mutation back"
status: done
priority: P3
completed: 2026-09-19
kind: rival
depends: []
gates:
  - "one renderer, total over ArithExpr; no `_ =>` that silently degrades"
  - "31.59, 31.63, 31.72-31.74, 31.90-31.106, cov.150-151 must all still pass"

# `arith_brief` / `expansion_mutation_brief` / `brief_arith`

Three functions answer "how do I quote this arithmetic mutation back to the
user". They are partial re-derivations of each other and they disagree, which
is DD#116's rival pattern in its most literal form.

| renderer | file | Assign | CompoundAssign | PostIncDec | BinOp | fallback |
|---|---|---|---|---|---|---|
| `arith_brief` | validate.rs:2405 | yes | yes | yes | **recurses** | `...` |
| `expansion_mutation_brief` | arith.rs:101 | yes | yes | yes | no | `None` |
| `brief_arith` | arith.rs:197 | yes | **no** | yes | no | `...` |

All three build on the same four primitives in `ir.rs` (`assign_brief`,
`compound_assign_brief`, `incdec_brief`, `arith_op_str`) — the vocabulary is
already shared. Only the dispatch is duplicated.

## Why this is worth doing

The defect has already been found and fixed *twice*, independently, in two of
the three:

- `expansion_mutation_brief`'s own doc comment (arith.rs:98-100): "`Assign`
  and `CompoundAssign` were once matched in a single pattern that printed
  `x = ...` for both, so `$(( x *= 2 ))` was reported with an operator the
  author never wrote."
- `arith_brief` rendered fourteen of nineteen operators as `?` until spec
  31.63 pinned it.

Neither fix reached `brief_arith`, which still has no `CompoundAssign` arm and
so renders `mutation ...` for `(( x = (y -= 2) ))`. Pinned by 31.106.

A fourth site, `validate_assignment_stmt`, had the same bug for a different
reason — it only ever sees the decomposed form — and is fixed separately with
`ir::mutation_target_brief` (31.90-31.94). That one cannot be folded in: the
spelling is genuinely gone by then, so it is a different question with a
different honest answer.

## Shape

Move the dispatch next to the primitives in `ir.rs`, total over `ArithExpr`,
and have all three call it. `expansion_mutation_brief` keeps its `Option`
wrapper (its caller uses `None` to mean "not a mutation, carry on"), so it
becomes a predicate plus a call rather than its own renderer.

## Done

`ir::arith_brief` is now the only renderer, total over all seventeen
`ArithExpr` variants with the deliberate elisions listed explicitly rather
than reached through `_`. `validate::arith_brief` and `brief_arith` are
deleted; `expansion_mutation_brief` keeps the *whether* question and delegates
the *how*.

Observable fix: `(( x = y -= 2 ))` reported `mutation ...` and now reports
`mutation y -= ...` (spec 31.117). The parenthesised form still elides (31.106)
— `Paren` is elided by all three and that was not changed here, since it is a
diagnostic-text decision rather than a consolidation.

**It also found a live bug.** The unified renderer has a `PreIncDec` arm,
which two of the three rivals already had and the third did not; resolving
that disagreement in favour of the documented `PREFIX_INCDEC_INVARIANT`
made the arm fire. `reject_prefix_incdec` was itself a fourth partial
re-derivation of a tree walk — no ternary, unary, or subscript-assign value —
so ten `++x` shapes survived the parser and ICEd downstream. Verified
pre-existing by rebuilding the previous tree and reproducing all ten. It now
delegates to `ArithExpr::children()`. Specs `31.107`-`31.116`.
