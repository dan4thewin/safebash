---
title: "bless-all on the function `args` array ICEs (codegen/types.rs:188)"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`fn f { bless x in args [[ -n $x ]]; echo $1 }; f a` compiles and runs (no panic)"
  - "the fix covers `@args` splat after the bless too"
---

## Found

2026-09-05, implementing the `@args` external-splat arity relaxation (journal
245). A `bless x in args [[ … ]]` (bless-all over the function argument array)
panics in codegen:

```
thread 'main' panicked at sash/src/codegen/types.rs:188:28
```

Pre-existing (reproduces with the change stashed) and independent of the splat —
`bless x in args; echo $1` alone ICEs. Blessing an ordinary read array works
(`bless f in norm …`), so it is specific to the reserved `args` array as a
bless-all subject.

## Likely area

`codegen/types.rs:188` — resolving the array slot / SSA name for `args`. The
`args` array is the function-args model (DD#75/#81), which lowers differently
from a user array (positional slots vs a table); the bless-all retag path
(`BlessRetag`/`SsaInst::Bless`) likely looks up an `args` table slot that does
not exist in that form.

## Workaround

Function args are usually clean (DD#24: args from a clean caller are clean), so a
bless-all over `args` is often unnecessary. If a specific arg is tainted, bless
the scalar (`bless $1 …` via its binding) instead of the whole `args` array.

## Context

Found alongside the `@args`-external-splat arity relaxation (DD/journal 245),
but wholly separate.
