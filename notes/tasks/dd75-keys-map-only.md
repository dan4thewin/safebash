---
title: "DD#75 §3 — keys is map-only (indexed/scalar → compile error)"
status: ready
priority: P3
kind: task
depends: []
gates:
  - "$(keys <map>) → splat of the string keys (unchanged); keys <indexed array> and keys <scalar> → compile error pointing at `for (( i=0; i<$(len a); i++ ))` / @a"
  - "the indexed-keys blank-output bug is gone (replaced by the reject)"
  - "map values via @m and ${m[$k]} still work; no `values` intrinsic added"
  - "0 native/c/janet spec regressions; specs pin keys-on-map OK, keys-on-array/scalar rejected"
---

# DD#75 §3 — `keys` map-only

Implements DD#75 §3. An indexed array's keys are trivially `0..len-1`, so `keys`
on a non-map is misguided bash-compat; reject it. Also removes the current bug
where `keys` on an indexed array returns blanks.

Related but separate: the P1 `keys-splat-context-bypass` (memory safety) — that
guards `$(keys m)` in command-arg position and should land first/independently.

See DD#75 §3.
