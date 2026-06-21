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
