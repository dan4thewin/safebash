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

## DD#77 Stage A — route arith `${…}` through the real param-expansion parser (DONE)

Fixes bug-arith-param-expansion-operator-zero (P1 silent miscompile):
`${var:OP…}` in `(( ))`/`$(( ))` swallowed the operator into a bogus var name
(`${s:-0}` → var `s:-0` → undefined → 0). `braced_body_token` (parse/arith.rs)
now: `${#name}`→LenOf and `${name[i]}`→Subscript (unchanged), a bare identifier
`${name}`→DollarVar (the working path), and **anything operator-bearing**→ new
`ATok::ParamExpansion(body)`. The arith parser routes that through the real
`parse_parameter_expansion` → `ArithExpr::Expansion` → `expr_to_value`. So
`${s:-N}`/`${s:?…}`/`${s#…}` are genuine expansions.

This is the bug fix / mechanism only — the DD#77 *disposition semantics* (strict
validity trigger, bare-string error, numeric-literal default) are Stage C.
Intermediate state: `${s:-N}` reads s / defaults on unset-empty (correct bash),
then lenient-atoi in arith (the remaining footgun Stage C closes).

Test-first: arithmetic.sh pex.1-4 (valid-int read, unset default, composes with
`+`, in a condition) — stable across Stage C (valid-int/unset results don't
change). native,c; janet's arith VALUE path reads 0 for an embedded `${…}`
(pre-existing janet-arith-subscript-read, generalized from subscripts to all
`${…}`; pex.1-3 guarded native,c, pex.4 condition-path works all modes).
Complexity: extracted `braced_length_token`/`is_bare_ident`. Full sweep: 0 real
passing→failing; native 0-fail.

## DD#77 Stage B — sash_is_int + `[[ -i EXPR ]]` (DONE)

Runtime `is_int_inner` (inner.rs) / `sash_is_int` (lib.rs): strict
integer-validity — the whole string must be an optional sign then decimal
digits, or a `0x`/`0o`/`0b` prefix with valid digits; empty/trailing-junk
(`42abc`) → false. Format-only (an overflowing but well-formed literal is
"valid"; DD#26 traps at conversion). Reuses `radix_prefix`. Unit test
(unit_tests.rs is_int_strict) + standalone-validated (the rt-test harness has
pre-existing link rot: `sash_streq`/`sash_arith_eval` absent from the lib —
unrelated).

`[[ -i EXPR ]]`: new unary test (bash has no `-i`, no collision). Wired in all
three backends — native (`lower_test_unary` → sash_is_int), c (emit_c extern +
call), janet (`_isint` preamble PEG matching the same grammar). DG's exact
example composes: `s=42abc; [[ -i $s ]] || s =~ s/\D//g; echo $s` → `42`.

Test-first: types.sh 77.1-77.7 (literal, non-numeric, set var, trailing-junk,
negative+radix, empty, DG validate-or-strip). All pass native+c+janet. Full
sweep: 0 real passing→failing; native 0-fail. clippy clean.
