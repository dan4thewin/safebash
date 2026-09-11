---
title: "`join <map>` silently drops the keys — reject it (SB4030)"
status: closed
priority: P2
kind: bug
depends: []
gates:
  - "`$(join m)` on a map is a compile error (SB4030), like `keys` on an array"
  - "`$(join a)` on an array is unchanged"
  - "a spec pins the rejection"
---

## Decided 2026-09-10 (DG): reject (option A)

```sh
array p=(k1 v1 k2 v2); map m = @{p:~}
$(join p)   # "k1 v1 k2 v2"
$(join m)   # "v1 v2"      <- keys silently gone, no diagnostic
```

`join` on a map returns the **values only**. It is in **no DD and no spec** —
undefined behaviour that happens to do something.

## Why reject

It is the hazard DD#99 stage 8 had just ruled on. Stage 8 rejected
`array a = @m` because flattening a map back was never specified and guessing
a pair order is a silent mis-pairing. `join m` is the quieter version of the
same thing: instead of guessing an order it discards half the data outright.

It also fails the test that disqualified polymorphic `len` (see
`dd75-len-collection-only-strlen`): one spelling whose *meaning* shifts with
the operand's type — "the elements" for an array, "the values" for a map.
Keeping it would have argued against a decision made the same day.

Rejected alternatives:

- **keep values-only, specify it** — cheapest, but entrenches the
  meaning-shift the `len` amendment exists to remove;
- **join the pairs** (`k1 v1 k2 v2`) — the symmetric inverse of
  `map m = @{a:DISP}`, but that is exactly the flatten stage 8 rejected; it
  would reinstate through the back door what was just closed at the front.

## If the capability is wanted

The explicit spellings exist or are cheap: `$(join $(keys m))` for the keys,
and a `values` builtin (map-only, mirroring `keys`) for the values. That would
be a separate proposal — the point here is not to smuggle it into `join`.

## REVERSED 2026-09-11 (DG) — not a bug; values-only is correct

> "arrays and maps differ only in how they're indexed, splat of an array
> doesn't produce indices, why would splat of a map…?"

The filing above is wrong, and the error is in its first sentence: it treats
map keys as **data** ("keys silently gone"). They are **indices**. `@a` does
not yield `0 1 2`, and nobody calls that silently dropping the indices — so
`@m` yielding values is the same operation, not a lesser one.

That also dissolves the meaning-shift objection. Under the values rule `join m`
is `join a` with the type selecting the traversal, which is precisely the
*coherent-category* polymorphism `dd75-len-collection-only-strlen` blesses. In
fact that task's own survey table already listed `for v in @x` as ✅ for array
**and** map — the values reading, one row above the `join` row used to condemn
it. The `join` row was the outlier in its own table.

The two rejected alternatives are re-read accordingly:

- *"keep values-only, specify it"* — this is now the ruling, and it entrenches
  nothing: there is no meaning-shift to entrench.
- *"join the pairs"* — still wrong, and now for a sharper reason (below).

`array a = @m` (SB5061) is **unaffected** and stays rejected — but its DD#99
stage 8 reasoning is replaced too, since "silent mis-pairing" presupposed the
pairs reading. It survives on the **broken round trip**: `map m = @{a:~}`
consumes alternating k/v, so `array a = @m` reads as its inverse and is not
one. See the DD#75 clarification.

No `values` builtin is needed: `join m SEP` gives the values, `keys m` the
indices.

Specs `spec/var_modifiers.sh` af.5–af.10.
