---
title: "DD#75 §2 — len is type-polymorphic (array/map → count, string → bytes); ${#x} is Int sugar for $(len x)"
status: ready
priority: P2
kind: task
depends: []
gates:
  - "len <array|map> → element/entry count; len <string> → byte length (DD#12); len <undefined/unknown> → compile error (no garbage)"
  - "dispatch is on the operand's TYPE (state.collections), not the `$` sigil (retires the len a vs len $a syntactic split)"
  - "${#x} is Int-typed sugar for $(len x): int n=${#s} and int n=${#a} compile (was SB4030); ${#a} on an array yields the count (was 0)"
  - "$(( (len a) + 1 )) and $(( ${#a} + 1 )) evaluate the count (see intrinsics-lengths-in-arithmetic)"
  - "0 native/c/janet spec regressions; specs in types.sh pin the polymorphism + int-typing"
---

# DD#75 §2 — polymorphic `len`; `${#x}` Int sugar

Implements DD#75 §2. **Supersedes** `length-operator-int-typing` and
`len-nonarray-operand-reject`, and satisfies the length half of
`intrinsics-lengths-in-arithmetic`.

Go semantics: `len x` is an int keyed on `x`'s type. `${#x}` is the operator
sugar for `$(len x)` (the only surviving `${#…}` form after §5). Fixes the
scalar-`len` garbage bug by dispatching on type and rejecting unknown names.

See DD#75 §2. Demo-motivated (DG: "definitely want `$(( (len a) + 1 ))` to work").
