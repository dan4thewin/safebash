---
title: "len <undefined> → compile error (DD#75 §2), safely (needs loop-var/param tracking)"
status: done
priority: P3
kind: task
depends: []
gates:
  - "`len nope` (a name declared nowhere — not a collection, scalar, loop var, or param) → compile error, not a silent 0"
  - "no false positive: `len s` (string), `len a` (array), `len <loopvar>` (`for e in @a; do … $(len e)`), `len <param>` all stay OK"
  - "0 native/c/janet spec regressions"
---

# `len <undefined>` hard reject — the DD#75 §2 remainder

## Context

DD#75 §2 wants `len <undefined>` to be a compile error. Today it dispatches
to `strlen` of an empty var → **`0`**. This is **safe** (the memory-safety
concern from `len-nonarray-operand-reject` was resolved by the type-dispatch —
no garbage/segfault), just lenient. Split out of `dd75-len-polymorphic` (whose
core shipped).

## Why it was deferred (confirmed 2026-08-20)

The reject would mirror `type_check::check_join_operand` (reject a `len`
operand that is not a known name). But a "known name" is
`env.vars ∪ collections ∪ intrinsic_int ∪ positionals`, and **loop variables
are not tracked**: `type_check::check_for` checks the header + descends into
the body but never inserts `f.variable` into `env.vars`. So a naive reject
false-positives on `len <loopvar>` (e.g. `for e in @a; do echo $(len e);
done`). Read-single-var targets and function params likely have the same gap.

## Fix direction

1. Track loop variables (and any other valid scalar bindings) in `env.vars`
   during type-check, scoped to the body — `check_for` inserts `f.variable`
   (String element type, or the int element type for an `int array` splat).
   Watch scope leakage (env is `&mut` and accumulates) and interactions with
   the DD#62 undeclared-read check.
2. Then add `check_len_operand` (mirror `check_join_operand`): a `len` operand
   that is neither a collection nor a known scalar → error. `len <string>`
   stays valid (byte length).
3. Specs: `len nope` → reject; `len s` / `len a` / `len <loopvar>` / `len
   <param>` stay OK.

## Provenance

`dd75-len-polymorphic` core done 2026-08-11; this remainder confirmed-deferred
2026-08-20 while finishing the arith partials (the loop-var tracking gap makes
a naive reject unsafe). Low value (safe→nicer error), real regression risk.

## Done (2026-08-21)

Implemented as a dedicated `validate.rs::check_len_operands` pass (SB5039), NOT
the type_check route the task originally sketched. Key realization (DG): the
loop-var gap is only in *type_check*'s env — `validate.rs`'s SB5031 machinery
(`collect_scope_names`) already tracks `for` variables, declarations, and
`read`/`mapfile` targets, so reusing it makes `for e in @a; do $(len e); done`
correct for free. Added `argv [N] ARR` as a target to `collect_scope_names`
(the last-arg array) so `argv 1 a; $(len a)` isn't a false positive. `args` is
valid inside a function, SB5024 at top level. Specs types.sh 75.27-75.30.
