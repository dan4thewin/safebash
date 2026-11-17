title: Rejecting trusted regex-pattern reads
date: 2026-11-17

# Rejecting trusted regex-pattern reads

DG's DD132 trial requires pattern operands to reject every actual trusted
read, independently of taint switches or a preceding successful validity
check. The AI implemented that bounded structural rule after `516944b5`,
on `glob-scratch-trial`. Main remains `6c7b899f`; the integrated trial,
immutable-value proof and caught-scratch permission work are not complete.

## One pattern-role check

The existing common regex checker now raises SB5105 before its
dynamic-expression early return. The builtin operand, test-expression and
dispatch-pattern routes share it. Substitution sugar already desugars into
those same builtin calls, including each chained edit and cell substitutions.
The diagnostic points at the first offending trusted expansion.

The guard tests `ParameterExpansion::taint_off`, not source text. It reuses
the existing trust-modifier predicate and canonical expression/body walkers.
The expression walker reaches arithmetic and subscript operands but stops
at capture bodies; the statement walker supplies that recursion, including
nested captures. No private structural walker or arithmetic grammar was added.

Trusted subjects, replacements and `r?` candidate reads remain legal, as does
literal text containing `:!`. This prohibition cannot be disabled by
`--no-taint` or `--no-taint-cst`. It does not certify ordinary untrusted
runtime patterns: the required immutable-value/all-reaching-path proof
still follows. No runtime, backend, ABI, cache or dependency changed.

Private checker names now reflect pattern validation rather than only
literal syntax. Flattening the existing test-argument loop removes needless
nesting from its touched caller (cc4/cog3), preserving all three checks
and their order under the normal complexity gate.

## Contract and preservation evidence

`regextrust.1`-`.18` cover match variants, sub/gsub, logical tests, later
dispatch arms, scalar/cell/chained sugar, combined dispositions, indices,
arithmetic, nested captures, both taint switches and a helper's args cell.
The three following cases preserve trusted non-pattern roles, literal
modifier-like text and trusted validity candidates.

All 21 source programs compile and run without diagnostics on the frozen
post-predicate compiler/runtime pair. Exactly the 18 rejection assertions
fail there in each backend; all 179 assertions pass on the positive tree.
Three initial fixture errors were corrected before accepting those controls:
a constant-test refusal, an invented CLI switch and another constant test.
The authoritative files are the `before-final-*` artifacts, not the drafts.

The existing ERR `migratea.48` expected the later SB5101 dynamic-regex
refusal for a trusted pattern. Its replacement, fresh `regextrust.22`,
requires the earlier SB5105. `regextrust.23` retains the separate ERR rule
by reading a trusted ordinary-data copy as its dynamic pattern. That case
passes before and after; it is not a validated-regex proof. All 79 terminal
assertions pass per backend. The old two mode IDs are explicitly retired.

Two independent reversions, applied separately, produce these exact red
sets in both modes:

| Disabled mechanism | Failing assertions |
|---|---|
| Common SB5105 guard | `regextrust.1`-`.18`, `.22` |
| Nested capture-body result | `regextrust.14` only |

Both campaigns run the complete regex and terminal files. All mutations
are restored; the final source, specs, compiler, runtime archive and both
local lockfiles match the positive manifest after the full gate.

## Acceptance and continuation

```text
NEW=46 FLIPS=0 RETIRED=2 TOTAL=12043
rows 7135096-7147138
native 6133 + C 5910
12030 passing + 13 existing skips
```

The sweep took 281 seconds; liveness remains 249. All 11997 retained
predicate-baseline statuses are unchanged. Only the two `migratea.48`
mode IDs retire; the 46 new mode IDs are `regextrust.1`-`.23`.
The ordinary unit suites remain 239/27/20. Formatting, Clippy, totality
and touched-function complexity retain their normal hook enforcement.

Evidence is in the session's
`files/glob-trial-20261117/regex-proof/trust-guard/`: before/positive
TSVs and xtraces, prior predictions, both reversion campaigns, restored
identities and the gate log. The runtime archive is unchanged from the
frozen post-predicate baseline.

DD39/DD132, GUIDE, status and the integrated owner record this delivery
without treating it as the full proof implementation. No fresh coverage
measurement or main-branch change is included. Continue with exact immutable
pattern proofs, lexical caught-scratch permissions, regex result channels
and `rglob`, then evaluate usefulness, migration and memory/time costs before
DG decides whether to adopt the package.
