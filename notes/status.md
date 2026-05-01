title: Status

## Current state

590 tests passing (268 codegen, 131 e2e, 59 oracle, 52 brush compat,
43 taint spec, 37 janet). 52 design decisions. CST taint gate + SSA
taint backstop (DD#51). Two adversarial audits complete.

Compiler works end-to-end: bash source → parse → validate → lower →
type check → transform → fold → codegen → link → static ELF.
Output is byte-identical to Bash.

## Test suite

| File | Count |
|------|-------|
| codegen_tests | 268 |
| e2e_tests | 131 |
| oracle_tests | 59 |
| brush_compat_tests | 52 |
| taint_spec_tests | 43 |
| janet_tests | 37 |

## Recent sessions

**2026-04-25/26/27:** DD#47-52, adversarial audit run-2, SSA taint
backstop, 17 codegen/runtime improvements, Brush compatibility suite.
20 commits, 590 tests.
