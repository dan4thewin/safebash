title: "P1 memory-safety: $(keys m) bypasses the SB5025 array-splat context guard (silent join + garbage bytes)"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "$(keys m) in command-arg / scalar position rejects with SB5025 (like $(argv)) — no silent space-join, no garbage output"
  - "for k in $(keys m) still splats correctly (the sanctioned splat context)"
  - "no compiled binary reads uninitialized memory for a keys splat (the `printf ==%s==\\n $(keys m)` garbage repro is gone)"
  - "0 native spec regressions; add a spec pinning the reject + the working for-in"

# `$(keys m)` bypasses the array-splat context guard (memory-unsafe)

## Discovery

Found 2026-08-11 (DG demo probe):

```
map m=([foo]=aa [bar]=bb); printf ==%s==\n $(keys m)
# → ==foo bar==   then GARBAGE bytes (%s walks a bad pointer)
```

`$(keys m)` is an array-returning splat, like `$(argv)`. `$(argv)` in a
command-arg / scalar position is rejected **SB5025** ("valid only as the sole
source of a for-in loop; snapshot into an array…"). But the guard
`check_argv_splat` (`sash/src/validate.rs:1772`) is hard-coded to `$(argv)`
and does **not** recognize `$(keys m)`. So keys slips through, is treated as a
scalar command-sub, and in arg position:

- best case: silently **joins** to one space-separated string (`"foo bar"`);
- worst case (unquoted format): a `%s` reads uninitialized memory → **garbage
  bytes**. A memory-safety escape in a *safe-subset* compiler.

## Behaviour matrix (verified)

| context | result |
|---|---|
| `for k in $(keys m)` | ✅ splats `foo`,`bar` |
| `@a`/`@m` → printf | ✅ SB5032 (DD#69) |
| `$(argv)` → printf | ✅ SB5025 |
| `$(keys m)` → printf/echo | ❌ silent join `"foo bar"` |
| `$(keys m)`, unquoted fmt | ❌ garbage / memory-unsafe |

## Fix

Generalize `check_argv_splat` to cover **every array-returning command-sub**
(`keys` as well as `argv`), so `$(keys m)` is legal only as a for-in sole
source (or array snapshot) and **SB5025** anywhere else. See DD#75 §3 / §7.

## Priority

P1 — memory safety in a safe-subset compiler is the project's core promise.
Independent of the DD#75 language work; fixable today.
