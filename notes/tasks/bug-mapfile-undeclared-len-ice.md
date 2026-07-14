---
title: "bug: mapfile / read -a into an UNDECLARED array ICEs on len/element-read (undefined array a)"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`mapfile -t a < f; echo $(len a)` (no prior `array a=()`) must NOT ICE — either auto-declare fully (like `argv [N] a` now does) or reject at compile time"
  - "same for `read -a a` and a subsequent `$a[0]` / `@a` read"
---

# mapfile / read -a undeclared target: ICE on len/element-read

## Symptom

```
mapfile -t a < /tmp/lines.txt
echo $(len a)
```

with **no prior `array a=()`** → `ICE: undefined array: a` (a codegen
panic, `sash/src/codegen/inst_handlers.rs::find_array_slot`).

## Root cause

`find_array_slot` (codegen/types.rs) resolves an array by its **versioned**
key `a_0` (or `a_*`). The codegen setup slot-scan for the fill instructions
(`SsaInst::Mapfile | ReadArray | MatchInto`) inserts the **plain** name
`a` (`setup.rs`), which `find_array_slot` never looks up. So without a
prior declaration/assignment (which is what creates the `a_0` slot), the
fill target has no `a_0` slot and any later `len`/`$a[i]`/`@a` panics.

DG's DD#60 auto-declare verification only exercised the **taint**-rejected
path (`mapfile … ; use → SB5020`), which never reaches the `a_0` lookup, so
this ICE was masked.

## Fix (argv already does this — mirror it)

`argv-snapshot-to-outparam` (2026-07-08) fixed the same class for
`argv [N] a`: `transform/stmt_handlers.rs::try_handle_argv` emits an empty
`IndexedArray` Assign for the target *before* the fill when it isn't
already a collection — that Assign creates the versioned `a_0` slot, so
reads resolve. Do the same for `mapfile` / `readarray` / `read -a`
(`try_handle_mapfile`, `cmd_handlers::handle_read`): when the target isn't
already declared, emit the empty-array Assign (and register the collection
in `state.collections` + the type checker's `check_command`, as argv does).

Alternatively (DG decision): if these are meant to REQUIRE `array a=()`
first, reject an undeclared target at compile time instead of ICE-ing.

## Notes

- Pre-existing; surfaced while implementing argv out-param. Orthogonal to
  that task (argv is fixed).
- An ICE is a zero-tolerance stop-everything per the bug policy — this is a
  real codegen panic on valid-looking input, not a diagnostic.
