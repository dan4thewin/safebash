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

## Stage A follow-up — close the pruned-operator ICE hole (DONE)

Stage A routed ALL `${…}` bodies through parse_parameter_expansion, which
surfaced the DD#65-pruned operators (`:?`, `:=`, `:+`, `##`, `%%`, …) inside
arithmetic. Those normally SB4034-reject in word context via `check_rejected` →
`walk_all_exprs`, but that walk never descended arithmetic — so an embedded
`${s:?…}` bypassed the reject and hit the codegen `emit_string_op_expansion`
ICE ("unsupported parameter expansion operator"). A live regression on the
pushed branch (only `:-`/subscript/length were tested in Stage A).

Fix (ir.rs): `walk_expr_tree` gained an `ExprKind::Arithmetic` arm and
`walk_all_exprs` a `StmtKind::ArithTest` arm, both delegating to a new
`walk_arith_expansions` that descends the `ArithExpr` tree and visits each
`Expansion(inner)` via `walk_expr_tree`. So expr-level checks (the SB4034
reject, and any future ones) now apply uniformly inside `$(( ))`, `(( ))`, and
`if/while (( ))`. Extracted `walk_each`/`walk_each_pair`/`walk_each_stmt`/
`walk_opt` to keep walk_expr_tree under the complexity gate.

Tests: arithmetic.sh pex.5 (`$(( ${s:?} ))` → SB4034), pex.6 (condition
position). All 3 modes; valid forms (`${s:-9}`, `${a[i]}`, `${#s}`) still
compile. Full sweep: 0 real passing→failing; native 0-fail. This is the
mechanism cleanup; DD#77 Stage C will UN-prune `:?` as an arith disposition
with strict-int semantics.

## Follow-up (DG) — out-of-range for `-i` (test) and conversion (convert)

DG: "check out-of-range inputs for both test and convert." Two issues found:

1. **`-i` was format-only** — `[[ -i 9223372036854775808 ]]` (i64::MAX+1) → yes,
   but that value can't convert. DG: range-check `-i`. `is_int_inner` now uses
   `parse_strict_i64` (strict grammar + u64 magnitude + i128 range-check,
   honoring the i64::MIN asymmetry) — false for out-of-range. So `[[ -i $s ]]`
   means "valid AND representable"; a validated value never overflow-traps on
   conversion. All 3 backends: native/c via sash_is_int; janet `_isint` =
   `_fmtint` (PEG format, all prefixes) AND `int/s64` range-check (decimal/hex),
   with a format-only fallback for `0o`/`0b` (int/s64 can't parse those — a
   negligible untested edge).

2. **Convert overflow panicked** — `s=<overflow>; $(( ${s:-0}+0 ))` → `sash:
   panic`, rc 99 (atoi_inner `.expect`), vs arithmetic overflow's clean
   `<prog>: arithmetic overflow`, rc 2. Pre-existing bug. `sash_atoi` now uses
   `atoi_checked` (None on overflow) and routes to `sash_arith_overflow` — the
   same clean DD#25/#26 trap. atoi_inner delegates to atoi_checked (keeps its
   panic contract for the pure-fn overflow_tests).

Tests: types.sh 77.8 (MAX valid, MAX+1 not; all modes), 77.9 (validated → safe
convert; native,c), arithmetic.sh pex.7 (convert overflow → clean trap;
native), sash-rt-tests is_int_strict range cases. Full sweep: 0 real
passing→failing; native 0-fail. clippy clean.

## DD#77 Stage C1 — un-prune `:?` (word-context abort) (DONE)

`${var:?MESSAGE}` un-pruned everywhere (was SB4034, DD#65) as the `:-`/`:?`
abort/default pair. Word context: set/non-empty → the value; unset/empty →
abort `<prog>: MESSAGE` on stderr, exit 1 (mirrors the DD#69 splat `:?` abort).

- parse: dropped `:?` from is_dropped_param_operator.
- transform: `${s:?msg}` → new `SsaValue::RequireValue{var, message}` (message
  interpolates, like the `:-` default).
- runtime: `sash_require_value(s_len, msg_ptr, msg_len)` — abort-if-empty,
  reusing the splat abort's `<argv0>: WORD` pattern.
- codegen: native (call sash_require_value, pass through the value), c
  (extern + emit_require_value_c, arm moved to the aggregate helper for the
  complexity gate), janet (`emit_pe_require` — `(if (empty? v) (do (eprint
  argv0 ": " msg) (os/exit 1)) v)`).

Tests: var_modifiers.sh vm.req1 (set → value), req2/req3 (unset/empty → abort,
message native-only — janet's argv0 is the script path). vm.dr3 (`:?`→SB4034)
flipped/removed. arithmetic.sh pex.5 switched from `:?` to the still-pruned
`##`. All 3 modes; 0 real passing→failing; native 0-fail; clippy clean.

Intermediate state: arith `$(( ${s:?} ))` has WORD semantics for now (abort on
empty, atoi otherwise); C2 tightens the arith trigger to strict-int (abort on
non-integer).

## DD#77 Stage C2 — arith strict-disposition semantics for :- / :? (DONE, native+c)

In arithmetic int-context the `:-`/`:?` trigger is "not a valid integer"
(unset/empty/non-numeric/out-of-range), gated by the strict is_int:
`${s:-N}` → is_int(s) ? s : N ; `${s:?M}` → is_int(s) ? s : abort(M).

Runtime helpers (no cranelift blocks needed): `sash_atoi_or(s, default)` =
`parse_strict_i64(s).unwrap_or(default)`; `sash_atoi_or_abort(s, msg)` =
value or the `:?` abort. Native + emit_c `lower_value_as_int` gained
DefaultValue/RequireValue arms calling these (extracted to
lower_disposition_as_int / _c for the complexity gate; also moved
Subscript/IntCell/Var arms out to fit).

Fixes the footgun: `s=hello; $(( ${s:-9} + 0 ))` → 9 (was lenient atoi → 0);
`$(( ${s:?bad} + 0 ))` on a non-int → clean abort. The primary conversion
idiom `int n=$(( ${s:?M} ))` validates+converts. Out-of-range now routes to
the default/abort (not the atoi trap) — pex.7 switched to a bare `$(( s ))`
(the only remaining atoi-trap site until C3's D1).

Tests: arithmetic.sh dsp.1-5 (native,c). janet's arith VALUE path doesn't
apply the disposition (janet-arith-subscript-read, gate extended). Known
minor quirk: `echo $(( <lone ${…}> ))` uses string context (not int-forced) —
`int n=$(( … ))` and operator/condition forms are correct. Full sweep: 0 real
passing→failing; native 0-fail; clippy clean.
