---
title: "sash-convert: map bash arithmetic subscripts to sash explicit braced-index forms (DD#80)"
status: ready
priority: P3
kind: convert
depends: []
gates:
  - "bash `${a[i]}` (arithmetic subscript, bare var) converts to sash `${a[$i]}`"
  - "bash `${a[i+1]}` / `${a[expr]}` converts to `${a[$(( … ))]}`"
  - "bash `${a[a[0]]}` (nested) converts to `${a[${a[0]}]}`"
  - "no regression in convert.sh"
---

## Context

DD#80 tightened the braced subscript surface: `${a[K]}` takes a DIRECT key
(integer literal, `$var`, an expansion, a quoted string, or a clean-identifier
bareword — maps only). Bash instead evaluates every subscript arithmetically, so
bash idioms that sash-convert currently passes through verbatim now reject:

- `${a[i]}` (bare var) → sash **SB4030** (string key `i` on an int array).
- `${a[i+1]}`, `${a[a[0]]}` → sash **SB4041** (ambiguous/implicit index).

The converter has no subscript-specific handling today (it passes them through),
and `convert.sh` has no subscript coverage, so nothing regressed — but converted
scripts that use array subscripts will fail to compile until the converter emits
the explicit sash forms.

## Approach

In `sash-convert`, when rewriting a `${a[K]}` where `K` is a bash arithmetic
subscript:

- a lone identifier `i` → `${a[$i]}` (interpolate the var).
- an arithmetic expression `i+1` / `f(x)` → `${a[$(( … ))]}`.
- a nested subscript `a[0]` → `${a[${a[0]}]}` (braced) or `${a[$(( a[0] ))]}`.
- a numeric literal `5` → unchanged (`${a[5]}`).
- for a map, a bareword key stays a bareword; a non-identifier key gets quoted
  (`${m["content-type"]}`).

Add convert.sh coverage for each. Distinguish array vs map from the declaration
where possible; otherwise prefer the `$(( … ))` arithmetic form (safe for an
int-indexed array) and flag ambiguous cases for review (DD#66 converter policy).

## Provenance

DD#80 migration section. Filed during the DD#80 implementation (journal 171).
