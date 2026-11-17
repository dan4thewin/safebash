title: Regex text plus truth, and executable redirect indices
date: 2026-11-17

# Regex text plus truth, and executable redirect indices

DG's authorized DD131/DD132 trial continues after `a4f29d65` on
`glob-scratch-trial`. The AI implemented scalar regex result channels and
the coupled redirect-index repair. Main remains at `6c7b899f`; this is not
adoption of the provisional package.

## Text and truth are separate results

Scalar `match`, `sub` and `gsub` now return their text together with a
matched/no-match Boolean. A direct untyped capture-condition consumes both:

```sash
if text=$(sub aba a X) { echo "$text" }
```

An empty match and an identity replacement are still matches. On no match,
substitution returns the original subject; scalar `match` returns empty
text. Global replacement remembers whether any iteration matched, rather
than returning the final unsuccessful search's verdict.

Otherwise SB4018 requires explicit Boolean discard, such as
`text=$(- sub aba a X)`. The existing canonical builtin classifier supplies
this family; there is no second private name list. Typed declarations,
discarded-result conditions and arbitrary body captures do not gain a
condition result. The separately accepted named-predicate capture extension
remains with its own implementation owner.

Substitution sugar projects text per edit. Its parser-produced command is
explicitly suppressed, and lifting clears the regex value's status-capture
bit. `basename` and `dirname` use the same regex implementation but remain
text-only. None of these projections overwrites an enclosing operation's
completion status. Mutation sugar remains a resultless write.

`RegexMatch` and `RegexReplace` carry `capture_status`. Their two runtime
entrypoints retain the pointer/length return and gain an optional final
status pointer: zero means matched, one means no match, and null discards
the result. Native uses an eight-byte output slot only when needed; C
passes its status address or null. Both preserve operand checks, counted
text, the existing subject-NUL boundary and fatal allocation failures.
This changes the scalar runtime ABI, but adds no regex handle, cache,
dependency or new engine semantics.

Only the final statement in a condition header consumes the enclosing
result. Prefix captures retain their own handling obligations, and neither
an outer condition nor a discard reaches operand or function-body failures.

## The redirect index is not the filename

While exercising substitution in cell indices, the work reached the
existing redirect-target ICE. DG resolved its surface question explicitly:
allow captures in guarded cell indices, while continuing to forbid direct
capture targets. DD119 records that distinction. The selected cell still
owes its nonempty disposition, and index evaluation retains taint, pattern
proof and ordinary failure checks.

Merely lifting the target earlier was insufficient. DD86 already requires
arguments to be evaluated and retained before targets, then each target
to be evaluated and opened in order, stopping at the first failure.
An index can change a global, run a capture or read a descriptor opened by
an earlier redirect. Opening every target after evaluating all of them
would violate that contract.

The shared redirect-scope constructor now lowers one target at a time,
inside the preceding successful scope. Commands and readers prepare
their arguments first. Inlined calls materialize scalar arguments in the
caller before changing positional arrays, collection slots or body context.
Collections passed by identity remain identities; argument splats instead
need snapshots. A bare SSA version reference is not sufficient retention,
because both backends collapse versions of a binding into common storage.
Scalar retention and elided arguments preserve their actual value and
trust metadata.

Executable stage indices stay in the stage child after pipeline plumbing,
not in the parent. Final-reader ownership remains DD122's. Earlier
redirect failure skips later index effects and the command body.
Temporary scopes restore saved descriptors on both completion paths.

FD-only `exec` needs a different close operation: keep descriptor changes
only after the whole list succeeds, otherwise restore the opened prefix.
`RedirectClose` distinguishes ordinary restoration, nested keep-on-success,
and outer keep-and-rebind-on-success. Both backends close successful backup
descriptors inner-first and rebind the diagnostic invoker once, after the
outermost success. The earlier draft committed each successful open too
soon; `regexresult.58` demonstrates its lost stdout/stderr and diagnostic
routing on a later failed open. `.59` separately pins successful diagnostic
rebinding.

