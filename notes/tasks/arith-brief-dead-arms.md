---
title: "`arith_brief`'s \"dead\" arms are live, and \"x = ...\" was rendered in four places"
status: done
priority: P3
kind: cleanup
depends: []
gates:
  - "the `Assign` and `CompoundAssign` arms of `arith_brief` are removed, or a comment records why they are kept"
  - "the independent renderings of the `x = ...` brief are reconciled to one, or each is documented as deliberate"
  - "the arms are reached by specs, so a third mis-measurement costs a red sweep"
---

# `arith_brief`: the arms are **live**

**This filing was wrong.** It claimed the `Assign` (validate.rs:2274) and
`CompoundAssign` (2275-2282) arms were unreachable, and said so with the
strongest evidence the project has: cold in union coverage, then *proved* by
replacing each arm body with a `panic!` and running the full sweep plus
fourteen hand-written probes aimed directly at them.

They are reachable. Measured 2026-09-21 with the same instrument:

```
if (( 1 > x = 2 ))     → Assign arm
if (( 1 > x += 2 ))    → CompoundAssign arm
if (( x++ > 0 ))       → PostIncDec arm   (already known live)
```

## Why fourteen targeted probes all missed

Every probe in the original set put the mutation in one of two positions, and
both of them route around these arms:

- **Top level** (`if (( x = 5 ))`, `while`/`until`/`for` headers) — classifies
  as `ArithKind::Mutation`, so `lower_arith_command` decomposes it into
  statements *before* validation. The message comes from
  `validate_assignment_stmt` (validate.rs:1050).
- **Parenthesised** (`if (( (x += 1) ))`, `if (( y + (x *= 2) ))`) — arrives
  wrapped in `ArithExpr::Paren`, for which `arith_brief` has no arm, so it
  falls to `_` and reads `...`.

The reaching shape is a mutation as the **unparenthesised right operand** of a
binary operator. Nobody writes it, which is why it was never probed: bash binds
`=` loosest and refuses `1 > x = 2` with *"attempted assignment to
non-variable"*, whereas sash's parser groups it as `1 > (x = 2)`.

That precedence difference is **not observable**. All eleven contexts probed —
condition, bare statement, both comma positions, all three `for`-header slots,
`$(( ))`, a ternary arm, an array subscript — reject the program
(SB5007/SB4020/SB4041). sash refuses everything bash refuses, by a different
route. No bug, so nothing to fix there.

## The methodology finding

Panic instrumentation plus a green full sweep proves **"not reached by these
inputs"**. It does not prove "unreachable". The sweep is 5900-odd programs
written by people who write ordinary sash; it says nothing about a grammar
corner nobody would author. The filing wrote "proved", and the second reader
(me) believed it and nearly deleted live code on that authority.

Where a reachability claim actually matters, the honest form is *"not reached
by the suite plus these N probes"*, with the probe set enumerated — which this
filing did do, and which is exactly what let the gap be spotted: every listed
probe shared one shape.

## What was done instead of deleting

1. **The arms stay**, with a doc comment on `arith_brief` recording the
   reaching shape and why top-level and parenthesised forms do not arrive.
2. **Specs 31.69-31.72** reach `Assign`, `CompoundAssign`, a non-`Add`/`Sub`
   compound operator, and `PostIncDec`, pinning the brief text in each case.
   A third mis-measurement now costs a red sweep.
3. **The four renderings are one** (DD#116). `crate::ir::assign_brief(var)` is
   the single source for `x = ...`; the four sites that built it independently
   were `validate.rs:2274`, `validate.rs:1050`, `parse/arith.rs:188` and
   `parse/arith.rs:84`.
4. **`arith_brief` is total over `IrArithOp`** via the new
   `crate::ir::arith_op_str`. DG's call (option A). It had arms for six of
   nineteen binary operators and two of nineteen compound ones, rendering the
   rest `?` / `?=` — so `(( y + (x *= 2) ))` reported `y ? ...`, quoting an
   operator the author never wrote. Spec 31.63 had *pinned* that `?`, with a
   comment noting the pin existed so a future arm addition would be visible;
   it is rewritten to pin the real operator.
5. **`$(( x *= 2 ))` reported `x = ...`** — the expansion site rendered both
   mutation kinds through one `Assign`-shaped message. Now
   `compound_assign_brief`. Specs 31.73-31.74.

`emit_ir::arith_op` is **not** a rival of `arith_op_str`: it renders Janet
keywords (`:add`) for the DD#104 IR seam, a different output language.

## Note

Filed from journal 383, corrected in journal 386. Sibling of
`tok-to-text-dead-arms`, which was the same class and went the other way —
there the `_` arm really was wrong, and making the renderer total fixed a bug
in what the compiler *accepted*. Here totality only improves message text; the
verdict was already correct.
