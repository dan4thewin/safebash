---
title: "`join <map>` silently drops the keys — reject it (SB4030)"
status: ready
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
