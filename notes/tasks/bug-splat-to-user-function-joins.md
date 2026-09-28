---
title: "bug: `@a` splatted into a user function collapses to ONE joined argument"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`fn f { … }; array a=(x y z); f @a` gives `$(len args)` == 3, not 1"
  - "each element is its own argument (`for v in @args` yields x, y, z separately)"
  - "an element containing a space stays one argument"
  - "all of the above hold in a PIPELINE stage, not only a plain statement"
---

## What

`@a` splats to an external command but **joins** into a user function:

```sh
array a=(x y z)

/usr/bin/printf '[%s]' @{a:~}   # [x][y][z]   -- 3 arguments, correct
fn show { echo "n=$(len args)" }
show @a                          # n=1         -- WRONG, joined to "x y z"
show x y z                       # n=3         -- control
for v in @args { echo "<$v>" }   # <x y z>     -- one element, not three
```

So the same `@a` surface means "splat" for an external and "join" for a user
function, silently. An author who writes `f @a` and reads `$1` gets the whole
array as one string.

## Why it matters beyond arity

It is **lossy in a way the author cannot undo**: once joined on spaces, an
element that legitimately contains a space is indistinguishable from two
elements. And it is silent — no diagnostic at either the call or the use.

It also defeats DD#75's `args` model: `$(len args)` is the documented way for
a function to learn its arity, and it reports 1 for any splat call.

## Found

2026-09-10, while investigating `bare-collection-name-operand-asymmetry` — the
asymmetry question is about *naming*; this is a straightforward defect
underneath it and is independent of how that decision goes.

## Fixed 2026-09-10

**Broader than filed.** The array-literal path had it too:
`array b=(@a q)` built a 2-element array whose first entry was the joined
`"x y z"`.

**Cause — the transform, not codegen.** There are two argument lowerings:
`arg_to_value` (command-arg position → `SsaValue::ArraySplat`, one value per
element) and `expr_to_value`, which gives `@a` its *joined-string* meaning.
Both `inline_function_call` (cmd_handlers.rs:293) and `lower_array_literal`
(expressions.rs:412) used `expr_to_value`, so a splat never became an
`ArraySplat` in those positions. Externals were correct because they use
`arg_to_value`.

Both now use `arg_to_value`. Codegen additionally grew a splat arm in the
array builder — `emit_splat_append` (native) and the equivalent loop in
`emit_indexed_array` (c) — so an `ArraySplat` arriving as an array element
splices rather than falling to the single-value arm. Without that the
transform fix alone would have produced a table entry holding a table.

Specs `functions.sh` sp.1-sp.8: arity, order, an element containing a space
(the lossy case), splat in an array literal, splat between other elements, an
empty splat contributing nothing, plus the external and explicit-args
controls. sp.1-sp.6/sp.8 are gated native,c — janet does not resolve
`$(len args)` at all (`dd75-args-model-polish`), which the no-splat control
sp.8 demonstrates is unrelated to splatting.

Sweep: 4616 passing / 39 failing / 459 skipped, 0 regressions, no new
failures.

## Reopened and closed again 2026-09-28 — a third position

The 2026-09-10 fix was still incomplete. It named "two argument lowerings"
and converted the two call sites it knew about; **`build_function_pipeline_stage`
(`stmt_handlers.rs:896`) was a third**, and it kept `expr_to_value`. So the
identical bug survived one pipe away from the specs that gated it:

```sash
array two=("a b" "c")
fn f { : @args; echo "n=$(len args)" }
f @two            # n=2   -- correct, gated by sp.2/sp.5
f @two | /bin/cat # n=1   -- the SAME bug, ungated
```

Lossy in exactly the way this file describes: `<a b c>` for `for v in @args`,
with the element boundary destroyed and no diagnostic.

**Why the original fix missed it.** It was scoped by *symptom* — it fixed the
sites the reproducer reached, then generalised to "call args and array
literals" and stopped. The complete statement is per-position and mechanical:
**every position that lowers a call's arguments must use `arg_to_value` and
`split_call_args`**; `expr_to_value` there means the splat silently reverts to
its joined meaning and `=name` silently reverts to a scalar. Grepping for the
*wrong* function across all call-lowering paths finds this in one step;
re-running the reproducer does not.

There were four such positions. All four are now correct, and the same audit
found `=name` broken on three of them (DD#100 addendum, 2026-09-28) — a
collection parameter is lowered by the same code, so it had the same hole.

Specs `functions.sh` sp.9-sp.11 gate the pipeline position directly: arity,
the lossy space-bearing element, and order. journal 443.
