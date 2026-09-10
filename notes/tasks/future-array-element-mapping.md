---
title: "future: element-wise string ops on an array (map over elements)"
status: deferred
priority: P3
kind: decision
depends: ["bug-sed-op-on-array-empties-it"]
gates:
  - "a decision recorded, not necessarily an implementation"
---

## What

Deferred alternative to `bug-sed-op-on-array-empties-it` (DG 2026-09-10:
reject now, "add A to list of future considerations").

The rejected-for-now design: a string operation applied to an array **maps
over its elements**.

```sh
array a=(x:1 y:2)
a =~ s/:/-/g        # -> (x-1 y-2)
```

## Why it was attractive

- Symmetric with "an array is a collection of strings" — the operation means
  the obvious thing rather than being an error.
- Makes in-process reshaping possible, which is what DD#99 leans on: today
  reshaping an array requires a `for` loop and a second array.
- The statement form is the natural place for it, since the target is already
  the array.

## Why it was not taken now

`$(gsub @a …)` is a *value* form, so mapping would give `gsub` two return
types (scalar or array) depending on its operand — real new surface, decided
against while the immediate defect (silent emptying, then a segfault) needed
closing. Rejecting is small, safe, and does not foreclose this.

## If revisited

The clean split is probably: map for the **statement** form (`a =~ s///`,
target is the array), reject for the **value** form (`$(gsub @a …)`, where
`@a` in a scalar operand position is arguably the real mistake). That keeps
one return type per function.
