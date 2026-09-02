---
title: "converter: flag/rename a bash `local` that shadowed a global (DD#86 no-shadow)"
status: ready
priority: P3
kind: feature
depends: []
gates:
  - "sash-convert detects when a bash `local x` inside a function shadows a same-named top-level global and either renames it (with a MANUAL/CHECK marker) or emits a review comment — since sash (DD#86) forbids shadowing (SB5049)"
---

# Converter: shadowing-local detection (DD#86 Phase 5 refinement)

DD#86 Phase 5 landed the core converter change: bash `local` is dropped
(`local x=5` → `x=5`, `local -i x=5` → `int x=5`), because sash scope is
structural (a name first assigned in a function is auto-local).

The one DD#86 requirement not yet implemented: **a bash `local` that
*shadowed* a same-named global must be flagged/renamed**, because sash forbids
shadowing (a typed re-declaration of a global inside a function is SB5049, and
a bare write to a global name writes the global — so a bash local that meant to
be independent would silently clobber the global after conversion).

## What's needed

When converting `local x=…` (or `local -i x=…`) inside a function `f`:

- if `x` is ALSO declared/assigned at top level (a global), the dropped-keyword
  conversion changes meaning: bash gave `f` a private `x`; sash makes `f`'s
  `x=…` write the global. Detect this and either:
  - rename the local (e.g. `x` → `x_local` / `_x`) consistently within `f`, with
    a `# CHECK:` marker, or
  - emit a `# MANUAL:` comment directing the user to rename.

## Notes

- Requires a global-name set (top-level declarations/assignments) + per-function
  local tracking during conversion — the converter already walks function
  bodies (`convert.rs`).
- The common case (a `local` that does NOT collide with a global) is handled
  correctly today: the keyword is simply dropped.
- Related: `notes/decisions/DD86-variable-scope.md` (§Consequences / migration),
  journal 229.
