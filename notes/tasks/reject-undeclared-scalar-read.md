---
title: "Reject a bare scalar read that is not definitely assigned (DD#62 zero-value model)"
status: ready
priority: P2
kind: feature
depends: [DD62-test-expr-simplification]
gates:
  - "echo $foo (assigned nowhere) is a compile error (new SB code)"
  - "msg=hi; echo $mgs (typo) is rejected"
  - "if c; then x=1; fi; echo $x is rejected (c-false path reaches $x unassigned)"
  - "x=1; echo $x compiles; echo ${x:-default} always compiles"
  - "while/for bodies that may run zero times do not count as assigning the body's targets"
  - "typed zero-init counts as assigned: int x -> 0, array a -> empty (settle the exact set)"
  - "diagnostic points at ${x:-...} for intentional maybe-unset"
  - "spec tests; native + c + janet; no net regression"
---

# Reject a not-definitely-assigned scalar read

## Decision (DG, 2026-06-21) — DD#62

A bare scalar read `$x` / `${x}` is a **compile error** unless `x` is
**definitely assigned on every path** reaching the read (strict
definite-assignment, not merely "declared somewhere in scope"). The
explicit opt-out is the default form **`${x:-WORD}`** (DD#65), always
allowed, which is the only way a scalar **zero value** is now reached.

```
echo $foo                      # reject — assigned nowhere (typo class)
msg=hi; echo $mgs              # reject — typo
if c; then x=1; fi; echo $x   # reject — c-false path reaches $x unassigned
x=1; echo $x                  # ok
echo ${x:-default}            # ok (explicit maybe-unset)
```

## Why

The headline safe-subset value: a never-assigned / typo'd variable read
is the single biggest shell footgun (`rm -rf "$PREFIX/"` with a typo'd
`$PREFIX`). sash is a compiler with full visibility, so it can reject this
at compile time. `${x:-…}` already expresses "maybe-unset," so erroring on
bare reads costs no expressiveness — it forces intent.

## Where / feasibility

- `type_check.rs::TypeEnv` already tracks declared variables/types — the
  starting point for "is this name known."
- The **all-paths** ("definitely assigned") analysis wants a CFG dataflow
  join. sash already builds SSA with **phi insertion at branch joins**
  (`transform/`), so the join machinery exists — the check can run on the
  SSA/CFG (a name unassigned on some predecessor → its phi/use is
  "maybe-unset") or as a dedicated definite-assignment pass over the IR
  CFG. Decide placement (pre-SSA IR pass vs piggyback on SSA) in design.
- `${x:-…}` reads bypass the check (explicit opt-out).

## Sub-questions to settle in design

- **Typed zero-init**: `int x` (no initializer) → is `x` assigned to `0`?
  `array a` → empty? Proposed: a typed declaration zero-inits and counts as
  assigned. Confirm the exact declaration set that zero-inits.
- **Loop variables**: after `for i in LIST { }` or a `while`, is `$i`/a
  body target "assigned"? A possibly-empty loop does not assign → reading
  after the loop is rejected unless pre-assigned. (Inside the body, the
  loop var is assigned.)
- **Positionals** (`$1`/`$@`/`$#`): governed by DD#46 (function-local,
  top-level SB error) — orthogonal; not scalars this check owns.
- **SB code**: assign next-free at implementation.

## Discovery

DD#62 decision #3 open item, resolved 2026-06-21 in the DD walkthrough.
Current behavior: undeclared/typo'd scalar reads compile and yield empty
silently (verified).

## Resolved (DG, 2026-06-21) — narrowed design

- **Scalars only.** Definite-assignment rejects a bare scalar read `$x`/
  `${x}` that is not definitely assigned on every path. `${x:-WORD}` is
  the opt-out.
- **Two passes (mirrors taint):** an **IR-pass** is primary — it carries
  source spans, so the diagnostic points at the read and suggests
  `${x:-}`; the **SSA** check is a soundness backstop that **ICEs** if it
  catches a maybe-unset read the IR-pass missed (version-0 / phi-source
  propagation already present in SSA). Exclude compiler-managed specials
  (`$1`/`$@`/`$?`/`$$`/`$LINENO`…) and the `${x:-}`/`${x:?}` forms.
- **Typed scalar decl = assigned.** `int x` (no initializer) zero-inits to
  `0`, so `$x` after it is fine (matches today).
- **Containers are OUT of scope.** `array a`/`map m` are always assigned
  (a valid *empty* container); definite-assignment never applies. An empty
  container's only consequence is cardinality at an **external `@a`
  splat**, handled by the disposition (`@{a:-}`/`@{a:?}`) — NOT here.
- **Loop targets:** a possibly-empty `for`/`while` does not assign its
  body's scalar targets (reading them after the loop is rejected unless
  pre-assigned); the loop variable is assigned inside the body.
