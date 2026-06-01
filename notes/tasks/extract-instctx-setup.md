---
title: Extract InstCtx setup boilerplate from codegen handlers
status: pending
depends: []
gates: []
---

# Extract InstCtx setup boilerplate from codegen handlers

`block_dupes` (indexnudge v7) surfaces the largest extraction
candidate in the codebase, all in `sash/src/codegen/`:

| occurrences | shape |
|---|---|
| 23 | handler signature: `builder: &mut FunctionBuilder, module: &mut ObjectModule, ctx: &mut InstCtx,) -> bool {` |
| 21 | signature + first setup line |
| 16 | the four-line setup `let vars = &mut *ctx.vars; let literals = ctx.literals; let externs = ctx.externs; ...` |

23 handlers all open with the same four-line dance to destructure
`InstCtx` into local mutable borrows. The boilerplate predates the
`InstCtx` struct itself (which already exists to bundle these refs)
and reads like the result of mechanical refactoring that didn't go
all the way.

## Approach options

1. **Macro** (`destructure_ctx!(ctx => vars, literals, externs)`).
   Cheapest. Each handler keeps its three local bindings, but
   the four-line ritual collapses to one.
2. **Method on `InstCtx`** returning a tuple.
   `let (vars, literals, externs) = ctx.borrow_parts();`
   Same effect, more discoverable than a macro.
3. **Pass the parts directly.** Change the handler signature so
   the parts arrive split. Touches the dispatcher; biggest blast
   radius but cleanest end state.

(2) is probably the right call — discoverable from the type,
no macro mystery, one-line per handler.

## Locating callsites

```bash
sqlite3 .indexnudge-rust.db "
  SELECT occurrences, sites FROM block_dupes
  WHERE shingle LIKE '%let vars = &mut *ctx.vars%'
  ORDER BY occurrences DESC LIMIT 5;"
```

## Why now

This is the top duplication hotspot in the codebase by a wide
margin. Doing it shrinks every codegen handler, makes new handler
additions follow a one-line pattern, and demonstrates the
`block_dupes` workflow for future refactors.

## Discovered

2026-05-31, via indexnudge v7 `block_dupes` view (see
`INDEXNUDGE_UPDATES_2026-05-31.md` from the indexnudge agent).
