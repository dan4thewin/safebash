---
title: "cleanup: remove dead sash_toupper_first/sash_tolower_first runtime"
status: ready
priority: P4
kind: cleanup
depends: []
gates:
  - "sash-rt no longer defines sash_toupper_first / sash_tolower_first (no front-end reference remains)"
  - "sash-rt builds; c-mode links; 0 spec regressions"
---

# Remove the dead first-char case-conversion runtime

The first-char case operators (`${var^}` / `${var,}`) were pruned by DD#65, and
the codegen case path is now a `bool` (upper/lower, whole-string only) — journal
174 removed the last front-end references to `sash_toupper_first` /
`sash_tolower_first` (the ExternFuncs fields, setup declarations, and emit_c
externs). The runtime functions in `sash-rt/src` are now unreferenced.

Remove `sash_toupper_first` / `sash_tolower_first` from `sash-rt`, rebuild
`libsash_rt.a`, confirm c-mode still links (`SASH_RT_LIB`), and run the spec
suite. Small; deferred from the P6 ICE-arm work to avoid a sash-rt rebuild in
that commit.
