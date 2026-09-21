---
title: "`arith_brief` has two dead arms, and \"x = ...\" is rendered in three places"
status: next
priority: P3
kind: cleanup
depends: []
gates:
  - "the `Assign` and `CompoundAssign` arms of `arith_brief` are removed, or a comment records why they are kept"
  - "the three independent renderings of the `x = ...` brief are reconciled to one, or each is documented as deliberate"
  - "spec suite unchanged (the arms are unreachable, so no behaviour moves)"
---

# `arith_brief`: two unreachable arms

`sash/src/validate.rs::arith_brief` (2272-2310) renders the description
embedded in SB5007 messages. Two of its arms cannot be reached:

- `ArithExpr::Assign` (2274)
- `ArithExpr::CompoundAssign` (2275-2282)

Both were cold in the union coverage (spec + unit, 5863 tests). Cold is only a
hint, so each was **proved** by replacing the arm body with a `panic!` and
running the full sweep plus targeted probes: **zero hits** in either case, from
5863 specs and 14 hand-written programs aimed directly at them
(`if (( x = 5 ))`, `while`/`until`/`for` headers, `if (( (x += 1) ))`,
`if (( y + (x *= 2) ))`, a comma condition, a ternary, `(( x <<= -1 ))`).

## Why they are unreachable

`arith_brief` has three callers, and none can deliver either shape:

- `validate_arith_as_statement` fires only for `ArithKind::Comparison`; an
  assignment classifies as `Mutation`.
- `validate_arith_comma_in_condition` fires only for a non-final part that is
  **not** a mutation; both shapes are mutations.
- `validate_arith_in_condition` is the one that could — but a mutation
  condition is decomposed first, and the message is emitted by
  `validate_assignment_stmt` (validate.rs:1050) instead.

`CompoundAssign` additionally never survives parse: `lower_arith_command`
(`parse/arith.rs:295`) routes through `lower_arith_mutation_structured`
(1314-1319), which rewrites `x op= v` into `Assign { value: BinOp }` via
`binop_on_var`. Embedded forms such as `(x += 1)` arrive wrapped in a `Paren`
and render through the `_` catch-all as `...`.

`PostIncDec` is **not** in the same position — `if (( x++ ))` does reach
`arith_brief`, and that arm is hot. The asymmetry between `x++` and `x = 5`
here is the surprising part and worth preserving in whatever comment replaces
the arms.

## The larger finding: three renderings of one brief

`"{} = ..."` is produced independently in three places:

| site | form |
|---|---|
| `sash/src/validate.rs:2274` | `arith_brief`, `Assign` arm — **dead** |
| `sash/src/parse/arith.rs:187` | `brief_arith`, `Assign` arm |
| `sash/src/validate.rs:1050` | hard-coded inside the `format!` of `validate_assignment_stmt` |

This is why the dead arm was not obvious: the message a user sees for
`if (( x = 5 ))` is character-identical to the one the dead arm would have
produced. Reading the output tells you nothing about which site produced it —
it took instrumentation to tell them apart.

That duplication is the part worth fixing, more than the seven dead lines. Two
of the three are live, so a change to the wording of one silently desynchronises
the others.

## Note

Filed from journal 383. Sibling of `tok-to-text-dead-arms` — same class (arms
kept for shapes an earlier pass has already eliminated), found the same way.
Worth deciding the two together as a convention rather than case by case: DG's
call whether provably-dead match arms come out or stay as defensive
documentation.
