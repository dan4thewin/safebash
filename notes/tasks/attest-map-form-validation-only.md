---
title: "record: the map 2-binder `attest` is validation-only now (its taint role is dead)"
status: done
completed: 2026-09-12
priority: P3
kind: docs
depends: []
gates:
  - "DD#93 (and/or DD#99) records that `attest k v in m` no longer has a taint role"
  - "no behaviour change — DG decided 2026-09-10 to keep the form"
---

> **DONE 2026-09-12** — recorded in `notes/decisions/DD93-attest-ergonomics.md`
> under "The map 2-binder form is validation-only". All four routes were
> re-measured; see the note below on the stronger finding.

## What

DG asked whether the 2-binder `attest k v in m` is dead code after the DD#99
stage 5 ruling ("the read of the array must have a clean array, so no tainted
maps").

**Its taint-clearing purpose is dead.** Every route to a tainted map is
closed:

| route | result |
|---|---|
| `m[k]=$(getenv X)` | SB5021 (guarded at the write) |
| `map m = @{a:DISP}` from a tainted array | SB5020 |
| map literal with a tainted value | SB5021 |

**The form itself is not dead**, and DG kept it. On a clean map it is still a
live runtime gate:

```sh
attest k v in m [[ $k =~ ^[a-z0-9]+$ && $v =~ ^[a-z0-9]+$ ]]
#   an entry fails -> rc 1, "attest: m failed the gate"
#   all pass       -> rc 0
```

That matters because **`attest` is the only assert-with-diagnostic in the
language**: DD#70 deliberately made a bare `[[ ]]` a must-consume compile
error rather than an assertion (SB4018), and `[[ … ]] || exit 1` aborts
silently. Dropping the map form would remove the only way to validate a map's
contents with a message, for no safety gain.

## To record

DD#93 (attest) and/or DD#99 should state that for a **map** the form is now
validation-only — taint cannot reach one. For scalars and arrays the taint
role is still live, and attest-on-clean is a degenerate-but-harmless use
there.

## Open, minor

"attest" reads as vouching for untrusted data, which is precisely what the map
form no longer does. A rename is not proposed — noting it so the asymmetry is
deliberate rather than forgotten.

## Verified 2026-09-12

All routes re-measured against the release binary. The filing's table holds,
with one correction and one addition:

| route | filing said | measured |
|---|---|---|
| tainted value into a map by subscript | SB5021 | **SB5021** |
| `map m = @{arr:DISP}` from a tainted array | SB5020 | **SB5020** |
| matchall on an unattested tainted scalar → map | (not listed) | **SB5020** |
| **`attest k v in m` on a tainted map** | (not listed) | **SB5020 — the attest is never reached** |

That last row is stronger evidence than the filing offered: it is not merely
that taint *does not* reach a map, but that the 2-binder form **cannot** clear
one even when written, because construction is rejected first. There is no
program in which its taint-clearing role is reachable.

Confirmed still live on a clean map: a passing 2-binder attest compiles and
the map is readable afterwards.
