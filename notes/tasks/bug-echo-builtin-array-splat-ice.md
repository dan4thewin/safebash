---
title: "BUG — `echo @a` (inline builtin) ICEs on array splat; external argv path is fine"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`array a=(one two three); echo @a` compiles and prints the 3 elements (echo's normal space-join), no ICE"
  - "arg count is correct (echo receives N args, one per element); empty array → echo prints a blank line (zero args)"
  - "native + janet + c identical; spec test added; 0 regressions"
---

# BUG — `echo @a` ICEs (inline-builtin arg path misses ArraySplat)

Found 2026-07-04 while probing operand contexts.

## Repro

```
$ sash -c 'array a=(one two three); echo @a' -o /tmp/b
thread 'main' panicked at sash/src/codegen/values.rs:61:
ICE: ArraySplat (@a) must be expanded by the argv builder, not lowered as
     a scalar value
```

Contrast — the external argv path handles it correctly:

```
$ sash -c "array a=(one two three); printf '[%s]\n' @a" -o /tmp/b; /tmp/b
[one]
[two]
[three]
```

## Root cause

`array-splat-command-arg` (DONE, DG A1+B1+C+D 2026-06-21) taught the
**external** exec/argv builder to expand `SsaValue::ArraySplat` into N
runtime args. But the **inline `echo` builtin** (`lower_echo`,
`sash/src/codegen/commands.rs:103`) lowers its args through the ordinary
scalar value path (`lower_value` → `values.rs:61`), which hits the
`ArraySplat` panic backstop instead of expanding it. `echo`/`:`/`true`/
`false` run inline (not resolved to an external path — see AGENTS.md
"Pipeline builtins"), so they bypass the argv builder that got B1. `printf`
is resolved to an absolute path → external → works.

## Fix direction

`lower_echo` (and any inline builtin that forwards user args) must expand an
`ArraySplat` arg the same way the B1 argv builder / `lower_for_array`
(transform/pipelines.rs:33-172) do: loop the array `SashTable`
(`sash_table_len` bound, `sash_table_value` per element), emitting one
element per output arg, before the scalar-write path. Empty array → zero
extra args. Mirror in emit_c (`echo` C lowering) and emit_janet (`;`-splice
into the printed args), matching the external-path treatment already there.

## Relationship

Completes `array-splat-command-arg` (done) for the inline-builtin arg path
it missed. The `values.rs:61` panic is a correct backstop — the fix is to
never reach it for `echo`, not to soften the panic.
