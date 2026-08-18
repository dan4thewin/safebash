---
title: "janet: an undeclared arith counter `(( f++ ))` emits `unknown symbol f`"
status: ready
priority: P3
kind: bug
depends: []
effort: effort-janet-backend-parity
gates:
  - "janet `(( f++ ))` on an undeclared `f` auto-declares it (starts 0 → 1), matching native/c"
  - "un-guard arithmetic.sh b2a.3 for janet"
  - "no janet regression"
---

## Symptom

`(( f++ )); echo $f` on an **undeclared** `f`:

- native / c: `f` auto-vivifies (0 → 1), `$f` = 1.
- janet: `compile error: unknown symbol f` — the emitted janet references `f`
  before any `(def/var f …)`.

The sash compile is correct in all modes (B2 / SB5031 treats `f` as in-scope
because the `++` assigns it — arith-subscript model B). Only the janet *emit*
fails to declare the auto-vivified counter.

## Context

B2 (SB5031 extended into arithmetic, DD#79) lets `(( f++ ))` stand on an
undeclared name because the mutation assigns it. native/c auto-declare the
underlying slot; janet's `emit_janet` does not emit a `(var f 0)` for a name
first seen as an arith mutation target.

Guarded: `spec/arithmetic.sh` b2a.3 is native,c. Fix: in `emit_janet`, when a
scalar arith mutation (`(( f++ ))` / `(( f = … ))` / `(( f += … ))`) targets a
name not yet declared, emit its declaration first (default 0).

## Provenance

Found during arith-subscript Stage 2 + B2 (journal 150).
