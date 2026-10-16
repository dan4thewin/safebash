# 511 - The nine lines were not one kind

2026-10-16. DG: "continue with coverage".

Started at `682203e` with `mk cov-stale` current: 284 production cold lines,
nine in `parse/parser.rs`. Journal 510 had called those nine ordinary totality
arms and shadowed fallthroughs. That classification was not earned.

## Two bugs

`fn f() -> { echo ok; }; f` compiled and printed `ok`. Once the parser saw an
arrow it called `consume_int_token`, whose non-int arm silently selected
`VarType::String`. DD#16 defines `-> int`, not an arrow that may omit its type.
The shared definition tail now propagates a parse error: SB4014, "expected
'int' after '->' in function return type". The silent helper is gone.
`rty.1`-`rty.6` cover all four definition spellings, an unsupported type and EOF.
Unannotated functions and valid int annotations retain their existing specs.

Reading the producer for `attest_shorthand_test` exposed the second bug.
`parse_attest_all_lead` accepted arbitrarily many binders, but DD#61 defines
only one for an array and two for a map. This still reached the same transform
ICE fixed for absent predicates in journal 510:

```sash
str x=p
array a=(p q)
attest x y z in a -n
echo done
```

Without the declaration SB5031 masked it. The full `[[ ... ]]` spelling also
ICEd after declaring the three names. Both are now SB4014, with the binder
arity requirement in the message (`93.62`/`93.63`).

The first implementation rejected every over-long lead. The targeted suite
caught `93.52`: DD#113 deliberately sends *arithmetic* attest-all shape errors,
even an over-long lead, to validate's SB5054. Read the preserved xtrace, checked
that DD, and restricted the new arity check to non-arithmetic predicates.
`93.52` and its diagnostic are unchanged; the new rejection covers the two
forms that actually bypassed the guards.

## Two real spec gaps, not two invariant failures

The corpus included a missing annotation as a positive control: it warmed
`consume_int_token`'s cold arm. A bare `-` also warmed `parse_suppressed`'s
fallback, producing normal SB5011. `pcold.1` pins that command-head diagnostic.

The cold closing brace in `parse_for` was not meaningless punctuation. It
carried the implicit false branch of the `Tok::In` test. An omitted list
warmed it. The spec uses the incomplete source `for x`, asserting SB4014
"expected a `{ }` body or `do`" (`pcold.2`), without settling the runtime
semantics of a complete loop that omits `in`.

## The remaining six

| Old line at `682203e` | Finding | Change |
|---|---|---|
| 602 | Empty iterator after already checking nonempty | One iterator check handles the existing else body; no duplicate fallback |
| 979 | Unknown declaration keyword, impossible under the five dispatch arms | ICE naming the caller contract; direct-call test |
| 2084 | Missing/nonliteral binder, impossible under the scalar/all lead builders | ICE; tests for both malformed inputs |
| 2605 | Array/map/test are not scalar command-head words | Correct `None` retained; direct test asserts all three variants |
| 3157 | Background spelling after the same match already returned for `&` | Carry `&&`/`||` spelling with the parsed operator |
| 3591 | Missing/nonliteral remainder for the `unrecognized` sentinel | ICE; tests for both malformed inputs |

These are two redundant branches, three producer-invariant guards and one
totality answer.

The three guards pass the rival test: they check contracts of parser producers,
not copies of validate/type-check gates on the same AST. The totality test
asserts the answer, not merely execution. Six new unit tests in the same file;
no new end-to-end Rust harness.

## Correcting the proof claim

Journal 510 and AGENTS.md said narrowing was "self-proving" and the compiler
partitioned live and dead cases without being told. That is false: the edit
also deleted or replaced branches. Type-checking the edited program proves
its new result contract; it cannot prove those old branches were unreachable.
The caller/producer enumeration did that work. Narrowing remains a good way
to encode an established invariant, not a replacement for establishing it.
Corrected AGENTS.md and the task resolution, and marked journal 510.

## Gates and measurement

Ten new specs in dual-mode files: predicted **8383 + 20 = 8403**.
Source-derived plans agree: native 4304, C 4099. The first sweep produced
8390 passing / 13 skipped, zero flips, exactly the 20 expected new IDs.
Regression-query liveness remains 258.

`mk test` initially failed inside the installed Go `mk` itself: "all goroutines
are asleep - deadlock!" after the regex checks. No Rust assertion failed.
`mk -p 1 test` completed: **131 + 27 + 20**, six more sash unit tests.
Filed the runner failure separately; its underlying cause is not established.

The pre-commit complexity gate then caught `declaration_axes` (13 cyclomatic /
6 cognitive), a pre-existing violator touched by its new guard. Flattened the
stacked declaration checks into one `(keyword, token)` match: **11 / 2**,
without a helper or bypass. Re-ran the unit gate and the union measurement
after that source edit. Fmt, clippy, totality and touched-function complexity
clear their gates.

`mk cov-union` then `mk cov-db` and `mk cov-stale`: current for the measured
working-tree blobs, based at `682203e`. Its rerun sweeps again produced 8403,
zero flips and zero new IDs. Production counts exclude `#[cfg(test)]` code:

| | Before | After |
|---|---:|---:|
| `parse/parser.rs` cold lines | 9 | **0** |
| Compiler cold lines | 284 | **275** |

The raw cargo-llvm-cov headline, which includes test-defined lines, is
98.74% line / 99.19% function / 92.90% branch. Zero production cold lines in
this file is not a claim of zero cold branches. Restored uninstrumented
binaries with `mk cov-reset`.
