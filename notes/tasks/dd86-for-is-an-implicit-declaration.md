---
status: done
depends: DD86, DD100
gates: spec/scope.sh
---

# `for` introduces a name that outlives the loop — is it a declaration?

DG (2026-09-17): *"I never sanctioned a for variable as exempt from the
no-shadow rule. Rather I recall saying that a for variable survives the loop,
making the for an implicit declaration, rather than a non-declared special
var."*

The exemption currently in DD#86's model section is **not a decision** — it
entered in implementation commit `06912b0` on the mechanical grounds that a
for variable is its own `StmtKind`. See the retraction addendum in DD#86.

Measured: the loop variable survives (`after=3`, sash and bash alike), so it
is not loop-scoped and "not a declaration" does not describe it.

## Cases, all currently accepted

| # | program | hazard |
|---|---------|--------|
| A | `int x=5` … `for x in 1 2 3 { … }` | a declared, typed variable is silently retyped and overwritten by a loop |
| B | `for y in 1 2 {…}` … `for y in 8 9 {…}` | two introductions of one name; harmless in practice |
| C | `for z in 1 2 {…}` … `int z=7` | `int z=1; int z=7` is SB5049, but interposing a loop as the first introduction makes it legal |

## RESOLVED (2026-09-17)

DG: *"I don't think a variable should ever change type"*, plus *"`((x=7))`
should also create an int by assignment"*. Implemented as SB4030 on a `for`
binding and **SB5082** on a declaration after a binding; see the DD#86
addendum for the full model and `spec/scope.sh` 86.39-86.58.

Answers to the questions below, in order: (1) yes — a binding fixes the type,
so a later declaration is rejected, which covers A and C; (2) B stays legal,
because untyped loop variables are strings either way and no type changes;
(3) yes — a word list binds strings, an arithmetic header binds ints; (4)
measured at two sites, both rewritten.

## Original questions

1. Does `for` count as the declaration of its variable for SB5049 purposes
   (making A and C errors)?
2. If so, is B — two sequential loops reusing a name, an extremely common
   shape — also an error, or does re-binding by a *second `for`* stay legal?
3. Does the loop variable acquire a **type** from the declaration it now
   constitutes, and if so what happens to `for x in 1 2` vs `for x in a b`?
4. Migration: how much existing spec/contrib code does the chosen rule break?
   (Unmeasured — measure before deciding.)

## Not in scope

The reserved-name collision (`for A in @B`) is already **SB5080** via DD#100
Part B, and is a reservation question rather than a shadowing one.
