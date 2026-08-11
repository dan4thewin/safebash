---
title: "Subscript-sigil enforcement asymmetry: a[i]= (bare string key) accepted on indexed array, but ${a[i]} rejected"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "a[i]=… on an indexed array (bare word key) rejects with SB4030, same as the read ${a[i]}"
  - "a[$i]=… (sigilled variable index) and a[1]=… (literal) still work"
  - "0 native spec regressions; add a spec pinning the assignment reject"
---

# Bare string-key subscript is rejected on read but not on assignment

## Discovery

Found 2026-08-11 (DG clarification on subscript sigils, DD#75 §4):

```
array a=(x y z); i=1
echo ${a[i]}   # SB4030 — bare `i` is the string key "i", rejected on indexed array
a[i]=Q         # accepted silently (should also be SB4030)
```

The subscript-index invariant (index is a numeric literal or `$`-sigilled; a
bare word is a string key) is enforced on the **read** path but not the
**assignment** path. Both sides must apply it.

See DD#75 §4 (Subscript-index invariant) and §7.