This closes the executable-index ICE, not the general timing backlog.
Legacy simple stage redirects retain their earlier path; general
inter-argument capture ordering and shared recoverable-file handling stay
with their existing owners. No rollback of created files, consumed input
or completed argument effects is claimed.

## Before controls and independent reversions

The regex file now has 321 checks per mode. `regexresult.1`-`.59` cover
the new results, preserved boundaries and redirect interactions. Thirty-eight
older contracts move to fresh `migratea.65`-`.102` IDs rather than recycling
their history: 194 added mode IDs and 76 retirements for the whole unit.

The matched original compiler/runtime fails 33 assertions per mode in the
earlier 302-check file: 31 new result assertions and two migrated
redirect-index programs. Its old ABI requires the saved runtime; actual
linker archive-open traces establish that pairing. The later prelude,
ordering, rollback and diagnostic baselines use the new ABI instead.

Saved intermediate compilers separately pin the diagnosed repairs.
The prelude baseline fails `.34`/`.35`; the pre-order baseline fails
`.41`/`.42`/`.43`/`.44`/`.45`/`.48`/`.49`/`.50`; the pre-rollback baseline
fails `.58`. The final `.44` uses a legal dispositioned splat and also
checks trusted elision. Its earlier bare-splat draft was invalid and is
not accepted evidence.

Ten independent reversions were then run against the final 321-check file.
Each runtime/semantic mutation ran in both backends; backend-specific
mutations also ran the unchanged backend as a control.

| Disabled mechanism | Failing `regexresult` checks |
|---|---|
| Runtime substitution verdict | 1, 3, 4, 6, 7, 23, 24 |
| Runtime scalar-match verdict | 8, 9 |
| Native result recording | 1, 3, 4, 6, 7, 8, 9, 23, 24, native only |
| C result recording | The same nine, C only |
| Unhandled-result guard | 11-16, 34 |
| Explicit-discard projection | 28 |
| Basename/dirname projection | 29 |
| Parser substitution projection | 28, 30, 39, plus 19 older sugar assertions |
| Native successful-exec rebinding | 59, native only |
| C successful-exec rebinding | 59, C only |

False-verdict stubs avoid turning a terminating regex loop into an infinite
one. Every predicted new failure matches. The sugar mutation's complete
22-ID set agrees across backends and is retained with the other TSVs.
All mutations were restored and their source, compiler, runtime, tour and
lock hashes matched the frozen positive before the full-gate retries.
The subsequent diagnostic repair has its own final snapshot.

## The first full gate exposed real omissions

The first sweep recorded 12437 checks, ten failures and twelve missing
checks, rather than the predicted 12449. The ten failures were `71.6`,
`71.8`, `nonvoid.15`, `atpred.26` and `atpred.31` in both modes. Actual
xtraces distinguished three causes:

- Two old assertions still expected exit-status-only capture advice.
- A rejected named-function capture-condition fell through into ordinary
  capture lowering, adding contradictory SB4016 after SB4025.
- The new synthetic suppression wrapper hid the operation-specific
  substitution-sugar explanation in the strict attest diagnostic.

The renderer now uses the canonical single-value-command peel, and the
capture-condition's value is classified without ordinary unhandled-capture
advice. The existing `nonvoid.15`, `atpred.26` and
`atpred.31` assertions are unchanged. Only the two obsolete wording
expectations change; refusal and source positions remain pinned.

The missing checks were `tourgen.1`-`.6` in each backend. The tour checker
itself still used plain `$(gsub ...)` for formatting and no longer compiled.
Its explicit discard repairs that migration. The retirement cap correctly
refused all 88 apparent retirements: the intended 76 plus twelve current
tour checks. Raising the cap would have concealed the omission.

The repaired actual errors, taint, harness and regex files pass in both
modes. The tour source now teaches text-plus-Boolean use, explicit discard
and accumulating match arrays. `mk -p 1 tour-doc` regenerated its GitHub
edition with all 56 snippets matching; the generated file was not edited
by hand.

