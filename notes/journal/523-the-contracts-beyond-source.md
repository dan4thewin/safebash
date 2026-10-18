# 523 - The contracts beyond source

2026-10-18. DG asked to continue coverage from `07f2dc5`: 147 compiler
production cold lines. This batch closes all fourteen in `ir.rs` (6),
`ssa.rs` (4), `resolve.rs` (2) and `emit_ir.rs` (2).

## Two diagnostic paths belong to source specs

A multipart quoted join separator is not a compile-time literal.
`errors.sh` `jsep.8` pins SB5074's distinguishing fixed-separator message,
not merely its code. It reaches `literal_word_text`'s multipart refusal.

`arithmetic.sh` `irbrief.1`-`irbrief.2` pin the deliberate `...` rendering
of a parenthesized operand in a non-final comma expression and a bare
comparison. These reach both callers of `arith_brief`. Corrected the stale
comment claiming spec 31.63 still exercises that renderer; mutations are
now diagnosed by their target instead.

The three real-CLI controls first produced the intended messages. A focused
instrumented corpus then warmed the exact two cold lines, with the join
case as a reasoned positive control. Running the actual spec files under
the focused driver warmed those same lines. The final CLI-only merge
counts are respectively 2 and 4; the unit union adds nothing to them.

## The other contracts have larger domains than source

Eight same-file API tests cover twelve source-inaccessible sites:

| Contract | Assertion |
|---|---|
| Generic attest recognition | Wrong predicate, nonliteral lead and zero/four test-form leads return `None` |
| Internal builtin metadata | All eleven internal kinds are statements and cannot take redirects; `Other` can |
| Internal instruction construction | An authorial builtin is rejected by the actual `SsaInst::internal` constructor |
| Rendered substring operands | Malformed integer text reports the SB4030 invariant rather than becoming zero |
| External name search | Absolute and relative path heads are not PATH-search candidates |
| Complete inert IR | Whole-program serialization retains an Export declaration and separates multiple flags |
| Validated descriptor access | Nine malformed targets refuse with the shared descriptor-invariant message |

The attest recognizer explicitly promises `None` for shapes it does not
recognize; that is not a reason to replace its answer with an ICE.
Likewise the internal metadata and path-search refusals remain correct
answers to their APIs. DD#104 requires complete inert AST serialization;
exercising Export and multiple flags does not add either source syntax.
These tests do not rival valid-source behavior already pinned by specs.

## One descriptor contract, five consumers

The old `dup_target_fd` returned an optional i64. Native, C and private-fd
reservation expected a value, while capture analysis quietly interpreted
`None` as a descriptor not holding the pipe. Its parse also accepted
negative, signed and out-of-i32 literals that SB4048 rejects.

The new API matrix failed against that implementation: its first malformed
target simply returned `None`. `validated_fd_number` now enforces the full
already-settled domain: a bare literal, nonempty ASCII decimal digits, in
0..2147483647. It returns an i64 or identifies the broken invariant.
The matrix also checks quoted/dynamic targets, empty and nondecimal text,
negative zero, an explicit plus sign, i32 overflow and i64 overflow.

The fifth consumer was a higher-order reference in `collect_stmt_fds`,
not an indexed call: `close`'s private-fd reservation. DD#14 and SB4052
require exactly the same descriptor domain, so the accessor is shared
there too, with both SB4048/SB4052 named in its refusal. All five consumers
are wired; missing close arity retains its separate SB4052 assertion.

The live complexity gate rejected the touched capture-flow function.
Separated the exhaustive redirect-source classifier from the left-to-right
fd-state update. Classification still happens before the destination is
removed, and the helper retains the declared-total/no-wildcard contract.
All staged gates then passed without a bypass.

This is an invariant tightening, not a demonstrated new source-level bug
or a change to descriptor policy. Existing source rejection and runtime
behavior are unchanged.

## Measurement and publication gates

Fresh, complete-scope union: **133 / 29912** production lines remain cold,
down **147 -> 133**, with no cold-file regressions. All four selected files
are now zero. Native's eleven executable modules remain **6535** production
lines, zero cold; C remains **2835**, zero cold. All **40** measured worktree
blobs are current.

Raw cargo-llvm-cov reports **99.10% line / 99.51% function / 94.29% branch**;
those figures include inline test scaffolding, which the production database
excludes. Separate attribution merges exactly **8766 CLI profiles** across
all **32** merge slots. Each of the twelve API-only sites has CLI count zero
and a positive union count. Missing DA records fail the attribution gate.
Saved both CLI and union exports and merged profiles before resetting.

Predicted **8909** checks, native **4557** and C **4352**: **8896 passing /
13 existing C skips**. Instrumented and ordinary sweeps both match exactly.
All **8903** baseline statuses at rowids **6057952-6066854** are preserved;
the only additions are the six intended mode-qualified IDs. No retirements
or previous-row flips; first-versus-current liveness stays **260**.

The instrumented block is **6066855-6075763**, with six first-ever IDs.
The final ordinary block is **6075764-6084672**, with zero first-ever IDs.
Both global collision checks are empty; historical numbering checks and
independent per-file plans agree with the predicted totals.

Ordinary tools restored with `mk cov-reset`; final units pass:
**187 sash + 27 convert + 20 expand**. Persistent-directory compiler and
native/C joined-program canaries produce correct output and no profiles,
both without profiling variables and with explicit compiler/runtime
destinations. Status records the result. No architecture or source-language
policy changed, and no coupled finding is deferred.
