---
title: "janet: array append `a+=(x)` is emitted as string concatenation"
status: deferred
deferred-by: DD104
priority: P1
kind: bug
depends: []
gates:
  - "janet: `array f=(a b c); f+=(d); echo $(len f)` prints `4` (matches native/c)"
  - "janet: `array f=(a b c); f+=(d e); echo $(join f)` prints `a b c d e`"
  - "spec `undeclared.sh` ud.29 passes in janet mode"
---

## Found

2026-09-08, while fixing DD#97 (`len` on a glob array read 0). The new spec
`ud.29` — a control asserting that appending to a *declared* array stays legal
— passes in native and c but fails in janet.

## Symptom

```sh
array f=(a b c)
f+=(d)
echo $(len f)
```

native/c print `4`. janet prints `44`.

## Cause

The emitted janet stringifies both sides and concatenates them:

```janet
(var f @["a" "b" "c"])
(set f (string f @["d"]))        # ← should be (array/concat f @["d"])
(print (string (length f)))
```

`(string f @["d"])` renders the two arrays as text and joins them, giving
`@["a" "b" "c"]@["d"]` — 44 characters, hence `44`. The variable stops being
an array at that point, so every later collection operation on it is wrong,
not just `len`.

`+=` on a *scalar* is string concatenation and is correct; the emitter is
applying the scalar rule to a collection target without checking the kind.

## Not the same as

`janet-indexed-assignment` (`a[i]=v`) and `janet-int-valued-collections` —
this is specifically the append operator on a whole-array target.

## Note

Independent of DD#97: `emit_janet` renders from the AST (`emit_janet(program)`),
not from SSA, so the transform-side collection registration that DD#97 fixed
never reaches this path. Left failing rather than `skip_chk`'d so it stays
visible, consistent with the other janet-only failures.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
