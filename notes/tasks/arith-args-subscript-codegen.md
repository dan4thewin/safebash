---
title: "bug: `$(( args[0] ))` (bare args subscript in arith) ICEs at codegen — masked by SB5031"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "`$(( args[i] ))` inside a function reads the positional as an int (or gives a clean error), not a codegen `undefined array: args` panic"
  - "no regression"
---

## Context

Found while fixing the undeclared-collection-read ICE (journal 172). The bare
arith cell read `$(( args[0] ))` inside a function is currently rejected by the
SB5031 undeclared-read check — a *false positive* (the implicit `args` array is
in scope in a function, per DD#75 §5), but a **protective** one: adding `args`
to the in-function scope set makes validate pass and then codegen panics
(`ICE: undefined array: args`, codegen/types.rs). So the arith path never
resolves `args` → the per-inline `__argsN` array the way the word path
(`${args[i]}`) and `resolve_args_name` do.

Word `${args[i]}` works; only the **arith** bare `args[i]` (and likely `$@`/`$#`
arith forms) is unresolved.

## Fix

Resolve `args` → `__argsN` on the arith-subscript lowering path (mirror
`resolve_args_name` / the word-context cell-read path), then drop the SB5031
false-positive for `args` in a function (add `args` to `check_function_scope`'s
`fn_known`). Add specs: `fn f { echo $(( args[0] + 1 )); }; f 5` → 6.

## Provenance

Uncovered by undeclared-collection-read-ICE fix (journal 172). The SB5031
masking is why it was never seen.
