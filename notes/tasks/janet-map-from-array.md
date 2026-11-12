title: "janet: `map m = @{a:DISP}` (DD#99 array→map) is not emitted"
status: deferred
deferred-by: DD104
priority: P2
kind: bug
depends: []
gates:
  - "janet: `array a=(k1 v1 k2 v2); map m = @{a:-X}; echo ${m[k1]}` prints `v1`"
  - "spec `var_modifiers.sh` mp.2, mp.3, mp.4, mp.6, mp.9 pass in janet mode"

## Found

2026-09-09, implementing DD#99 stage 3. Native and c emit `SsaInst::MapFromArray`
(a `sash_map_from_array` call); the janet backend has no arm for it, so the map
stays empty.

## Note

The rejection half works in janet (SB5058 for a bare `@a` is a front-end
check), as does the empty-array case. Only the fill is missing. Same shape as
`janet-read-a-element-count` and the other janet parity gaps — the front end is
shared, the emitters are not.

## Raised to P2 (2026-09-10)

This now costs **real, previously-green coverage**. DD#99 stage 5 migrated the
`mapfile -m` sites onto `matchall` + `map m = @{a:DISP}`, and janet does not
emit that form at all, so five specs that passed under `mapfile -m` are now
skipped in janet: `wordsplit.sh` 22.6, 22.8, 22.9, 22.10 and `misc.sh` 54.1.

Nothing is wrong with those programs — janet simply cannot express the route
DD#99 requires for building a map. Until this lands, janet has no way to
construct a map from external data at all.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
