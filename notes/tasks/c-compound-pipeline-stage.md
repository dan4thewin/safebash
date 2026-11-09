---
title: "c backend: implement compound (group/for/if/function) pipeline stages"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "a `{ }` group / for / if / while as a pipeline stage runs in c and janet (matches native)"
  - "a user function as a pipeline stage runs in c and janet (`fn f { echo hi }; f | cat` -> hi)"
  - "then spec/pipe_functions.sh can drop its native-only guard and run in all three modes"
  - "no runtime `sash: bug: bare command` / janet `unknown symbol pN` from a compilable compound-pipeline program"
---

# c/janet compound pipeline stages are unimplemented

**Historical completed record.** The accepted command-group retirement
(DD70, journal 596) requires source group stages to reject; enforcement
belongs to `retire-command-groups`. The completed function-stage backend
support below remains required and must not be deleted with group syntax.

Surfaced 2026-07-05 while fixing `bug-function-in-pipeline` (functions as
pipeline stages). The native fix routes a function stage through the DD#14
`__compound_<id>` mechanism (the transform inlines the body into its own
SSA blocks; native codegen `inst_handlers.rs:153` forks and jumps to the
block). **Native works. c and janet have no `__compound_` handling at all.**

## Repro (a plain compound stage — not even a function)

```
$ echo hi | for x in a; do cat; done          # native: prints hi
```

- **c** (`sash --emit-c`): the `__compound_<id>` stage name falls through to
  `emit_pipeline_exec` (`emit_c.rs:2077`) → `sash_exec("__compound_5", …)` →
  runtime `sash: bug: bare command passed to sash_exec`.
- **janet** (`sash --janet`): emits `unknown symbol p1` (compile error) —
  the janet pipeline lowering doesn't model compound stages.

Because functions-in-pipeline reuses `__compound_`, functions have the same
c/janet gap. `spec/pipe_functions.sh` is therefore **native-only guarded**
(like `brace.sh` / `convert.sh`).

## What's needed

- **emit_c**: handle a `__compound_<id>` stage name in `emit_pipeline_stage`
  (`emit_c.rs` ~2003-2071) — emit the compound block's body in the forked
  child instead of exec'ing the name. Requires emitting the referenced SSA
  block (`ssa.compound_pipeline_blocks`) as C in the child branch.
- **emit_janet**: same for the janet pipeline lowering — run the compound
  block's statements in the child rather than treating the stage as an
  external command.
- Then drop the native-only guard in `spec/pipe_functions.sh` and add the
  group/for/if compound-stage cases for all three modes.

## Relationship

- Sibling to `bug-function-in-pipeline` (native half — done). This is the
  c/janet half of the same DD#14 compound-pipeline-stage story.
- Related pre-existing c/janet backend gaps: `cf-backend-gaps-brace-surfaced`.


## Rescoped to c-only (DD#104, 2026-09-12)

The janet backend is removed. This task was filed against both
backends; the **c** half is still real and still open — it shows up
as one of the c-mode skips. The janet half is gone, not fixed.


## Completed 2026-09-13

The c emitter now mirrors native. `emit_pipeline_stage` (`sash/src/emit_c.rs`)
falls through to `emit_pipeline_exec` only when the stage name is a real
command; a `__compound_<id>` name instead emits `goto _blk_<id>;` inside the
forked child. That works because the c backend already lays every SSA block
down as a `_blk_<id>:` label in the same function, so native's
`builder.ins().jump(compound_cl, &[])` (`codegen/inst_handlers.rs:170`) has an
exact one-line analogue — the whole fix is three lines.

Before: `sash_exec("__compound_2", …)` → `sash: bug: bare command passed to
sash_exec`, rc 127 for a function stage and 141 for a `{ }` group.

Verified at parity with native across function-as-first-stage,
function-as-second-stage, `{ }` group, loop-inside-function, two adjacent
compound stages, compound-in-the-middle-of-three, nonzero exit propagation,
and splat argv on either side of the compound stage (the `goto` jumps *out of*
the child's block rather than into a VLA scope, so the
`bug-emit-c-splat-vla-jump` hazard does not apply).

`spec/pipe_functions.sh` is un-gated as a result: 11 existing tests plus
`pf.12`–`pf.15` now run in both backends. Fail-first: reverting `emit_c.rs`
fails all 15 in c and none in native.
