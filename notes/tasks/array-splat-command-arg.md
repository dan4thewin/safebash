---
title: "@a array splat in command-arg position expands to N verbatim args, not one joined string"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "@a passed as command arguments expands to one argument PER element (verbatim — no word splitting within an element), matching for-in element semantics; empty array → zero args (not one empty arg)"
  - "arg boundaries verified (e.g. an arg-boundary-visible consumer shows one element per arg, not a single space-joined arg)"
  - "works in native, janet, and c backends"
  - "spec tests added; native/janet/c no net regression; complexity gate green"
---

# `@a` command-arg splat → N verbatim args (bug)

## Symptom

`@a` (array splat) used as command arguments collapses to a **single
space-joined string arg** instead of one arg per element:

```
array a=(one two three)
printf '[%s]\n' @a      # prints:  [one two three]   (WRONG)
                        # want:    [one]\n[two]\n[three]
```

Confirmed 2026-06-20 (native). `echo @a` similarly passes one arg
`one two three`. Contrast: `for x in @a` iterates per-element correctly,
and bash `"${a[@]}"` expands per-element.

## Root cause

Command args are lowered one-`SsaValue`-per-`Expr` via `expr_to_value`
(transform/stmt_handlers.rs:205–216, :461, …). A `@a` word becomes a
*single* `SsaValue` (the array joined to a string), so the exec path
receives one arg. The for-in path has dedicated splat handling
(`detect_array_iter`, `expand_dollar_at_words`, control_flow.rs) that the
command-arg path lacks. `@`/`*` splat is recognised at cmd_handlers.rs:444
(`v == "@" || v == "*"`) only for the `$@` cases, not array `@a`.

## What "correct" means (DD#22 no-word-splitting)

The safe splat is **N elements → N args, each verbatim**: no splitting
*within* an element (a filename with spaces stays one arg), but still N
separate args. Empty array → zero args.

## Implementation note (non-trivial)

The command's arg list is built at compile time as `Vec<SsaValue>`, but
`@a` expands to a **runtime** number of args. The exec/argv-construction
path (codegen `lower_exec` / the fork-exec argv build, and the janet/c
twins) must splice a runtime-sized array's elements into argv. This is a
real codegen feature, not just a transform tweak — argv becomes
"build from a mix of fixed SsaValues and a runtime array span."

## Why first

Prerequisite for `glob-as-array-splat-disposition` (the `@{a:-…}` /
`@{a:?…}` forms and the external-splat rule all assume `@a` already
expands per-element). Land this standalone.

## Cross-refs

- Discussed 2026-06-20 alongside the glob-as-array reframe.
- for-in splat precedent: transform/control_flow.rs `detect_array_iter`.

## Implementation approach — investigated 2026-06-21 (needs DG sign-off on B)

Feasibility confirmed: `sash_alloc(n)` exists (sash-rt/src/lib.rs:575),
arrays are integer-keyed `SashTable`s with `sash_table_len`
(lib.rs:1136) and `sash_table_value` (lib.rs:1196), and `lower_for_array`
(transform/pipelines.rs:33-172) already does Cranelift array iteration
(arrlen bound + `TableValue{array,index}` per element) — the exact
pattern an argv fill-loop would reuse. No fundamental blocker.

**A. SSA/IR representation** (recommend A1):
- A1: new `SsaValue::ArraySplat { array: String }`. Command args stay
  `Vec<SsaValue>` but may contain a splat; each backend's argv build
  expands it. Fits the existing array variants (IndexedArray/TableValue).
- A2: restructure the command SsaInst into a fixed/splat "arg plan".

**B. Native runtime-sized argv — the architecturally significant choice:**
- B1 (codegen-side): compute argc at runtime (`1 + #scalars +
  Σ sash_table_len(splat)`), `sash_alloc((argc+1)*8)`, fill with Cranelift
  loops (scalars direct, splats loop `sash_table_value`). No ABI change;
  more Cranelift in `lower_exec` (codegen/commands.rs:103-169). Note: the
  current argv is a fixed stack slot — B1 moves it to a heap alloc.
- B2 (runtime-side): new exec ABI — codegen passes an arg descriptor
  (scalars + array table ptrs + splat flags) and sash-rt builds argv
  (e.g. `sash_arglist_new/push/push_array` + `sash_run_list`). Less
  Cranelift, but a new runtime ABI surface that the pipeline/fork-exec
  paths must also adopt.

**C. C backend** (emit_c.rs:601-693): build argv as a dynamically-sized
list (count then fill, or a small emitted helper); arrays are `SashTable*`.

**D. Janet backend** (emit_janet.rs:678-693): use janet's `;` splice in
the `[...]` command array: `(os/execute [cmd ;(elems-of a) …] :p)`.

DG: B1 vs B2 sets the runtime ABI direction (and whether pipelines/
builtins paths change). Recommend A1 + B1 + C + D (smallest blast radius,
reuses sash_alloc + the for-in iteration pattern, no ABI churn). Holding
for your call on B before implementing, per decision authority — this is
the runtime-architecture choice, not the (already-decided) semantics.

## Decision (DG, 2026-06-21): A1 + B1 + C + D

- **A1** — new `SsaValue::ArraySplat { array: String }`; command args stay
  `Vec<SsaValue>` and may contain a splat each backend expands.
- **B1** (native) — **codegen-side** runtime argv: compute argc at runtime
  (`1 + #scalars + Σ sash_table_len(splat)`), `sash_alloc((argc+1)*8)`,
  fill with Cranelift loops (scalars direct; each splat loops
  `sash_table_value`), null-terminate, pass to `sash_run`. **No runtime-ABI
  change**; argv moves from the fixed stack slot to a heap alloc. Reuses
  the `lower_for_array` iteration pattern (transform/pipelines.rs:33-172).
- **C** (emit_c) — build argv as a dynamically-sized list (count then
  fill); arrays are `SashTable*`.
- **D** (emit_janet) — `;`-splice into the command array:
  `(os/execute [cmd ;(elems-of a) …] :p)`.

Implementation still owns: the empty-array → zero-args case, DD#69's
empty-disposition forms (`@{a:-W}`/`@{a:?W}`) which build on this, and
verifying arg boundaries across all three backends. SB/validation for the
DD#69 external-splat rule is the separate `glob-as-array-splat-disposition`
task that depends on this one.
