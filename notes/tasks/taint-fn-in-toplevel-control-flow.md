---
title: "A function defined inside top-level control flow keeps the definition-point taint walk"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "a function defined inside a top-level `if`/`while`/`for` is taint-checked against the completed top-level tainted set, like every other function"
  - "no duplicate diagnostics for such a function"
---

## Context

`check_taint` now runs in two phases (see SB5067 and DD#100 Part F): phase 1
walks the whole top level deferring function definitions, phase 2 walks each
deferred body against the completed tainted set. That closed the bypass where a
global tainted *after* a function definition was invisible inside the body.

Phase 1 collects the functions to defer by scanning the **top-level statement
list** for `StmtKind::Function`. A function defined inside top-level control
flow —

```sh
if [[ … ]]; then
  fn f { for v in @g; do echo "$v"; done }
fi
argv g
f
```

— is not in that list. It is reached through `taint_stmt`'s `_` arm, which
recurses into `child_bodies()` and hits the `StmtKind::Function` arm, so it
still gets the **old** definition-point walk with the partial tainted set.

## Severity

This is strictly **not worse than before** the two-phase change: it is the
behaviour every function had. But it is the one remaining hole of that shape,
and the pattern is legal sash.

Left as-is deliberately rather than half-fixed: making phase 1 skip a nested
function needs either a separate top-level dispatch (duplicating `taint_stmt`'s
match) or a phase flag threaded through the walk, and phase 2 would then have
to walk it with the right *enclosing* scope — a function nested inside another
function's body must still inherit that body's locals, which the current
recursion handles correctly.

## Fix sketch

Give phase 2 the list of *all* functions reachable from the top level (not just
direct children), paired with the scope they should be walked in, and make
phase 1 skip exactly that set. Nested-inside-a-function definitions must keep
inheriting enclosing locals via `taint_function_body`'s existing recursion.

## Related

- SB5067 (`validate::check_fn_taints_global`)
- `notes/decisions/DD100-collection-parameters.md` Part F
