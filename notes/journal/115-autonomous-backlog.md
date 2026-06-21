---
title: 115 — autonomous backlog work (running log)
date: 2026-06-20
---

# 115 — autonomous backlog (running commentary)

DG: work the ready backlog in dependency order, don't stop for status,
file decisions/non-blocking bugs as tasks, fix blocking bugs with
straightforward resolutions inline. Brief running log here.

Ready-task survey (no unmet deps, freshest context first):
`for-loop-arith-condition` → `array-splat-command-arg` →
`glob-as-array-splat-disposition`. DD#63-blocked ones
(`regex-interpolation-delimiters`, `dd66`, `dd68`, `vet-var-modifiers`)
wait on the regex-builtin work. Other standalone readies
(`subscript-append-broken`, `nonbrace-dynamic-subscript`,
`tilde-vs-dd53`, `emit-janet-argv-parity`, …) after.

## for-loop-arith-condition — START

Goal: for-loop bound checks use the `(( ))` arith path (the proven
`while (( … ))` mechanism: arith value + `LetExit` → `test_result_var`),
not a synthesized `[[ __ai -lt … ]]` test. Then delete the numeric arm
(`numeric_cc` + branch) from `codegen/tests.rs` and `emit_c.rs` so
`[[ ]]` carries zero integer machinery. Rider: direct `arrlen` →
`sash_table_len` arm in `lower_value_as_int` (kill the int→string→int
round-trip).

## for-loop-arith-condition — landed (with fallout)

Converted both for-in bound checks (pipelines.rs::emit_for_bound) to an
`ArithBinOp` + `LetExit` on the `(( ))` path; generalized `emit_let_exit`
to accept any `SsaValue`; deleted the `[[ ]]` numeric arm (`numeric_cc`,
emit_c `numeric_c_op`/`lower_numeric_test_c`); added a direct `arrlen` →
`sash_table_len` arm to `lower_value_as_int`.

Fixed a latent bug it exposed: `for_arr.exit`/`for_at.exit` weren't in
`is_catching_construct_exit` (both backends), so `LetExit`'s exit-status
leaked rc=1 from the failing final bound. Added them → for-loops rc=0.

Test fallout + a real bug found:
- 10 codegen_tests used numeric `[[ ]]` (`[[ $x -lt .. ]]`) via the
  validation-bypassing `compile_to_object` path — testing the now-removed
  arm. Converted to `(( ))`.
- **Real bug fixed:** `(( <= ))`/`(( >= ))` were silently wrong (`<=`
  always 0, `>=` always 1). Root cause: the bash lexer splits `<=` into
  `<`+`=`, and `collect_arith_content`'s `needs_separator_space` inserted
  a space (`5 < = 5`) so the arith byte-lexer parsed `5 < (=→0)`. Fixed
  in `arith.rs::needs_separator_space` (no space between `<`/`>` and `=`).
  (`==`/`!=` were fine — single `Word` tokens.)
- Two pre-existing bugs filed, tests `#[ignore]`d: `$#` not read in
  `(( ))` (bug-dollar-hash-in-arith); `until (( ))` not negated — the
  until desugar only wraps `SsaInst::Test`, not the `LetExit` arith
  condition (bug-until-arith-condition).
