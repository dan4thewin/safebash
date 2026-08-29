---
title: "bug: `$(( args[0] ))` (bare args subscript in arith) ICEs at codegen — masked by SB5031"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "`$(( args[i] ))` inside a function reads the positional as an int (or gives a clean error), not a codegen `undefined array: args` panic"
  - "no regression"
---

## RESOLVED (2026-08-29)

Two coupled fixes, exactly as planned below:
- **transform** (`transform/expr_handlers.rs` `subscript_read_to_ssa`): resolve
  `args` → the per-inline `__argsN` slot via `resolve_args_name`, mirroring the
  word cell-read path (`subscript_to_value`). The array name (and the
  `assoc_arrays`/`type_map` type lookups) now use the resolved name, so `args`
  no longer reaches codegen unresolved and the defensive `find_array_slot` ICE
  (codegen/types.rs) stays unreachable.
- **validate** (`validate.rs` `check_function_scope`): add `args` to the
  per-function `fn_known` set (DD#75 §5 — the implicit positional array is in
  scope in every function body), dropping the SB5031 false positive.

Verified: `f(){ echo $(( args[0]+1 )); }; f 5` → 6 (native+c). The pre-existing
positional rules apply uniformly to arith args now (they were fully masked
before): non-contiguous `args[1]` → SB5044, over-arity → SB5043 — same as the
word form `${args[i]}`. Top-level `$(( args[0] ))` still SB5031 (positionals are
function-only); a real typo `$(( notavar[0] ))` still SB5031. janet's arith
VALUE path still can't read the embedded subscript (pre-existing
`janet-arith-subscript-read`), so the positive specs are native,c (like the
sibling `bsx.*`). specs arithmetic.sh bsx.8/9 (positive) + bsx.10 (top-level
reject). 0 regressions native/c/janet.

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
