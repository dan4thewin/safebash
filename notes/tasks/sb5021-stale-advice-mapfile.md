---
title: "SB5021's advice names the retired `mapfile`, and calls a map an array"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "SB5021 does not name `mapfile` (retired by DD#99; following it gets SB5060)"
  - "the message says `map` when the target is a map"
  - "the advice for a map names the actual route: read -> attest -> `map m = @{a:DISP}`"
---

## What

```
$ m[foo]=$(/usr/bin/printf zz)
error [SB5021] tainted value cannot be placed in array 'm' — external data
enters arrays via read/mapfile; or attest a scalar first and assign the
clean value
```

Two defects, both introduced by DD#99 rather than original:

1. **`mapfile` is retired.** An author who follows the advice hits SB5060
   ("`mapfile` is retired"). The message sends them to a dead end.
2. **It calls a map an "array"**, and the suggested route is wrong for one:
   under DD#99 stage 5 external data cannot enter a map at all except via a
   vouched-for array (`read ARR` → `attest … in ARR` → `map m = @{ARR:DISP}`).

The second half of the advice ("attest a scalar first and assign the clean
value") is correct and verified working for both arrays and maps.

## Context

This is the guard that makes DG's 2026-09-10 "no tainted maps" ruling
airtight: collections are protected at the **write**, not the read.

| | behaviour |
|---|---|
| `s=$(cmd)` | allowed, taints `s`; the READ needs attest (SB5020) |
| `a[0]=$(cmd)` | **SB5021** at the write |
| `m[foo]=$(cmd)` | **SB5021** at the write |

## Found

2026-09-10, backlog prune — by reading the diagnostic rather than the code.
