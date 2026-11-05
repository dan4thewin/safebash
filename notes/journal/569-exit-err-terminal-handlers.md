# 569 - Checked EXIT and ERR terminal handlers

2026-11-05. DG chose actual process-global storage and ERR as a generated
function, explicitly putting optimization below that reasoning model.
The storage, typing, placement and strict-resolution prerequisites are in
journals 565-568. I implemented the remaining DD128 contract on 2ad11d3a:
generated handlers, their distinct checks, cause-based dispatch and
signal protection. DD128 is now accepted and W5 is complete.

DG's decisions remain the authority: reject END; explicit full-i64 zero
selects EXIT and nonzero selects ERR; signals and fall-through select EXIT;
attempt ERR once on the first fatal fault, including from EXIT; require
failed-attest replacement directly in its branch; allow adjacent, caught
attestation of genuinely fresh ERR input. SB5071 applies uniformly.

## Real globals, independent entries

The parser and AST carry separate optional EXIT and ERR bodies. END gets
a migration refusal, not an alias. Function definitions remain legal only
directly in main's statement list. Executable statements and calls in both
handlers still participate in every whole-program pass.

Lowering builds independent SSA roots, with input versions that alias the
existing process-global cells. Native and C emit private callback functions.
There is no nonlocal jump into main, copied final-state snapshot, or new
source-language scope. A handler sees completed writes, including those
made before an early abort or a later-discovered producer failure.
An unfinished assignment has no result to publish.

Collection initialization uses actual declarations, not provisional entries
in the checker's collection map. The initial broader seeding produced false
SB5055 errors; it is not retained. Function-local bindings and compiler
temporaries remain private, with journal 568's fresh-entry initialization.

The runtime records terminal progress before cleanup or callback execution:

```text
main -> EXIT -> termination
main -> ERR  -> termination
main -> EXIT -> ERR -> termination
```

Full-i64 status reaches the selected callback before final process-byte
truncation. Falling off a handler preserves its entry status; explicit
exit overrides it. Missing handlers do not substitute for each other.
Signals cannot re-enter a handler, and children inherit an explicit
no-handler state.

Arithmetic and heap faults use the same fatal dispatch, including startup
heap-mapping failure. Entry itself allocates nothing. A second fatal fault
in ERR terminates rather than retrying, re-entering or inventing a value.
This is a chance for cleanup, not a promise that cleanup can allocate,
finish, or survive SIGKILL.

## Incoming trust and actual operational failures

EXIT's ordinary uses impose requirements on actual orderly entries,
including zero exits from a failed attest and the false-reader path.
ERR conservatively treats a global as unresolved if main-process execution
could have acquired tainted input into it, even if a normal path later
attests it. Potential sources inside called helpers, mapped slots and EXIT
count; uncalled templates and child-only writes do not.

The source checker reuses independent summaries and incoming-origin
ownership obligations. A partial fresh acquisition cannot authorize an
attest of inherited input on the other branch. The first implementation's
name-only probe missed that distinction; retaining `Incoming` provenance
and its existing attest obligations closes it for both scalars and slots.
The SSA backstop independently classifies handler inputs and rejects
inherited-input attestations. Neither is merely trusting the other gate.

SB5101 checks live same-process lowered ERR instructions, including inlined
helper bodies, and reports the call and failing operation. Catching a
helper's returned status does not catch an earlier abort inside it.
Likewise, a prefix on an outer command does not catch operand or setup
failures. The former blanket END unchecking is gone.

Conservative value facts recognize safe nonempty dispositions, static
regexes, bounded argv traversal and known even-length map conversion.
Mutations invalidate the relevant proofs. Unknown values do not receive
success-shaped assumptions. Descriptor captures, bulk/slurp reads and
redirects currently have operational failure paths with no adequate ERR
handling form, so they are refused even where an enclosing status is
caught. Arithmetic faults and heap exhaustion remain the explicit fatal
exceptions, not reasons to prohibit all arithmetic or allocation.

