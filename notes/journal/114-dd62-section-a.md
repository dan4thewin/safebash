---
title: 114 — DD#62 section A: [[ ]] operands → SsaValue (kills lower_subscript)
date: 2026-06-20
---

# 114 — DD#62 section A: `[[ ]]` value operands → `SsaValue`

DG: "continue / push and continue / no risk, continue." Landed the
operand→`SsaValue` refactor that was the remaining piece of DD#62. This
is the change that fixes the `[[ ${m[$k]} ]]` map-dynamic-key bug
(`dynamic-map-subscript-in-test`) and deletes the duplicate test-path
codegen subsystem.

## What changed

**New SSA repr** (`ssa.rs`): `SsaTest` / `SsaTestRhs`. `SsaInst::Test`,
`Bless.test`, `BlessAll.test` now carry `SsaTest` (was raw `ir::TestExpr`).
Value operands are pre-lowered `SsaValue`; only `=~`/`=glob` *pattern*
RHS stays raw text (`SsaTestRhs::Pattern`).

**Transform** (`transform/expressions.rs`): `test_to_ssa(&TestExpr,
state) -> SsaTest`. Value leaves via `expr_to_value` (so subscripts use
`subscript_to_value`'s `assoc_arrays`-correct key-typing); `=~`/`=glob`
right → `Pattern(expr_to_string)`. Applied at all 7 `Test` sites +
`Bless`/`BlessAll`. Case dispatch (`build_dispatch_chain`) pre-lowers the
subject once and synthesises `=glob` `SsaTest`s.

**Codegen** (`codegen/tests.rs`): `lower_test_expr` consumes `SsaTest`,
threads `func_constants`; operands lower via `super::values::lower_value`
(values) / `lower_literal_value` (patterns) / `lower_value_as_int`
(numeric). The C backend (`emit_c.rs`) mirrors it via `lower_value_as_str`
/ `lower_value_as_int`.

**Deletions** (the payoff): the whole IR-`Expr` test-lowering subtree —
`lower_expr_value`, `lower_subscript` (the key-type-guessing culprit),
`lower_concat`, `lower_expr_value_as_int`, `lower_int_fallback`,
`lower_simple_expansion`, `lower_literal`, `empty_value`,
`lower_int_simple_expansion`, `try_lower_int_arrlen` (codegen/values.rs);
`intern_test_ir_expr` (setup.rs); `find_var_slot`/`find_var_key`
(types.rs, only the deleted `lower_subscript` used them);
`latest_var_as_int`/`ir_expr_as_int` (emit_c.rs); `next_slot_version`
(dispatch.rs). ~330 lines net.

## The bug it fixes

`codegen/values.rs::lower_subscript` guessed map-vs-indexed key type from
index *syntax* (a `$k` subscript → integer key), so `[[ ${m[$k]} ]]` on a
map did an integer-keyed `table_get` → miss → empty. The SSA value path
(`subscript_to_value` → `lower_subscript_value`) carries the real
`is_integer_key = !assoc_arrays.contains(name)`. Pre-lowering operands to
`SsaValue` routes `[[ ]]` through it. **Fixed in native *and* C.** Closes
`dynamic-map-subscript-in-test`. Specs `types.sh` 62.11–62.15.

## Three bugs the refactor introduced and I then fixed

These were caught by the spec/rust suites before commit — none shipped.

1. **Synthetic numeric tests ICE'd.** I dropped codegen's numeric-op
   path on the (wrong) assumption SB5019 makes `-lt`/`-eq`/… dead. SB5019
   only rejects *user-written* numeric ops; the for-in lowering
   synthesises `[[ __ai -lt ${arr arrlen} ]]` bound checks that bypass
   validation. Restored `numeric_cc` + a numeric arm (operands via
   `lower_value_as_int`) in both backends. `emit_c::lower_value_as_int`
   fallback now lowers-as-string + `sash_atoi` so `arrlen` works.

2. **Scalar `bless x [[ … $x … ]]` validated the target, not the
   source.** `test_to_ssa` ran *after* `state.define(var)`, so `$x` bound
   to the new clean version — which `emit_bless` hasn't populated yet when
   it evaluates the predicate → empty → abort. Fixed: resolve the
   predicate *before* `state.define` (both stmt- and if-form). The old
   raw-`TestExpr` path dodged this because codegen resolved by
   highest-present version (= source) at eval time.

3. **Bless-all binders mis-resolved.** `bless x in a [[ -n $x ]]`:
   transform bound `$x`→`x_0` (loop-local binders aren't in `state`, so
   `lookup` returns 0), but codegen created the binder slot at
   `next_slot_version` (e.g. `x_4`, dodging the unrelated for-loop's
   `x_0`). `lower_var_value` resolves by *exact* versioned key, so they
   missed. Fix: `BlessAll.binders` is now `Vec<VarRef>` — transform
   `state.define`s each binder (globally-unique version), resolves the
   predicate against them, then `restore`s (binders stay loop-local; the
   counter isn't rewound so versions stay unique); codegen uses the
   binder ref's own version for the slot key. Indexed + map bless-all
   both green.

## Verification (zero net regression)

- **Native spec** back to baseline: only `cf.c7`/`cf.c8` (pre-existing
  case-glob). 5 new tests (62.11–62.15) green.
- **C** spec: zero new failures (2 improvements). **Janet**: zero new
  failures (3 improvements). Janet still lacks dynamic *indexed* read in
  `[[ ]]` (its `[[ ]]` lowers from IR `TestExpr`, not the SSA operand
  path) — 62.14 is guarded `in_modes native,c`; consistent with the
  existing `33.5` janet gap.
- **Rust suite**: 186-failing set unchanged vs baseline (the lone diff,
  `oracle_pipeline_grep`, is the known full-suite flake — passes 3/3 in
  isolation; `test_v_unset` now passes).
- `cargo fmt`/`clippy --tests -D warnings` clean; complexity gate green.

## Notes

- DD#62 **C/D** (`=~` quoting/spaces + the `a[[:space:]]b` `]]`-lexing
  bug) still fold into **DD#63** (which reworks `parse_regex_rhs` /
  desugars `=~`). The `=~`/`=glob` RHS stays raw `Pattern` text here.
- Janet `[[ ]]` indexed-dynamic-key parity is a separate pre-existing
  gap (same root as `33.5`); not filed as new since it predates this work
  and tracks with the broader janet-subscript story.
