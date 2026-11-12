title: "SB5021's advice names the retired `mapfile`, and calls a map an array"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "SB5021 does not name `mapfile` (retired by DD#99; following it gets SB5060)"
  - "the message says `map` when the target is a map"
  - "the advice for a map names the actual route: read -> attest -> `map m = @{a:DISP}`"

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

## Resolution

Fixed 2026-09-11. `validate::SlotCx` now carries a `map_names` set (built by
the new `collect_map_names`, which reuses `type_check::collect_collection_kinds`
rather than re-deriving the `-A`/`-a` classification), and the SB5021 text is
produced by `tainted_into_collection_msg(name, is_map)`.

Array target:

```
tainted value cannot be placed in array 'a' — external data enters an array
via `read ARR`; or attest a scalar first and assign the clean value
```

Map target:

```
tainted value cannot be placed in map 'm' — a map is built from a vouched-for
array: `read ARR`, then `attest x in ARR [[ … ]]`, then `map m = @{ARR:DISP}`;
or attest a scalar first and assign the clean value
```

All four write shapes are covered: array subscript, map subscript, `array`
literal, and `@{…}` literal (the last passes `is_map = true` straight from the
AST; the other three consult `sx.is_map`). Every route the new text recommends
was run end-to-end before the wording was written.

Spec: 60.56–60.59 in `spec/taint.sh` (all four shapes; fail-first verified).
