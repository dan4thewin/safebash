---
title: "DD#77 Stage C — disposition discipline (:? un-prune, strict trigger, bare-string error)"
status: in-progress
priority: P2
kind: feature
depends: []
gates:
  - "C1: ${s:?msg} valid in word context (abort on unset/empty) and arith; no longer SB4034"
  - "C2: arith ${s:-N} = is_int(s)?atoi(s):N ; ${s:?msg} = is_int(s)?atoi(s):abort(msg) (strict trigger, all 3 backends)"
  - "C3 D1: a bare string var operand in arith → compile error directing to ${s:-N}/${s:?msg}"
  - "C3 D4: ${s:-N} default N in arith must be a numeric literal (static check)"
  - "C3 migrate: DD#72 arith_literals.sh bare-coercion → dispositions; amend DD#72"
  - "native+c+janet (or filed guards); spec tests; no regression"
---

# Staging (DG-directed 2026-08-17)

Design DD#77 (+ resolved sub-decisions). `:?` un-prune scope = everywhere
(mirror `:-`); D1 as approved (bare string in arith → error), DD#72
bare-coercion superseded (migrate + amend).

## C1 — un-prune `:?`
- parse: drop `:?` from is_dropped_param_operator (keep the rest pruned).
- transform: `${s:?msg}` → an abort-if-unset/empty SSA node (mirror
  DefaultValue for `:-`); message carried like DD#69 splat `@{a:?WORD}`.
- codegen native+c+janet: emit "unset/empty → <prog>: msg to stderr, exit 1"
  (DD#55 abort style).

## C2 — arith strict-disposition semantics
- arith int-context lowering of `:-`/`:?`: gate on sash_is_int(s).
  `${s:-N}` → is_int(s) ? atoi(s) : N ; `${s:?msg}` → is_int(s) ? atoi(s) : abort.
- 3 backends.

## C3 — D1 bare-string error + D4 numeric default + DD#72 migration
- type_check: a bare `ArithExpr::Var`/`DollarVar` referencing a String-typed
  var in arith → SB error (directs to `${s:-N}`/`${s:?msg}`). A `${…}`
  disposition (Expansion) is exempt.
- D4: `${s:-N}` in arith — N must be a numeric literal (else error).
- migrate DD#72 arith_literals.sh bare-coercion tests to `$(( ${x:?} + 0 ))`;
  amend DD#72 (bare-coercion superseded; success grammar still defines valid).

## Backends
- emit_c + emit_janet paths for the `:?` abort and the strict disposition, or
  per-mode guards + filed tasks.
