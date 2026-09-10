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
