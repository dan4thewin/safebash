# 526 - A private name is not a private binding

2026-10-18. DG continued the coverage campaign from `bc613d6`: 106 cold
compiler production lines. The selected cohort was seven in function
lowering and two in command handlers. Whole-function reading exposed real
source bugs before the cold contracts could be closed.

## A prefix was not an allocation

Legal globals named `__strcap_f`, `__intret_f`, `__bodycap_1` and `__args0`
were overwritten by compiler temporaries. An integer sentinel 77 became 7;
an anonymous capture replaced a sentinel with `left`/`right`. Repeated
string or integer calls in one argument list printed **`2 2`**, not `1 2`,
because both values referred to the same synthetic name. Native and C
agreed on the wrong answers.

DD#125 does not reserve the private `__` namespace. The fix is allocation,
not another restriction on source names. All four function producers and
the sibling redirect spill now use `VersionState::reserve_temp_name` before
`define`, retaining the correct string/int type. Its reservation set covers
all inferred source names, including later declarations, and every previous
temporary. A block count, a counter or a function prefix alone does not.

The first twelve `tmpcap` regressions all failed against the old native and
C tools while the original 190 function checks passed. Their outputs and
failed-before logs were saved before removing the task executables.
The initial allocator repair fixed eleven; the twelfth found another bug.

## An argument is evaluated before the callee exists

`pair $(next) $(next)` reached the unlifted-function-value codegen ICE.
Plain calls, function captures and function pipeline stages built their
implicit argument arrays from values that could still contain function
calls. Moving only the storage names did not execute those values.

These paths now reuse `lift_int_func_args` before installing the callee's
argument array, collection slots and inline context. Finite `f $(f leaf)`
nesting is not recursive inlining; caller positionals must still mean the
caller. A pending call redirect is taken before lifting and opened after
it, so nested captures cannot steal it.

Pipeline argument evaluation stays inside its existing forked function
stage. The source control prints `1 2` from the stage and then `0` from the
parent counter. This preserves the boundary instead of moving effects into
the parent to make lifting easier.

Producer tail calls had the same missing step and used the scalar renderer:
`return F @a` joined rather than splatted. The shared tail emitter now lifts
before rebinding `args`; producer arguments use canonical `arg_to_value`.
The producer path also matches the existing int-tail refusal of collection
slot rebinding: `=A` cannot become a scalar while the old slots remain.
Three source cases pin tail captures, splats and SB5037. The complete block
is **21 dual-mode `tmpcap` specs**.

## Digits were not the runtime integer domain

`${args[18446744073709551616]}` reached native's non-integer-literal ICE.
The literal-index predicate checked only decimal spelling; the shared
integer-value predicate accepted any sequence of digits and minus signs,
including `--1`, `1-2`, `-` and empty.

Integer values now require a successful `i64` parse, retaining the existing
minus-only spelling rather than introducing leading `+`. A bare array
index additionally retains its unsigned-decimal spelling requirement.
Digit-only map-key classification stays separate: an oversized numeric key
still needs quotes, and the quoted key remains an ordinary string.

Fifteen dual-mode `lit64` cases pin both signed endpoints, malformed scalar
and int-array values, oversized reads/writes, implicit `args`, the maximal
valid literal index, numeric-map-key quoting and the existing leading-plus
refusal. The first fixtures incorrectly expected type-check errors for
cases the parser already rejects, and used unsupported quoted map-literal
syntax; those assertions/setups were corrected, not the compiler.

## Cold answers still need their own evidence

Six direct API units pin seven source-inaccessible sites:

- missing registered function bodies cannot leave a `FuncCall` unlifted;
- nested functions own their arguments and returns, not the outer scope;
- a malformed literal index cannot erase the inferred arity ceiling;
- a nonliteral command head is not a literal `return`;
- a malformed `read` cannot silently disappear;
- failed late `exec` resolution records SB5011, not a bare runtime target.

The missing-body guard is orthogonal SSA evidence: the real constructor
requires an already registered body. The malformed-read guard has no safe
decline: its old fallback deleted the command. The correct classification
answers assert their work, not merely reach a line.

Removed retired raw-`$@` signature support and reused the canonical
positional helpers instead of keeping a silent out-of-range branch.
Signature and return statement walkers are explicitly total. The live
complexity gate rejected the first nested `sig_record_index` shape; an
early dynamic-index return expresses the same distinction without nesting.
No gate bypass or new test scaffolding.

## Fresh measurement

Complete-scope union: **97 / 30450 production lines cold**, down
**106 -> 97**. Function lowering is **7 -> 0**, command handlers **2 -> 0**;
no other file regresses. Binding resolution, native emission and C emission
remain zero cold. All **41** measured source blobs are current.

Raw cargo-llvm-cov: **99.17% line / 99.56% function / 94.62% branch**,
including inline test scaffolding. Exactly **9010 CLI profiles across all
32 slots** separate source from API execution. Each of the seven retained
API sites is **CLI 0 / union 1**. All five allocation producers and all four
repaired argument-materialization paths are source-warm; the actual tail
argument renderer is warm too. Saved CLI/union exports, merged profiles and
the database before ordinary restoration.

The independent prediction is **9147 = 9134 passing + 13 existing C
skips**, native **4676**, C **4471**. The instrumented sweep matches exactly:
**72** intended first-ever IDs, every one of the **9075** baseline statuses
at **6156883-6165957** preserved, no unexpected additions, retirements or
previous-row flips. Liveness is **260**. Its block is
**6165958-6175104**. Source plans and both global ID checks agree, including
the known two-check expansion behind `coverage.sh`'s variable-ID loop.

## Ordinary publication gates

Restored with `mk cov-reset`, not a cargo no-op. Final units:
**208 sash + 27 convert + 20 expand**. The ordinary sweep is the same
**9147** checks, block **6175105-6184251**, with zero first-ever IDs.
The exact baseline/addition/retirement/previous-row gates pass again;
liveness remains 260. Native/C canaries preserve legal private-looking
globals and produce distinct nested-call results with no profiles under
either default or configured compiler/runtime profiling destinations.
Fmt, clippy, totality and live staged complexity gates pass without bypass.

DD#125, DD#81, DD#27, DD#80, GUIDE and the temporary-allocation SOP record the
repairs. The existing run-3 review scope now includes these identity,
evaluation-boundary and integer-domain seams. No coupled bug is deferred.
