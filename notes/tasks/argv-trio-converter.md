---
title: argv/arg/argc trio — sash-convert updates
status: done
priority: P0
kind: feature
depends: [argv-trio-codegen]
gates:
  - top-level $@/$* → $(argv); inside-function $@/$* preserved verbatim
  - top-level $1..$N → $(arg N); inside-function $1..$N preserved verbatim
  - top-level $# → $(($(argc) - 1)); inside-function $# preserved
  - top-level $0 → $(arg 0)
  - quoted array splats `"${arr[@]}"` → bare `@arr`
  - spec/convert.sh extended with conv.* test cases for each form
  - no regressions in existing convert tests
---

# argv/arg/argc trio — sash-convert updates

sash-convert produces sash-native source from bash. After T2 lands
the new validator rules, the converter's existing output starts
failing to compile because it emits forms the new validator rejects.

Required changes are mechanical but must respect DD46 — only
**top-level** bash positionals get rewritten; inside-function forms
stay as-is (function-local clean per DD46).

## Current state

`sash-convert/src/convert.rs:620,651,684` already rewrites:
  - `$@`, `$*`     → `$(argv @)`  (BUG: needs DD46 context check)
  - `${@}`, `${*}` → `$(argv @)`  (BUG: same)
  - `$1..$N`       → `$(argv N)`  (likely; verify)
  - `$0`           → `$(argv 0)`  (likely; verify)

Two problems:
  1. The target form `$(argv @)` no longer exists in DD60 — needs
     `$(argv)` (whole) or `$(arg N)` (scalar) depending on intent
  2. The rewrite is context-blind — converts inside-function forms
     that should stay as `$@`/`$1` etc.

## Required changes

### A. Target-form rewrite

| Bash form           | sash-native target |
|---------------------|--------------------|
| `$@` (top-level)    | `$(argv)` (in splat context) |
| `$*` (top-level)    | `$(argv)` |
| `${@}` (top-level)  | `$(argv)` |
| `$1`..`$N` (top-level) | `$(arg N)` |
| `${1}`..`${N}` (top-level) | `$(arg N)` |
| `$#` (top-level)    | `$(($(argc) - 1))` (subtract program-name slot) |
| `$0` (top-level)    | `$(arg 0)` |
| `${#}` (top-level)  | `$(($(argc) - 1))` |
| any positional (inside-function) | preserve verbatim |

### B. Context awareness

Walk the bash AST tracking "are we inside a function body". Apply
the rewrite only when the answer is no. This requires the converter
to track function-body nesting depth.

If the converter is currently single-pass and stateless on this,
adding the nesting tracker is the main implementation work.

### C. Quote-unwrap on array splats

`"${arr[@]}"` (bash idiom for safe array splat) → `@arr` (sash-native,
the canonical splat spelling per DD#41 / argv-trio-design Decision 7).

Today the converter likely passes this through verbatim — verify
and add the rewrite if needed.

### D. Splat-context awareness

`$(argv)` is splat-context only (DD60). The converter must place it
only in for-in headers and array-RHS positions. If the bash source
has top-level `"$@"` in a command-arg position (e.g.,
`some_cmd "$@"`), there's no clean sash-native equivalent — the user
has to write `snapshot+iterate` or `snapshot+pass-array`. Decide
how the converter handles this case:

  1. Emit a comment + manual-edit marker (`# TODO(sash-convert):
     replace $@ command-arg splat`)
  2. Emit a compile error from the converter ("cannot mechanically
     convert; requires snapshot")
  3. Auto-emit a snapshot + bless boilerplate

Recommend option 2 — fail loud with guidance. Mechanical conversion
silently producing non-compiling sash-native is worse than a
converter error.

## Test gates

Extend `spec/convert.sh` with one test per row in the table above,
plus:

  - conv.f1: function with `$@` inside body — converter preserves verbatim
  - conv.f2: function with `$1` inside body — converter preserves verbatim
  - conv.f3: function with `$#` inside body — converter preserves verbatim
  - conv.q1: bash `"${arr[@]}"` → sash `@arr`
  - conv.x1: top-level command-arg `cmd "$@"` → converter error (option 2)

## Sites likely affected

  - `sash-convert/src/convert.rs:620-690` — main positional rewrite block
  - `sash-convert/src/` walker module — needs in-function-body flag if
    not already present
  - tests under `sash-convert/tests/` — extend coverage

## References

  - [argv-trio-design](argv-trio-design.md) — DD60 + DD46 update
  - DD46 (positionals function-local — converter must honor)
  - `sash-convert/src/convert.rs:620,651,684` — current rewrite sites
