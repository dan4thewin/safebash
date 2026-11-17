title: Caught scratch keeps its data, not its read permission
date: 2026-11-17

# Caught scratch keeps its data, not its read permission

DG's DD131/DD132 trial separates immutable contents, unresolved taint,
ordinary-read permission and regex validity. The AI implemented the
caught-scratch read/failure unit after `ebdf18bf` on `glob-scratch-trial`.
The package remains provisional; main stays at `6c7b899f`.

## Permission is lexical, not a clean merge

SB5107 rejects ordinary reads of caught scratch outside its attest-success
arm, even when the candidate was originally clean or the failure branch
cannot continue. Explicit `:!` reads remain ordinary data operations; they
cannot manufacture a regex proof or override SB5105's pattern-role ban.
`--no-taint` disables the ordinary-read check; `--no-taint-cst` does not.
Mandatory regex proving remains independent of both switches.

The existing SSA proof worklist now also checks ordinary reads. One total
instruction inventory serves propagation reads and the existing taint escape
sinks; the value walk retains separately evaluated indices underneath trusted
cells. Assignments, collection copies/publication, keys, map conversion,
metadata and predicate operands cannot silently bypass the permission check.
Only the actual eligible attest subject is exempt in its predicate.
Source-name restoration and call-site provenance preserve useful diagnostics.

R2 is relaxed only for scratch subjects. A failed immutable instance may
remain unresolved and fall through, including without an else. Ordinary
mutable bindings and ordinary consts still need resolution or diversion;
a scratch conversion destination does not excuse an ordinary source.
Late and inherited-input attests retain their existing refusals. An
otherwise-legal later standalone attest permits its own success continuation.

Fresh construction clears expired permissions along with proof/equality/
membership facts. Inlined helper/capture reads and collection slots use the
same resolved identities. Child-only restrictions do not feed the parent.
EXIT receives actual main-process terminal/completion and eligible signal
inputs. ERR conservatively receives global restrictions from main and EXIT.
Neither handler imports a success-arm permission or a regex certificate.

## Shared caught forms and actual results

The IR now owns recognition of a direct attest and its one-stage negated
form. Logical lists normalize to positive-attest branches using the same
recognizer as direct negated-if parsing, validation and source-origin tracking.
This does not peel arbitrary wrappers or reclassify an attest later in a
general condition as the caught operation.

Scalar, attest-all and arithmetic-conversion forms share the existing branch
lowering. The logical-use descriptor keeps the source polarity, which arm
executes the RHS, and whether a consumer owns its result. Only the consumed
RHS result loses its implicit check; argument and body failures remain
checked. Skipping the RHS preserves the negated left result. Statement-level
OR retains the existing successful-guard status reset.

The validator uses the same normalized if transfer instead of its separate
logical-attest resolution approximation. False edges retain real unresolved
taint and provenance; lowering merges the actual reaching versions rather
than treating caught failure as clean.

## Two coupled repairs

A bare trusted array splice reached lowering as `arrvals`, but
`splat_disposition` recognized only default/abort operators. Its fallback
dropped `taint_off`. The existing disposition carrier now retains that
modifier without a default word or abort, preserving the zero-element result
of an empty undispositioned list splice. Populated, empty and genuinely
tainted failed-scratch copies are independently asserted.

Signal protection had two separate edges. Scratch `LifetimeStart` uses a
meaningful-zero definition recorded among resolutions. The signal planner
mistook it for the end of the pending acquisition/attest guard, dispatching
EXIT before the capture or read was published. Actual injected traces showed
the scalar `"bad"` write and array `"bad\0good\0"` read followed by a handler
that observed empty data. `LifetimeStart` now explicitly cannot resolve
that guard.

Conversely, a caught scratch predicate must finish signal deferral on failure
without cleaning its data. Its source origin identifies the subject for
guard completion on either outcome. Pending-signal tracking is shared by the
regex/read and SSA terminal-taint analyses, and signal planning precedes both.
This avoids a second private implementation of the signal state machine.

## Before and independent reversions