Fresh ERR input can still be acquired, caughtly attested and read normally.
Its failed branch must resolve its subject directly or explicitly terminate.
Literal overwrites and late attests cannot bless inherited input.

## Signal windows and interrupted captures

One shared plan describes semantic acquisition/resolution windows.
Pending signals are checked before entering a protected acquisition, then
dispatch into EXIT is deferred until the required resolution completes.
Receipt still updates the signal flag; ordinary failures, explicit exits
and fatal faults are not deferred.

The plan follows single-use returns, captures, condition edges and reader
completion, not just adjacent instruction numbers. Both explicit SSA
checkpoints and direct native/C polls consume it. A reader false edge is
not treated as if its body attest ran. The actual post-reader backend polls
remain present and have their own removal control.

`sigguard.15` injects SIGUSR1 at the second `rt_sigaction`, while installing
SIGUSR2 FLAG after SIGUSR1 EXIT. Its trace confirms the intended registration.
The pending signal must dispatch before getenv acquisition: status 138,
output `initial`. This pins the pre-acquisition poll without a timing race.

Main, EXIT and ERR have separate heap-backed linked capture stacks.
Dispatch selects the handler's existing head without allocating, closes
reader scopes and restores the invoker's descriptors. It cannot append to
an interrupted capture's borrowed frame. The old fixed 64-frame ceiling
is removed. Capture memfd, dup2 and rewind failures now abort explicitly
instead of producing an empty successful result.

The final nested-resource fixture uses `completed`, not the reserved word
`done`. Main and EXIT captures are both live when ERR captures cleanup;
the result is `:cleanup`, status 2. The child fixture handles its pipeline
with an `if`, not the illegal pipeline `||` form. It proves that a child's
arithmetic fault does not run the parent's ERR and that the parent can
continue to orderly EXIT.

## Source evidence and independent controls

`terminal.sh` has 81 cases: errfail 21, sigguard 15, errentry 8, errproof 8,
errresource 12 and handler128 17. Final focused runs pass 81/81 in both
modes, including after every mutation is restored. The complete sweep also
passes every case.

A pre-change parser rejecting EXIT/ERR says nothing about the new checks.
Sixteen distinct reversion mechanisms were therefore built and executed
independently. The final red sets below agree between native and C.
Earlier 72-case runs are retained; expanded controls were rerun against
the final 81 cases where their new witnesses mattered.

| Reversion | Red cases per mode |
|---|---|
| Name-only source input instead of incoming provenance | handler128.16, .17 |
| Omit independent SSA inherited-attest check | errentry.1, .2, .4-.7 |
| Ignore deferral at direct backend polls | sigguard.1-.3, .10-.12 |
| Retain nonempty proof after mutation | errproof.2, .3 |
| Omit live ERR operational checks | errfail.1-.6, .10, .11, .14-.21; errproof.1-.3, .5 |
| Ignore deferral at explicit checkpoints | sigguard.1, .2, .12 |
| Route nonzero fall-through/signal status to ERR | sigguard.1-.3, .9, .11, .12; handler128.8 |
| Select explicit exit destination after byte truncation | handler128.6 |
| Reuse main's capture head in handlers | errresource.7, .11 |
| Omit the pre-acquisition checkpoint | sigguard.15 |
| Omit the fork-child terminal mark | errresource.12 |
| Mark initial EXIT entry as already in ERR | errresource.6, .11; handler128.11 |
| Omit source handler requirements/EXIT entry reports | sigguard.4, .7; handler128.12-.14, .16, .17 |
| Classify SSA handler inputs as clean | sigguard.5; errentry.8 |
| Remove actual post-reader native/C polls | sigguard.9 |
| Permit exactly one extra same-handler entry | errfail.12; handler128.10; errresource.10 |

