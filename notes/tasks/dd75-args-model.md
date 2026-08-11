---
title: "DD#75 §5 — function args become the implicit `args` array (Model B); $1/$@/$# are operator sugar; $* removed"
status: ready
priority: P3
kind: task
depends: []
gates:
  - "function args live in an implicit, 1-indexed, reserved per-function array `args` (keys 1..n; index 0 unused)"
  - "$1..$n → ${args[1]}..${args[n]} (literal-index sugar); computed positional is ${args[$i]}"
  - "$@ → @args (splat); $# → ${#args} (= $(len args) = n); no bare $args scalar; `args` is reserved inside function bodies"
  - "$* is removed → $(join args); $0 stays the program name ($(arg 0)), not part of args"
  - "0 native/c/janet spec regressions; specs pin $1/$@/$# equivalences and the $* removal"
---

# DD#75 §5 — the `args` model (Model B)

Implements DD#75 §5. Resolves the `@` double-duty (splat operator in `@a` vs.
special-name in `$@`): after this, `@` is always the splat operator and `$@` is
sugar for `@args`. Unifies function args with named arrays and DD#60's
program-arg model.

`args` is **1-indexed** — the direct consequence of `$1 ≡ ${args[1]}` and
`$# ≡ ${#args}` both holding (so `${#args} = n = $#`). This moots the old
`array t=($@)`-into-array bug (use `@args` directly).

Functions are inlined and `$@` already lowers to the explicit `$1..$argc` list,
so `args` is that list presented as a 1-indexed table (DD#33). See DD#75 §5.