The second sweep restored all twelve tour checks and retired exactly the
intended 76 IDs. At rows 7196317-7208765 it found two failures instead:
`rec.9` in each backend. The first diagnostic repair had stopped lowering
an already-rejected capture, losing the independent SB5037 recursion
diagnostic that this existing assertion requires alongside SB4025.
The actual xtraces showed the missing code, not a new acceptance or crash.

The corrected route classifies the value once, without the contradictory
ordinary-capture advice, but still lowers a rejected value through the
existing capture machinery. Independent recursion and arity checks remain.
Both unchanged files now pass: 253 function and 725 error assertions in
each backend, including both `rec.9` and `nonvoid.15`. No assertion was
weakened or retired to accommodate the diagnostic repair.

The redirect-index task also contained a separate historical SB4049 advice
defect: it recommends redirecting to bare `$v`, which DD119 refuses for
possible emptiness. The current parser still contains that advice. Its
wording-only repair is now separately owned by
[bug-sb4049-redirect-advice](../tasks/bug-sb4049-redirect-advice.md);
closing the index ICE does not claim that unrelated text was repaired.

## Final acceptance

The diagnostic-repaired sweep at rows 7208766-7221214 passed all 12449
checks: 12436 passing and 13 existing C-mode skips, with exactly the two
expected `rec.9` repairs and no additions or retirements. A stop request
arrived around its completion, but its log records `GATE PASS` and its
complete database block; it is a completed passing sweep, not an interrupted
one.

The live complexity check then required a small internal separation:
`assignment_value` classifies the RHS, while `handle_plain_assign` emits it.
Both score cc3/cog2; `maybe_capture_condition` is cc4/cog3. Classification
still happens once, and rejected captures still reach their independent
checks. The post-extraction compiler/source snapshot is `results-before-lint`;
the earlier passing diagnostic snapshot is `results-before-complexity`,
not the older `results-positive` used for the mechanism reversions.

The post-extraction serialized ordinary gate used
`NEW=0 RETIRED=0 FLIPS=0 TOTAL=12449`. Rows 7221215-7233663 contain exactly
6336 native and 6113 C checks: 12436 passing, 13 existing skips and zero
failures, flips, additions, retirements, ghosts or plan mismatches.
The sweep took 293 seconds; liveness remains 245. The gate regenerated
status from this exact sweep.

The first normal commit attempt then refused a redundant `&name` borrow in
the new regex-result diagnostic. Removing that borrow changes no behavior;
the complete formatting, Clippy, totality and complexity hook now passes
without a bypass. No commit was created by the refused attempt.

The final ordinary gate, with the same predictions, passed at rows
7233664-7246112: 12449 checks, the same native/C counts and 13 skips, and
zero failures, flips, additions, retirements, ghosts or plan mismatches.
This sweep took 294 seconds; liveness remains 245. `results-final` now
identifies this post-lint source/compiler snapshot, and status is generated
from this final sweep.

The independent comparison uses the complete 12331-row accepted
caught-scratch block ending at 7183879, never the dirty tree's shared HEAD
hash. It confirms 12255 retained statuses unchanged, 194 additions and
76 retirements. An initial comparison accidentally began 22 rows late;
correcting the range, rather than accepting its inflated addition count,
produced the stated full-block accounting.

The final source, compiler, runtime, tour and lock manifest still matches.
Four owned generated probe executables were removed after inspection;
their hashes, sources, output and before/final compiler/runtime artifacts
remain in the session evidence directory
`files/glob-trial-20261117/regex-results/`. No fresh coverage was run.

## Remaining trial work

`rglob` is the next implementation unit over the shared selector/publication
machinery. Useful programs, converter migration and memory/time costs still
need integrated evaluation before DG's adoption decision. Existing runtime
regex compilation remains per use; this unit makes no allocation-performance
or retained-cache claim. Coverage was not resumed.
