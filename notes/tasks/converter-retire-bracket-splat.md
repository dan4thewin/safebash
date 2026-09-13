---
title: "sash-convert — rewrite retired `${a[@]}`/`${a[*]}`/`${#a[@]}`/`${!a[@]}` bracket forms to canonical DD#75 spellings"
status: done
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

## Note (2026-09-11)

`${names[@]}` currently draws a **wrong** MANUAL note — `${var@X} parameter
transformation not supported in sash — use ${var^^}/${var,,} for case`. The
expansion converter sees the `@` and reads it as the `@X` transformation
operator rather than the `[@]` subscript. Found while converting the DD#99
read family (a converted `mapfile` target is naturally iterated with
`${names[@]}` in the bash original). Whatever rewrites `[@]` here should also
stop that misfire.

## Done 2026-09-13 (journal 296)

`bracket_splat` + `convert_bracket_splat` in `sash-convert/src/convert.rs`.
Specs `57.93`–`57.101`. Round-trip verified: the converted output compiles
and is byte-identical to bash across all the runnable forms.

**One gate was stale.** It asked for `${#a[@]}` → `` `${#a}` (or `$(len a)`) ``
— but `${#a}` is itself retired (SB4038, the `${#…}` length sugar). The only
target is `$(len a)`, and the converter already produced it: `${#a[@]}`
reaches `length_expansion` before any bracket handling. Spec `57.101` pins
that, and was the one new spec that passed under fail-first.

**The replacement depends on context, and the three sash spellings are not
interchangeable** — `@a` is a splat legal only as a for-in source, `$(join a)`
is a scalar, and `$(keys m)` is a splat that cannot be joined at all (SB5025).
So:

| bash | context | sash |
|---|---|---|
| `"${a[@]}"` | for-in source | `@a` |
| `"${a[@]}"` | `echo` argument | `$(join a)` |
| `"${a[@]}"` | other command argument | MANUAL (SB5032) |
| `"${a[@]}"` | value / spliced in a string | `$(join a)` |
| `"${a[*]}"` | anywhere | `$(join a)` |
| `"${!m[@]}"` | for-in source, map | `$(keys m)` |
| `"${!a[@]}"` | for-in source, array | MANUAL (indices, no sugar) |
| `"${!a[@]}"` | anywhere else | MANUAL (SB5025) |

`echo` earns its exception honestly: it *rejects* a splat (SB5030) and
directs to a join, and a join is what bash's quoted form produces anyway. The
external-command case gets the same MANUAL wall `"$@"` already hits — bash
runs the command with zero arguments on an empty array and neither sash
spelling that states the empty case does that, so there is no faithful
mechanical answer (see `converter-splat-empty-case`, still open for DG).

**A wrong diagnostic was displaced.** `${!a[@]}` used to draw
`# MANUAL: ${!var} indirect expansion not supported in sash` — but it is not
indirect expansion, it is the keys form, and sash supports it for maps. Spec
`57.99` asserts the new note *and* the absence of the old one.

**Not done: `$a[@]` / `$a[*]`** (gates 1 and 2 name them). Those are not bash
array splats — bash reads `$a[@]` as `$a` followed by the literal `[@]`, and
tree-sitter agrees, emitting four sibling nodes. Converting them would mean
inventing a meaning bash does not give them.
