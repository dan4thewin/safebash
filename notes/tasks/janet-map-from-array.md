---
title: "janet: `map m = @{a:DISP}` (DD#99 array→map) is not emitted"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "janet: `array a=(k1 v1 k2 v2); map m = @{a:-X}; echo ${m[k1]}` prints `v1`"
  - "spec `var_modifiers.sh` mp.2, mp.3, mp.4, mp.6, mp.9 pass in janet mode"
---

## Found

2026-09-09, implementing DD#99 stage 3. Native and c emit `SsaInst::MapFromArray`
(a `sash_map_from_array` call); the janet backend has no arm for it, so the map
stays empty.

## Note

The rejection half works in janet (SB5058 for a bare `@a` is a front-end
check), as does the empty-array case. Only the fill is missing. Same shape as
`janet-read-a-element-count` and the other janet parity gaps — the front end is
shared, the emitters are not.
