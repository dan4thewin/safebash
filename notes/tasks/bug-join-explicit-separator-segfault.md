---
title: "P1 memory-safety: join <arr> <sep> (explicit separator) segfaults"
status: ready
priority: P1
kind: bug
depends: []
gates:
  - "$(join a \",\") — join with an explicit separator — produces the joined string, never segfaults"
  - "$(join a) (default space separator) still works"
  - "0 native/c/janet spec regressions; add a spec pinning explicit-separator join"
---

# `join <arr> <sep>` segfaults

## Discovery

Found 2026-08-11 while designing DD#75 (the `*`-join → `$(join …)` replacement):

```
array t=(a b c); echo $(join t ",")
# → Segmentation fault (exit 139)
```

Bare `$(join t)` (default space) works; passing an explicit separator crashes.
The runtime signature is `sash_join_args(argv, argc, sep_ptr, sep_len, out_len)`
(`sash/src/codegen/setup.rs:340`) — the explicit-separator path mishandles the
separator argument.

## Why it matters

DD#75 retires the implicit-join `*` forms (`${a[*]}`, `$*`) in favour of the
**explicit** `$(join a SEP)`. That replacement must not crash — and a segfault
is a memory-safety escape in a safe-subset compiler. P1.