The frozen compiler is the accepted proof-unit binary, with a byte-identical
runtime archive. Final scratch has 109 cases: 51 retained and
`scratchperm.1`-`.58`. Signals has 128: 126 retained and `scratchsig.1`-`.2`.
Before the change, exactly 48 new scratch assertions fail natively and 49 in
C; both new signal assertions fail in each mode. Every retained assertion
passes. The extra C failure is `scratchperm.54`, the negated attest-all list.

All thirteen independent mutations were rebuilt and exercised in both modes.
The table gives the actual red new IDs; all other cases in the respective
file stayed green.

| Mutation | Red IDs, identical in both modes |
|---|---|
| Disable ordinary-read enforcement | scratchperm.2,3,7,14,17,23,24,26,28,30,32,33,35,36,40,41,42,47,48,50 |
| Restore R2 for scratch too | scratchperm.1,5,6,9,11,12,14,22,26,27,28,29,30,31,32,33,34,35,36,37,38,39,41,42,43,44,45,46,48,50,51,52,54,56,57,58 |
| Remove R2 for ordinary bindings too | scratchperm.19,20,21 |
| Invert logical RHS arm selection | scratchperm.5,6,7,9,10,11,12,13,52,54 |
| Leave consumed RHS results implicitly checked | scratchperm.8,9,53,55 |
| Drop skipped-RHS negation | scratchperm.12 |
| Skip trusted-cell operand reads | scratchperm.30,50 |
| Drop bare trusted-splice metadata | scratchperm.25,57,58 |
| Drop handler input restrictions | scratchperm.41,42,47 |
| Retain expired permission at reconstruction | scratchperm.38,39,46 |
| Remove the eligible predicate-subject exemption | scratchperm.56 |
| Let fresh zero end signal protection | scratchsig.1,2 |
| Keep protection after caught-predicate failure | scratchsig.1,2 |

One prediction was too narrow: skipping trusted-cell operands also breaks
the redirect-index assertion `.50`, not just `.30`. Its xtrace shows
successful compilation/execution without SB5107 in both modes. Normal
scalar redirect spilling sends the index through that same canonical value
walk; the later unlowered redirect target is only the spill reference.

The signal mutations fail for different measured reasons. Premature release
prints empty data and exits 143. Failure to release runs the unwanted
`delayed` output, then the ordinary EXIT path, with status 0. Both controls
retain complete strace evidence in their xtraces and assert the intended
write/read and SIGTERM injection, not just an error string.

The initial final tainted-copy fixture used forbidden `<<<` and correctly
received SB4050. It was replaced with `write bad | read _a`, producing exactly
the intended kernel-read bytes. That invalid fixture was not a compiler bug.

## Acceptance and remaining trial

All mutations are restored. The restored source/compiler/runtime/lock
identities match the positive snapshot. Compiler units are 241; the existing
storage-reference unit also compares readonly/mutable traversal and trusted
direct indices. The taint-sink dispatcher refuses its structurally impossible
binding-only read arm rather than inventing a speculative name-based answer:
that variant is produced only by the propagation-enabled ordinary-read walk.
This also keeps the touched dispatcher within the unchanged complexity rubric.

The full serialized gate passes with `NEW=120 FLIPS=0 RETIRED=0 TOTAL=12331`.
Rows 7171549-7183879 contain exactly 12331 distinct checks: native 6277,
C 6054, 12318 passing and 13 existing skips. There are no failures, flips,
retirements, ghosts or plan mismatches; liveness is 248. All 12211 accepted
proof-baseline results at rows 7159338-7171548 are retained unchanged.
The final scratch/signals/regex files pass 109/128/262 checks per mode.

After the gate, two lowering comments were clarified; no executable source
changed. Final publication identities are recorded separately from the
pre-comment positive. Evidence remains in the session's
`files/glob-trial-20261117/caught-scratch/`: frozen/positive artifacts,
before and reversion results, signal traces, source patches and gate log.

No runtime, dependency, compiled-regex cache or persistent runtime allocation
was added. This closes the caught-scratch unit, not the integrated owner.
Regex text/boolean result handling, `rglob` and the useful-program, helper,
converter, memory and time evaluation remain. No fresh coverage or adoption
on main is included.
