---
title: "sash-convert — rewrite retired `${a[@]}`/`${a[*]}`/`${#a[@]}`/`${!a[@]}` bracket forms to canonical DD#75 spellings"
status: ready
priority: P3
kind: task
depends: [dd75-retire-bracket-subscripts]
gates:
  - "`${a[@]}` / `$a[@]` → `@a` (splat); a value context that needs a joined string → `$(join a)`"
  - "`${a[*]}` / `$a[*]` → `$(join a)` (the `IFS`-joined string; splat use → `@a`)"
  - "`${#a[@]}` / `${#a[*]}` → `${#a}` (or `$(len a)`)"
  - "`${!a[@]}` / `${!a[*]}` → `$(keys a)` for a map; for an indexed array the keys are `0..len-1` (emit a CHECK comment — sash has no index-range sugar)"
  - "unaffected: `${a[$i]}` / `${a[1]}` element access; `a[$i]=` element assign"
  - "round-trips through the compiler (converted output compiles clean, no SB4038)"
---

## Why

DD#75 §4 retired the `[@]`/`[*]` bracket-subscript collection forms in the
sash **compiler** (SB4038 at parse; commit on `hand-parser`). `@a` is the sole
splat. sash-convert still emits the retired bash forms verbatim when it lifts a
bash array, so converted scripts now hit SB4038.

This is the converter half of DD#75 §4 — split out per DG's standing rule that
converter questions are follow-ups (the compiler-side reject shipped first,
unreleased code → no migration risk).

## Scope

sash-convert (stock `tree-sitter-bash 0.25`). At each array-expansion site,
map the bash spelling to the canonical sash spelling per the gates above.

Open converter questions to raise with DG before implementing:
- `"${a[*]}"` (quoted star) is the single `IFS`-joined string → `$(join a)`.
  Bare `${a[*]}` (word-split) has no exact sash target — likely `@a` with a
  CHECK comment, mirroring the `${a[@]}` handling.
- Indexed-array `${!a[@]}` (numeric key list `0..len-1`): sash `keys` is
  maps-only (DD#75 §3). No index-range sugar exists — emit a CHECK comment and
  leave the original as an SB4038 compile-blocker to force review (same policy
  as converter-case-to-dispatch).
