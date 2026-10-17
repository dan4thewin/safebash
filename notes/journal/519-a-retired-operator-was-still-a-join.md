# 519 - A retired operator was still a join

2026-10-17. DG asked to continue coverage from `b5701f4`, with 189 compiler
production cold lines. Selected all six in native `codegen/arithmetic.rs`
and `codegen/setup.rs`.

## One comparison dispatcher

`emit_arith_op` first handled every comparison through `comparison_intcc`,
then panicked if a comparison reached its trailing match. That panic could
not be exercised even by a direct call: the same function had already
intercepted its entire domain. The six comparisons now live in the one
exhaustive match, with their signed Cranelift conditions unchanged.

The logical-op refusal is different. Both general arithmetic paths intercept
And/Or before evaluating either operand, while the synthesized `ArithExpr`
path is built only at four loop-increment sites, all using Add. Eager logical
emission would run the right operand's guards even when the left decides the
result. Keep that loud backend contract and exercise both invalid operations
through the existing direct-SSA fixture, not a rival source test.

Dynamic CLI controls use `$(argc)` rather than constant folding: short-circuit
And/Or skip an otherwise failing division, and comparison produces the
expected result. Top-level `$#` and literal division by zero were invalid
initial fixtures, not compiler findings.

## Setup was retaining two obsolete answers

The interner still split `arrvals_/` operands at NUL for DD#33's per-element
operators. DD#65 retired that family; the parser retains its operator names
only to diagnose SB4034 and no longer constructs that regex/replacement
encoding. Removed the leftover interning path.

The `MatchInto` and `ReadArray` base-declaration fallbacks were also redundant.
Enumerating their actual constructors leads to `try_handle_match_into` and
`lower_newread`, both of which first call `autodeclare_bulk_target`.
An initializing collection Assign owns the base slot; fill versions alias it.
The fill must not independently allocate a fallback string slot.

Before deleting these paths, three temporary panic sentinels each fired in
a direct-SSA positive-control matrix. The complete 8789-check CLI sweep
reached none. The temporary matrix and sentinels were then removed. Producer
enumeration, rather than the green sweep alone, establishes the invariants.
This applies DD#116's distinction between a backstop and a redundant answer.

## The retired family was still being accepted downstream

Following the removed interning path exposed a coupled native API bug:
`handle_param_expansion` treated every `arrvals_*` name as an ordinary array
join. It did not perform the requested operator and did not refuse it.
C already accepted only the exact `arrvals` and `arrjoin` names.

A new direct-SSA regression failed against native's old dispatcher: malformed
SSA compiled successfully into an object. Narrowing the dispatcher to those
two supported names now sends retired operators to the existing loud
unsupported-operator refusal. DD#65 records the correction; no language
decision or new source spelling is introduced.

Native and C API matrices cover all ten retired operator strings. C needed
no production change. The native fixture now decodes both formatted and
static panic payloads, and reports accidental successful compilation without
dumping the whole object file.

## Measurement and gates

No source specs were added or retired. Ordinary and instrumented sweeps retain
exactly **8789** IDs: **8776 passing / 13 existing C skips**. All 8789 baseline
statuses, saved at rowid **5863407**, are unchanged; reruns introduce zero IDs.
The first-vs-current liveness query reports **260** differences.

Units are **162 sash + 27 convert + 20 expand**. Formatting, clippy, totality,
whitespace and staged touched-function complexity gates pass without a bypass.

Fresh serialized union, database load and worktree-provenance check:

| Cohort | Before | After |
|---|---:|---:|
| Native `codegen/arithmetic.rs` cold production lines | 2 | 0 |
| Native `codegen/setup.rs` | 4 | 0 |
| Native `codegen/value_handlers.rs` | 5 | 4 |
| C emitter | 21 | 20 |
| Compiler total | 189 | 181 |

The predicted eight-line reduction is exact. No other file gains cold lines,
and earlier zeroed cohorts remain zero. The database records **29831**
production lines. Raw cargo-llvm-cov coverage is **99.02% line / 99.50%
function / 94.02% branch**.

Separate attribution merges **8624 CLI profiles**, matching the compiler
signature across all **32** merge slots and excluding the unit binary.

| Backend guard | CLI | Union |
|---|---:|---:|
| Eager logical arithmetic | 0 | 2 |
| Native unsupported parameter operator | 0 | 10 |
| C unsupported parameter operator | 0 | 10 |

The native panic's executable DA record is line 359, not its format-string
line 360. Checking the latter first produced a missing-record assertion,
not a false zero; inspecting source and both exports resolved the attribution.
These are backend API contracts, not source-test gaps.

`mk cov-reset` restored ordinary release tools. The final ordinary unit gate
and full sweep pass with the same counts. A persistent-directory compiler
and compiled-program canary leaves no profiles, even with an explicit
`LLVM_PROFILE_FILE`; provenance still matches all 39 measured source files.
No architecture or SOP changed and no coupled finding remains deferred.
