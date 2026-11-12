title: "BUG — `until (( cond ))` is not negated (infinite loop); until only negates [[ ]] Test conditions"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "until (( cond )) runs the body while cond is false and exits when true, matching bash (e.g. `x=5; until (( x < 10 )); do …; done` exits immediately)"
  - "until <command> (non-[[ ]] condition) is also negated by exit status"
  - "re-enable the #[ignore]d until_basic / until_immediate_exit / until_countdown codegen_tests"

# BUG — `until (( ))` / `until <cmd>` not negated

> **DONE (2026-06-20, native).** The until desugar
> (transform/statements.rs::lower_while_stmt) now appends the
> `NegateTestResult` sigil (the `!`-prefix primitive) to the loop header
> instead of wrapping each `SsaInst::Test` in `SsaTest::Not`. Negates ANY
> condition form ([[ ]], (( )), command) once. The 3 #[ignore]d until
> codegen_tests are re-enabled and pass. c/janet `until (( ))` still fails
> on separate backend gaps (NegateTestResult / int-echo / janet (( ))
> handling) — not the desugar bug.

## Symptom

`until (( x < 10 ))` with `x=5` infinite-loops (runs the body while the
condition is *true*). `until [[ … ]]` works.

## Root cause

`transform/statements.rs::lower_while_stmt` desugars `until` to `while`
then patches the header block, wrapping each `SsaInst::Test` in
`SsaTest::Not`:

```rust
for inst in insts.iter_mut() {
    if let SsaInst::Test(ref mut test) = inst {
        *test = SsaTest::Not(Box::new(test.clone()));
    }
}
```

A `(( ))` condition lowers (via `lower_arith_test_stmt`) to an `Assign` +
`LetExit` sigil — **not** an `SsaInst::Test` — so the negation finds
nothing to patch and the loop runs un-negated. The same gap applies to a
plain command condition (`until grep …`), which sets exit status, not a
`Test`.

## Fix sketch

Negate the condition regardless of how it sets `test_result_var`:
- For the `LetExit` arith condition, wrap its arg value in a logical-not
  (`arg == 0`) — e.g. replace the arg `v` with `ArithBinOp { v, Eq,
  Expr("0") }`, so `LetExit` sets `test_result_var = (v == 0)`.
- For a command condition, flip the resulting `test_result_var` (the
  general "negate last status" the `!` prefix uses).

A cleaner long-term shape: a single negation primitive on
`test_result_var` appended at the header, rather than per-inst patching.

## Discovery

Surfaced 2026-06-20 while converting numeric `[[ ]]` codegen_tests to
`(( ))` after the for-loop-arith-condition work removed the `[[ ]]`
numeric arm. Pre-existing (`until (( ))` was never exercised; the until
tests used `until [[ -ge ]]`).
