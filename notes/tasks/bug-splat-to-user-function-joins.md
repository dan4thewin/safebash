---
title: "bug: `@a` splatted into a user function collapses to ONE joined argument"
status: ready
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
