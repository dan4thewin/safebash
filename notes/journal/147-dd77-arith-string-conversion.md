---
title: 147 — DD#77 arith string→int conversion discipline (+ point-1, running log)
date: 2026-08-17
---

# 147 — DD#77 + point-1 (running commentary)

Unattended SOP (journal 115/132/145): ready backlog in dependency order,
fix blocking bugs inline, file non-blocking items, test-first per part,
brief log. This session's design conversation ratified two things:

- **Point 1 (DD#30 refinement):** a bare numeric literal to an already-int
  LHS is accepted (`int x=0; x=5` aligns with `int x=5`), incl. `+=` and
  DD#76 int-collection cells. Original asymmetry: init accepted bare
  numerics, reassignment didn't.
- **DD#77:** the string→int boundary in `(( ))`/`$(( ))` must carry an
  explicit failure disposition (`${s:-N}` default / `${s:?msg}` abort,
  borrowing DD#69 splat grammar); bare string operand → compile error;
  strict validity; numeric-literal default; plus a `[[ -i … ]]` test. This
  subsumes a P1 silent-0 bug (bug-arith-param-expansion-operator-zero).

Baseline (HEAD 9a823ea): native 0 failing; c/janet at their DD#76 baselines.

## Point 1 — known-int literal reassignment (DONE)

`check_assignment` (error pass) + `infer_stmt`/`infer_assignment_type`
(codegen type map) both gained the exception: an already-int LHS accepts a
bare numeric literal and keeps int (via `is_numeric_literal`, the same gate
declaration-init uses). Scalar reassign, `+=`, and int-collection cells
(`a[1]=99`).

**Blocking bug fixed inline (janet):** scalar `+=` was a pre-existing silent
miscompile — `emit_assignment` ignored `a.op`, emitting `(set x v)` for both
`=` and `+=`. So `s=ab; s+=cd` → `cd` (not `abcd`), untested so unnoticed.
Fixed to accumulate (`(+ x v)` int / `(string x v)` string), mirroring
`emit_indexed_assignment`. Exposed because point-1 newly allows
`int x=4; x+=3` (was a reject → never reached janet before).

Test-first: types.sh 30.5-30.11 (reassign, negative, `+=`, int-cell,
boundary `x=5+1`→reject, new-var-stays-string, string-`+=`). DD#76 `ivc.10`
flipped from "bare `a[1]=99` → SB4030" to "→ 99" (point-1 overturns its old
premise). native+c green; janet green except int-collection cells (30.8,
ivc.10 → skipped, janet-int-valued-collections). Full sweep: 0 real
passing→failing; 8 new passing; native 0-fail. clippy clean. DD#30 amended.
