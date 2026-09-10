---
title: "bug: subscripting a scalar (`${s[0]}`) segfaults"
status: ready
priority: P1
kind: bug
depends: []
gates:
  - "`s=hello; echo ${s[0]}` is a COMPILE error naming the type mistake, not a segfault"
  - "the undefined case stays SB5031 (it already is)"
---

## What

```sh
s=hello
echo "[${s[0]}]"     # Segmentation fault (core dumped), rc 139
echo "[${s[9]}]"     # likewise
```

Compiles clean, then dumps core. An **undefined** name is correctly rejected
(SB5031), so the guard exists — it just does not cover "declared, but not a
collection".

The subscript lowers to a table read; a scalar's slot holds a string pointer,
so the read dereferences a `char*` as a `SashTable*`.

## Type context

Subscript is **collection-polymorphic** by design (array by position, map by
key — both work, DD#80). The defect is that the third case, a scalar subject,
is neither supported nor rejected.

Sibling operations reject it properly: `keys s` and `join s` are both SB4030
("`s` is a scalar"). The same check applied to the subscript path is likely
all this needs.

## Found

2026-09-10, while surveying which operations in the language are
type-polymorphic (`len`, `join`, subscript, for-in) for the `len` review.
