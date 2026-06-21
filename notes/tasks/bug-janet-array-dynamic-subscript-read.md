---
title: "BUG — janet backend: dynamic subscript read of an indexed array in value context returns empty"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "janet: `array a=(x y z); i=2; echo ${a[$i]}` prints `z` (and the non-brace `$a[$i]`)"
  - "drop the janet exclusion / un-fail spec types.sh 33.5 and 33.14"
---

# BUG — janet dynamic indexed-array subscript read (value context)

## Resolution (2026-06-21)

`emit_expr_subscript` emitted `(get a <idx>)` with the index untouched, so
a dynamic `${a[$i]}` became `(get a i)` where `i` is a string var — janet
`get` on an array with a string key returns nil → empty. Now it types the
index by collection kind (reusing `EmitCtx::map_vars` from
bug-janet-indexed-assignment): a map keeps its string key
(`emit_expr_inline`), an indexed array coerces to an integer
(`emit_expr_as_int` → `(scan-number i)` for a string var). Both brace
`${a[$i]}` and non-brace `$a[$i]` reads now work in janet (they share the
Subscript path), and the `[[ ${a[$i]} == … ]]` test case (62.14) too.

Verified vs native; un-guarded types.sh 62.14 (all modes). Also fixed a
pre-existing spec-hygiene bug: `chk 33.5/33.6/33.7` were each used twice
(dynamic read/write rows AND the array per-element-ops rows) — the latter
renumbered to 33.16/33.17/33.18, so test_status now reports the dynamic
rows accurately (janet:33.5/33.6/33.7 passing). janet 108→107; zero
regressions. (c brace `${a[$i]}` value read remains a separate c gap.)

## Symptom (janet backend only)

```
array a=(x y z); i=2; echo ${a[$i]}    # janet: empty   (native/c: z)
array a=(x y z); i=2; echo $a[$i]      # janet: empty   (native/c: z)
```

native and c both print `z`. janet prints empty. The **map** equivalent
works in janet:

```
map m=([a]=1 [b]=2); k=b; echo ${m[$k]}   # janet: 2  (ok)
map m=([a]=1 [b]=2); k=b; echo $m[$k]     # janet: 2  (ok)
```

So the gap is specific to **indexed arrays** with a **dynamic** index in
**value context**. Spec rows: janet:33.5 (brace) and janet:33.14
(non-brace) fail; janet:33.6/33.15 (map) pass.

## Likely cause

`emit_janet.rs` `emit_expr_subscript` (or its array path) does not resolve
a dynamic index against the janet array representation the way the map
path does — likely emitting a literal/identifier index or not coercing
`$i` to the integer used to index the janet array/tuple. Compare the
indexed-array branch to the working assoc/map branch and to the native
`Subscript` lowering.

## Discovery

Found 2026-06-21 implementing nonbrace-dynamic-subscript (decision B).
The new non-brace read produces the same `Subscript` IR as the brace
form, so it inherits this pre-existing janet gap (native + c pass).
Same root as the long-standing janet:33.5 failure.