Paired backend edits are one mutation build exercised separately in each
mode, not two independent builds. The re-entry control is deliberately
bounded: one extra entry, then termination. `errresource.8` stays green
under it because its second message is diverted into an already-live ERR
capture; it is not claimed as an independent re-entry witness.

The actual pre-capture-fix runtime fails exactly errresource.1-.3 and .9
against the final fixtures. These pin the three capture syscall failures
and the arbitrary depth ceiling. The current runtime archive is restored
byte-for-byte after this control.

Early fixtures with the wrong signal-registration count, pipeline `||`,
reserved `done`, or invalid deep-capture source are not evidence. Their
results are preserved and distinguished from the corrected runs. In
particular, the first capture-head mutation had two invalid fixtures in
addition to its two genuine failures; the final rerun has only the genuine
errresource.7/.11 pair.

## Migration, gate failure and acceptance

Eighty-four old dual-mode END fixtures migrate to fresh `h128.*` IDs,
retaining their relevant assertions under the selected new handler
contract. The converter gains native-only `57.193`. Artifacts use schema 2
with separate `:exit-block` and `:err-block` fields, including empty programs;
an empty handler remains distinct from an absent handler.

The first full gate predicted NEW=331, RETIRED=168, FLIPS=0, TOTAL=11157.
Counts, plans, registry coverage and liveness were correct, but four
checks failed: 109.1 and 109.9 in each mode. The appended CLI xtraces
showed successful extraction/emission of schema 2 while the fixtures still
demanded schema 1. This was a missed fixture migration, not a successful
gate or evidence of a compiler failure.

Those assertions now have fresh IDs `h128.109.1` and `h128.109.9`.
The original compiler and its matching runtime fail exactly those two
out of all 78 CLI checks, in both modes. The current pair passes 78/78.
The structural and byte-identical embedded-versus-flag assertions remain.

The corrected gate passes:

```text
NEW=4 FLIPS=0 RETIRED=4 TOTAL=11157 timeout 900 mk -p 1 gate
rows 6811808-6822964
native 5690 + C 5467 = 11157
11144 passing + 13 existing skips
250 seconds; compiler/converter/expand units 238/27/20
liveness 253; no ghosts, plan mismatches or unexpected changes
```

The failed first sweep is retained separately at rows 6800651-6811807.
The four obsolete schema assertions retire; they are not reported as four
failure-to-pass flips. Across this unit, 86 dual-mode migrations plus the
81 new terminal cases and converter case give exactly 335 new mode IDs
and 172 retirements. All 10822 retained original statuses are unchanged.
The comparison uses saved IDs and strict row boundaries, not a repeated
working-tree commit hash.

The restored compiler matches the frozen functional positive. Final source,
spec, compiler, helper and runtime hashes match before and after the
accepted gate; a separate runtime rebuild is a no-op with the same archive.
No reversion, instrumentation or probe implementation remains active.

## Documents and port accounting

DD56, DD55, DD24, DD89, DD120, DD121 and DD127 are rewritten coherently,
not contradicted by later amendments. DD104/DD109 describe schema 2;
the scope, typing, reader and keyword companions, GUIDE, manual, converter
guidance and AGENTS agree. The general by-form fork-reporting question
remains open; only its obsolete END-silence premise is retired.

W5's B96/B97/B109/B110/B111/e7 claims are now accounted for under DD128.
H327 and H329 become reimplemented; five already-superseded walker rows
lose stale W5 remainder notes. All 330 rows retain their first fourteen
metadata fields exactly. Final counts: 246 ported, 62 reimplemented,
7 superseded and 15 dropped, with no pending rows.

Evidence belongs to the session archive
`files/port-dd128-handlers-probes.tar.gz`: before/after and reversion
results, source identities, full matched xtrace blocks and both gate
records. The control index distinguishes corrected evidence from invalid
early fixtures. Frozen binaries and final gate/commit/push records remain
separately available. Independent backlog, formal port wrap-up and any
resumption of coverage are not silently included in this handler unit.
