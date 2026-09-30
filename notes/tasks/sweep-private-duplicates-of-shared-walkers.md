---
title: "Sweep for other private duplicates of shared walkers"
status: next
priority: P2
kind: hardening
depends: []
gates:
  - "every module-private function whose body duplicates a `crate::ir` / `crate::ssa` walker is listed"
  - "each is either deleted in favour of the shared one, or its divergence is justified in writing"
  - "any behaviour difference found is treated as a bug, not a style issue"
---

# A fix reached the original and not the copy — twice now

`bug-arith-default-blind-under-subscript` (journal 456): `type_check.rs` held a
**private duplicate** of `ir::walk_arith_expansions`. The shared one was made
total for `ArithExpr::Subscript`/`SubscriptAssign` — its comment names the
hazard explicitly — and the copy never got the fix. Result: an ICE natively and
a silent `sash_atoi("abc")` under `--emit-c`.

`notes/journal/441-the-second-implementation.md` is the same failure in
codegen: `emit_exec_static_argv` had a stale fork of `build_argv_fixed`, which
hid a splat/elide ICE and a missing NUL guard.

Two instances, two different subsystems, same mechanism: **the fork existed, so
the fix had somewhere not to reach.** Neither was found by reading the copy —
both were found while reading something else nearby.

## The work

Enumerate module-private functions that re-implement a walker or builder that
already exists in `ir.rs`, `ssa.rs`, or the codegen helpers. Name collisions are
the cheapest signal (that is how this one surfaced — two functions named
`walk_arith_expansions`), but a renamed copy will not show up that way, so the
index's `block_dupes` view and a read of each pass's local helpers are both
needed.

For each: delete in favour of the shared one, or write down why it must differ.
A divergence nobody can justify is a bug waiting for the next fix to miss it.

## What to watch for when deleting

The one in `type_check.rs` looked interchangeable with
`ir::walk_arith_expansions` and was not — the shared name descends into each
`Expr` via `walk_expr_tree`, while the copy stopped at the root. The
behaviour-preserving target was `walk_arith_expansion_roots`. Check which of a
pair the callback actually assumes before swapping; picking the descending form
would have quietly *widened* coverage, which is a different change.
