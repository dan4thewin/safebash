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

One instance is already filed on its own: `type_check::check_subscript_types`
walks expressions privately for DD#80's key-type check and ends in `_ => {}`.
SB4041 runs on `ir::walk_all_exprs` instead. The private walker misses most
index positions, giving a native ICE and a silent C miscompile
(`bug-subscript-key-check-skips-positions`). Like the first two, it was found
while probing something nearby.

## What to watch for when deleting

The one in `type_check.rs` looked interchangeable with
`ir::walk_arith_expansions` and was not — the shared name descends into each
`Expr` via `walk_expr_tree`, while the copy stopped at the root. The
behaviour-preserving target was `walk_arith_expansion_roots`. Check which of a
pair the callback actually assumes before swapping; picking the descending form
would have quietly *widened* coverage, which is a different change.

## A fourth instance, found and fixed (B37, journal 538)

`resolve.rs` resolved command heads inside `$( … )` bodies through its own
walk (`resolve_expr`, `resolve_each`, `resolve_test_expr`), which missed a
parameter expansion's index, the leaves of `$(( … ))` and `(( … ))` tests. A
command there kept a bare head, which the taint pass reads as clean. The walk
is deleted in favour of `Statement::cmdsub_bodies_mut`, a twin of the
checkers' `cmdsub_bodies` pinned by `ir::cmdsub_twin_tests`.

Unlike the first two, it was not a fork: it predates the shared walk
(d4fdc63, 2026-06-22, against 93d4ac1, 2026-09-29) and shares no name with
it, so the name signal above would not have found it. Compare what each walk
reaches, not only what it is called.

## A fifth instance, found and fixed (B120, journal 539)

A function pipeline stage set up its call through its own copy of the
inliner's argument handling. The copy took the caller's positional array
before lowering the arguments, so an argument reading the caller's `$1` or
`$#` found no array: an ICE in both backends, and a wrong `$#` in C. F2
deleted the copy: `build_function_pipeline_stage` now calls
`inline_function_call` inside `enter_forked_child`/`exit_forked_child`
(stgarg.1-9).

## `Program::function_defs` and the scans it could replace (F3, journal 541)

F3 added `ir::Program::function_defs`, every definition after SB5071 (top
level and END), and moved the transform's `register_functions` and
`bindings::resolve` onto it (B8, B16). Other private scans for definitions:
- `resolve.rs` `collect_functions` recurses through every block, so it
  covers END already. It runs after SB5071 and could use `function_defs`.
- `type_check.rs`'s scans are repaired in journal 565:
  `infer_collection_signatures` and `type_check` use `function_defs`, and
  `infer_types` is deleted. Collection calls in both roots share global
  metadata; `typeflow.21`-`.24` pin the signature traversal as well.
- `validate.rs` `collect_functions` runs before SB5071 rejects a misplaced
  `fn`, so it must keep recursing. It is not a candidate.

## Instances six and seven, found and fixed in F5 (B53, journal 543; B73, journal 544)

- `analyze_arity` inferred a function's signature from its own copy of the
  body walk: eleven `sig_scan_*` functions ending in `_ => {}`. The copy
  missed positional reads in six positions (an assignment index, a
  dispatch pattern, a redirect target, …), so calls drew a false SB5043.
  It now runs `ir::walk_scoped_stmts` and `ir::walk_own_exprs`.
- Arithmetic had three private call walkers. type_check's
  `walk_arith_funccalls` spelled out its recursion behind a `_ => {}` that
  swallowed `Subscript`/`SubscriptAssign`, so a sexpr under a subscript
  index escaped every type_check rule (`$(( a[(toupper x)] ))` printed `1`).
  validate's `walk_arith_intrinsic_calls` had had the same wildcard until
  `bug-sb5012-blind-to-arith-status-reads` repaired it in place, and that
  repair never reached type_check's copy: this task's title pattern, a
  third time. All three are gone. `ir::walk_arith_funccalls` runs over
  `ir::walk_arith_nodes`, whose descent is the total `ArithExpr::children`.
