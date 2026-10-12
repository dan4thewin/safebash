---
title: emit_c's var-slot fallback is wider than native's, and it is live
status: ready
priority: P2
kind: bug?
depends: []
gates:
  - explain all 20 exact-version misses in load_var_or_empty_c
  - decide per tier: legitimate, or a missing entry in the emit_c type map
  - the two backends make the same claim about a missing slot
---

# emit_c's var-slot fallback is wider than native's, and it is live

Discovered 2026-10-12 while making the native miss an ICE (DG's call).

## The asymmetry

`codegen/value_handlers.rs::load_var` looks up exactly one key,
`{name}_{version}`, and as of this session **ICEs** if it is absent. That is
sound: `setup::map_inst_versioned` maps every SSA version that defines a
variable, so a miss is a compiler fault.

`emit_c.rs::load_var_or_empty_c` has **three** tiers for the same question:

1. exact `(name, version)` in `VarTypes`;
2. else the **highest version** of `name`;
3. else the literal `("\"\"", "0")`.

Its doc claimed to "mirror `load_var_or_empty`" and to follow "codegen's
`find_var_slot` path". Neither was true — the native function has never had a
version-fallback tier; it was a bare `vars.get(&key)`.

## It is not dead code

From `coverage.db` at `aee328632af5` (the full spec sweep, both modes):

| line | tier | hits |
|---|---|---|
| 3028-3029 | exact version | 489 / 509 |
| 3030 | exact **missed** | **20** |
| 3034 | highest-version fallback **taken** | **8** |
| 3036 | empty-string sentinel | **12** |

Meanwhile `load_var`'s miss branch is hit **0** times in 232 calls.

So on the same corpus the C backend fails to find the named version 20 times,
and in 8 of those it emits the value of a **different SSA version** than the
IR asked for.

## Why this is not obviously a divergence (and why that is not reassuring)

The sweep is green in both modes, so wherever these tiers fire the resulting
program still matches bash. Three explanations, not yet distinguished:

- the sites are genuinely empty/unused in those programs;
- `VarTypes` is simply sparser than codegen's `vars` (it is a *type* map, not
  a *slot* map) and the highest version is always the right answer;
- there is a real divergence on a shape no spec covers.

The call counts differ (509 vs 232) because `load_var_or_empty_c` has a second
caller, `lower_value`'s `SsaValue::Var` arm (`emit_c.rs:3074`), which the
native function does not. Split the counts by caller first.

## Why it matters

`--emit-c` is the differential oracle. A tier that silently substitutes a
different SSA version is precisely the kind of thing the oracle is supposed to
*catch*, not *contain*. And the empty-string sentinel is the same silent shape
that `map_inst_versioned`'s comment records as having segfaulted once already.

## Approach

1. Instrument or probe to attribute each of the 20 misses to a caller and a
   source program. `contrib/cov-batch.sh` plus the branch lcov should do it.
2. For each: is the version the IR names genuinely absent from `VarTypes`, and
   if so why?
3. If the map is just sparse, populate it and delete tier 2 and tier 3 so both
   backends make the identical claim. If it is not, the gap is a bug.

## Do not

Delete the tiers before step 1. They are load-bearing today — 20 call sites
depend on them, and removing them blind turns a green sweep red with no
information about why.
