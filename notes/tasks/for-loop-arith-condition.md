---
title: "for-loop bound check should use the (( )) arith path, not a synthesized [[ -lt ]] test"
status: ready
priority: P2
kind: refactor
depends: [dd62-test-expr-ssavalue]
gates:
  - "[[ ]] codegen carries NO integer-comparison machinery: numeric_cc + the numeric arm are deleted from codegen/tests.rs::lower_test_binary AND emit_c.rs (numeric_c_op/lower_numeric_test_c); the `other => panic` arm is the only thing left for numeric ops"
  - "no code synthesizes an ir::TestExpr with a numeric op (-lt/-eq/…); pipelines.rs for-in-array and for-in-argv build their bound via the arithmetic path instead"
  - "for-loops, while-(( )), and (( ))-as-condition all drive the loop/branch via the same test_result_var mechanism (LetExit), not via SsaInst::Test"
  - "spec native back to baseline (cf.c7/cf.c8 only); c/janet no regression; rust suite no net regression; complexity gate green"
---

# for-loop bound → arithmetic `(( ))` path (drop the `[[ -lt ]]` synth)

## Principle (DG, 2026-06-20)

`[[ ]]` machinery should contain **no integer tests at all**. DD#62
already rejects user-written numeric ops in `[[ ]]` (SB5019, numeric →
`(( ))`). A `for` loop that needs an integer bound check (`i < len`)
must therefore use **what underlies `(( ))`**, not borrow the `[[ ]]`
test path.

## Current smell

`transform/pipelines.rs` lowers both for-in forms by **manufacturing an
`ir::TestExpr::Binary { op: "-lt" }`** purely to reuse `SsaInst::Test`:

- for-in-array (pipelines.rs:34–56): `[[ __ai -lt ${arr arrlen} ]]`
- for-in-argv  (pipelines.rs:~196–209): `[[ __fi -lt __argc ]]`

These are the **only two** synthesizers of a numeric `[[ ]]` test
(`grep '"-lt"' transform/`). Because they bypass validation, codegen
*must* handle `-lt` — which is why DD#62 section A (commit `14bd0e0`)
had to **restore** a numeric arm (`numeric_cc` + integer `icmp`) inside
`codegen/tests.rs::lower_test_binary`, and a twin (`numeric_c_op` /
`lower_numeric_test_c`) in `emit_c.rs`. That arm is a **band-aid**: it
re-introduces integer-comparison machinery into `[[ ]]`, the opposite of
the principle above. (Original ICE: dropping the arm panicked on every
`for` loop — see journal 114.)

## The redress (the mechanism already exists)

`(( expr ))` as a condition is lowered by
`transform/statements.rs::lower_arith_test_stmt` (≈:297):
`expr_to_value(Arithmetic)` → an `ArithBinOp` comparison → assign to a
temp → `SsaInst::sigil(BuiltinCmd::LetExit, …)`. `LetExit` sets
`test_result_var` (the universal condition slot that
`Terminator::Loop`/`Branch` reads — same slot `SsaInst::Test` and plain
commands write, codegen `commands.rs:163-167`). Proven end-to-end:
`while (( i < 3 ))` runs correctly in native **and** c today, and
`LetExit` is handled in both backends (dispatch.rs:198, emit_c.rs:826).

So **no new wiring** is needed. Steps:

1. **pipelines.rs** — rewrite both for-loop bound checks to mirror
   `lower_arith_test_stmt`: build the comparison as an arithmetic value
   (`__ai < arrlen(arr)`, `__fi < __argc`), assign it, emit `LetExit`,
   keep the existing `Terminator::Loop`. Delete the synthesized
   `TestExpr`/`test_to_ssa` calls.
2. **codegen/tests.rs** — delete `numeric_cc` and the numeric arm from
   `lower_test_binary`; numeric ops fall to `other => panic` (now
   genuinely unreachable — correct, not a disarmed trap).
3. **emit_c.rs** — delete `numeric_c_op`, `lower_numeric_test_c`, and the
   numeric branch in its `lower_test_binary`.
4. Confirm nothing else builds a numeric `TestExpr` (the two for-loops
   are the only synthesizers; user numeric `[[ ]]` is SB5019-rejected
   pre-codegen).

## Rider: `arrlen` int→string→int round-trip

The synthesized comparison still has `arrlen` (`${#arr[@]}`) as an
operand. `codegen/values.rs::lower_value_as_int` (≈:622) has no
`SsaValue::ParamExpansion` arm, so `arrlen` falls to its
`_ => lower as string, then sash_atoi` fallback — i.e. `sash_table_len`
(an i64) is stringified (itoa) and re-parsed every iteration. The
old (now-deleted) `try_lower_int_arrlen` called `sash_table_len`
directly. Add a `SsaValue::ParamExpansion { operator: "arrlen", .. }`
arm to `lower_value_as_int` (and the emit_c twin) that calls
`sash_table_len` and returns the i64 — kills the round-trip for the
loop bound and any `(( … ${#arr[@]} … ))`. Orthogonal to the test-vs-
arith move but converges with it. (Perf regression introduced by
DD#62 section A; correctness was never affected.)

## Why it matters

- `[[ ]]` becomes purely string/file tests — matches DD#62 intent and
  removes a class of "synthetic test bypasses validation" gotchas.
- Removes the committed band-aid (`numeric_cc`/`numeric_c_op`) rather
  than entrenching it.
- One uniform loop/branch-condition mechanism (`test_result_var` via
  `LetExit`/command status/`[[ ]]` test) instead of two.

## Cross-refs

- Introduced by `dd62-test-expr-ssavalue` (section A, commit `14bd0e0`);
  journal 114.
- Template: `transform/statements.rs::lower_arith_test_stmt`.
- Memory: "Codegen must handle numeric `[[ ]]` ops … for-in synthesizes
  `[[ __ai -lt ${arr arrlen} ]]`" — once this lands, that memory is
  obsolete (codegen will no longer handle numeric ops; downvote/replace).
