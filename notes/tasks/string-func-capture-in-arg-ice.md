---
title: "ICE: capturing a string function's stdout as a command argument (`echo $(greet)`)"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`echo $(greet)` (greet is a normal stdout function) either works (prints greet's stdout) or is a clean compile error — never an ICE panic"
  - "no `panic!(\"ICE: non-constant elidable function call not yet supported\")` reachable from user source"
  - "0 native spec regressions; specs across native/c/janet"
---

# `echo $(string_func)` ICEs (non-constant elidable function call)

## Discovery

Found 2026-08-04 while implementing Option C (int-func `$()` capture). This
is a **pre-existing**, separate gap (unrelated to int functions): capturing a
normal stdout function's output *directly as a command argument* panics.

```
greet() { echo hi }
echo $(greet)      # thread 'main' panicked: ICE: non-constant elidable
                   # function call not yet supported: greet
```

`s=$(greet); echo $s` (capture into a variable, then use) works — only the
inline arg-position capture ICEs.

## Root cause

`codegen/values.rs:472` (`lower_value` for `SsaValue::FuncCall`):
`panic!("ICE: non-constant elidable function call not yet supported: …")`.
A `FuncCall` (string-function stdout capture) in argument position reaches
codegen as a non-constant value the elidable-call lowering can't yet emit.
(Option C routes `-> int` functions to `IntFuncCall`, which now lifts into a
temp in arg position; the string `FuncCall` path has no such lift.)

## Fix (sketch)

Either lift a `FuncCall` argument into a captured temp in the transform (like
`lift_int_func_args` does for `IntFuncCall`) so codegen only sees a scalar, or
support the non-constant elidable call directly in `lower_value`. Whichever,
the codegen `panic!` must become unreachable from user source. Check the c and
janet backends for the same arg-position string-capture gap.
