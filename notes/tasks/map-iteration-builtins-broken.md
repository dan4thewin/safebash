---
title: keys/values map-iteration builtins are broken (wrong output, taint hole, ICE)
status: ready
priority: P1
kind: bug
depends: []
gates:
  - keys on a clean map emits the actual keys (one per line), not blank/garbage
  - values on a clean map emits the actual values, no ICE
  - keys/values over a tainted (mapfile -m, unblessed) map is rejected SB5020, not compiled
  - mi.* spec tests in spec/argv.sh or spec/taint.sh lock all three above
---

# keys / values map-iteration builtins are broken

Found 2026-06-16 during the bless-shape negative-test audit (every blessed
spec shape must have a no-bless negative). `keys`/`values` are **not used in
any spec test**, so these defects were invisible. Three distinct defects:

## 1. `keys` emits garbage on a CLEAN map (correctness)

```
$ sash -c 'map m=(a 1 b 2); for k in $(keys m); do echo "k=$k"; done'
k=
k=
k=
k=          # four blank lines for a two-key map — wrong count and wrong values
```

Expected: `k=a` / `k=b` (two lines, the keys). Output is blank and the
cardinality is wrong (4, not 2). `keys` codegen / iteration is broken even on
clean data.

## 2. `values` ICEs on a CLEAN map (taint backstop fires on clean data)

```
$ sash -c 'map m=(a 1 b 2); for v in $(values m); do echo "v=$v"; done'
-c: ICE: tainted variable v_1 used as argument to command 'echo' —
    CST taint pass missed this
```

The map is clean literals, yet the SSA taint backstop marks the `values`
output tainted and the CST pass never saw it → ICE. Either `values` output
should be clean for a clean map (and the SSA classifier is wrong), or it is
genuinely tainted (DD-level decision — see below) and the CST pass must mark
it so it becomes a clean SB5020 instead of an ICE.

## 3. `keys`/`values` over a TAINTED map ESCAPES taint (security hole)

```
$ printf 'NAME=Alice\n' > km.txt
$ sash -c 'map m=(); mapfile -m m < km.txt; for k in $(keys m); do echo "k=$k"; done'
[COMPILES]   # should be rejected SB5020 — m is tainted, unblessed
```

A tainted map (populated by `mapfile -m`, no `bless k v in m [[…]]`) flows its
keys out through `keys` with no bless. `for k in $(keys m)` is a use site of
tainted-map data and must be rejected. (Contrast `values`, which ICEs at #2 —
inconsistent handling of the same hole.)

## Root cause (preliminary)

`check_expr_taint`'s `CommandSubstitution` arm now recurses into inner command
args (fixed `$(join @a)` this session), but `keys m` / `values m` take the map
name `m` as a **bare literal argument**, not `@m`. So the recursion sees the
literal `m`, not a tainted ParameterExpansion, and doesn't flag it. The
taint of `keys`/`values` **output** is dataflow-dependent on the map's taint —
it needs source-classification (like `cmdsub_is_scalar_taint_source`) that
knows "keys/values of a tainted map is tainted", plus a DD on whether keys are
even tainted (a key like `NAME` may be considered structural, the value `Alice`
clearly tainted — DG to decide). And `keys` codegen (#1) is independently
broken regardless of taint.

## Proposed spec tests (write first, TDD)

In spec/taint.sh (native-only, alongside the 60.x mapfile block):

- `mi.1` clean keys: `map m=(a 1 b 2); for k in $(keys m); do echo $k; done`
  → sorted/insertion-order keys, two lines.
- `mi.2` clean values: `map m=(a 1 b 2); for v in $(values m); do echo $v; done`
  → two lines, no ICE.
- `mi.3` tainted keys no-bless: `map m=(); mapfile -m m < f; for k in $(keys m); …`
  → reject SB5020 (`$rc!=0 && -z $out && $err==*SB5020*`).
- `mi.4` tainted values no-bless → reject SB5020.
- `mi.5` blessed map then keys/values iterate → accepted, correct output
  (negative/positive pair, per the bless-shape rule).

## Notes

- Needs a DD (or DD#41/DD#61 addendum) on the taint status of map keys vs.
  values before #2/#3 can be fixed correctly — present options to DG.
- #1 (`keys` garbage output) is a plain codegen bug and can be fixed first.
- Discovered alongside the join-on-tainted-array fix (commit pending) which
  closed the `$(join @a)` sibling of hole #3 for arrays.
