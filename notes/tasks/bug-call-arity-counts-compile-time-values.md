---
title: "SB5042/SB5043 count compile-time SsaValues, so a splat or elide at a call site is miscounted in BOTH directions"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`f @q` into a fixed-arity `f` is rejected as SB5087, whatever `q` holds"
  - "`f one ${x:~}` cannot silently deliver fewer arguments than the callee's `min`"
  - "no diagnostic states an argument count that is not the count"
  - "a splat or elide into a VARIADIC callee is accepted and delivers per-element"
  - "specs for both directions, in both modes"
---

# The bug

`transform/cmd_handlers.rs:282`:

```rust
check_callee_arity(name, body, arg_values.len(), state);
```

`arg_values.len()` is the number of **SsaValues**, and both
`SsaValue::ArraySplat` and `SsaValue::ElideEmpty` are *one* SsaValue at
compile time that expands to 0..n tokens at runtime. So the number handed
to the arity check is neither the runtime count nor a bound on it, and the
check is wrong in both directions.

This is the third instance of one class: **a compile-time count taken over a
value that expands at runtime.** The other two were the `exec` argv builder
and the DD#84 elidability gate (both fixed, journal 441).

## Direction A — accepts a call it has itself proven invalid

`f one ${x:~}` into a `Fixed{2,2}` callee compiled and printed `a=[one] b=[]`.
Writing the same runtime shape literally (`f one`) is correctly SB5042.

## Direction B — rejects a valid call, with a false count

`f @{q:-}` on a two-element `q` was rejected as "called with 1" — the constant
1 for any splat, regardless of the array's size.

# Resolved 2026-09-28 — DG's rule

DG settled it, and the answer was simpler than any of the options this file
had drafted:

> to my mind, an elision, given it could remove an argument, can only work for
> a variadic context and not a fixed one. similarly, a splat should only work
> for variadic given it could be empty (could try to track provably present
> elements of the array, but edge cases make that a non-starter, I think)

A dynamic count is legal only where the callee accepts any number. Recorded as
the DD#84 addendum of 2026-09-28, with the per-argument minimum table;
implemented as **SB5087** in `check_callee_arity` + the new `call_arg_bounds`
(which is why the check now takes the argument *values*, not their count).

Both directions fall out: direction A is now rejected outright, and direction
B never reaches a count — a splat into a variadic callee is accepted without
one, and into a fixed-arity callee it is SB5087, whose message does not claim
an argc. Every count still printed belongs to a statically-countable call.

**Measured cost: one spec** out of 6865 (`functions.sh` sp.3, migrated).

## What this file got wrong

Two sections argued against the answer, and both were unfounded:

- **"Why option 'reject dynamic counts into fixed-arity callees' is wrong"**
  claimed the rule kills DD#84's motivating case, `f a ${maybe:~}` into a
  `Fixed{1,2}` callee. It does reject that — but DD#84 §(A) never promised it.
  The DD says an absent optional arg is one "known at compile time", which a
  `:~` elide by construction is not. The objection was invented here, not read
  from the DD. The motivating case is written `f a` or `f a "$maybe"`, or the
  callee declares `: @args`.
- **"The honest check"** proposed diagnosing only on disjoint ranges. It also
  asserted that bare `@a` "is rejected earlier (SB5030/SB5032 — DD#69 requires
  a disposition), so it does not arise". False: bare `@a` is legal at a call
  site and is the common spelling (specs sp.1, sp.9). DD#69's disposition
  requirement is about *empty*-array behaviour, not about splatting at all.

Both were asserted from reading rather than execution — the same failure mode
the DD#100 addendum of 2026-09-17 records against itself. The options A/B/C
this file offered DG were therefore a choice among three answers to a question
that did not need asking.

## Found while implementing — three further bugs

1. **`: @args` promoted an optional arg to required.** `analyze_arity`'s
   variadic floor chained `required ∪ optional`, unlike the fixed branch.
   Adding DD#81's variadic marker turned an accepted `f` into SB5042. Fixed;
   specs 84.66/84.67; DD#84 addendum.
2. **A splat joined in a pipeline stage** — the same bug
   `bug-splat-to-user-function-joins` closed on 2026-09-10, in a third
   position its fix missed. See that file.
3. **`=name` reached the slot on one call path in four** (SIGSEGV and an ICE).
   See the DD#100 addendum of 2026-09-28.

Numbers 2 and 3 were found by probing the *consequences* of this rule across
every call position, not by the rule itself — the rule needed to know what a
splat lowers to on each path, and three of the four paths turned out not to
lower it at all.

## Left open, deliberately

- `arity_too_many_msg` is called with `min`, not `max`: a `Fixed{1,2}` callee
  called with 3 prints "takes 1 arguments". Message-only, pre-existing,
  untouched here to keep the regression signal clean. Worth its own filing.
- `collect_bulk_targets` / `collect_collection_names` (`validate.rs` ~7169,
  ~7144) look to share the missing `$( )` descent that
  `collect_slot_arg_reads` and `check_call_args_in` had. Not probed.

# Where

- `sash/src/transform/cmd_handlers.rs` — `check_callee_arity`,
  `call_arg_bounds`, `arity_dynamic_msg`, `arity_floor_msg`
- `sash/src/transform/functions.rs` — `analyze_arity`
- Specs `optional_args.sh` 84.53–84.67; `functions.sh` sp.3

journal 443.
