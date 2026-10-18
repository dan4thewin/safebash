# 527 - The reader has an owner

2026-10-18. DG continued coverage from `292e20a`: 97 cold compiler
production lines. The selected cohort was thirteen lines in expression,
pipeline and statement lowering. Reading the producers found redirect
crashes, nested-reader data loss and a wait-before-read deadlock. Bugs took
priority over the coverage queue.

## Redirects belong to the command, not its setup

Argument materialization can leave assignments in the starting block and
the actual command in a different block. Attaching a command's redirects
to the starting block's last instruction attached them to an assignment
instead. A refused recursive call could also leave no instruction at all.
Both paths reached compiler invariants instead of producing the intended
program or diagnostic, in native and C modes.

Redirect attachment now follows the returned command block. An unconsumed
user-call redirect scope is discarded after the call has already been
refused, rather than inventing an instruction to carry it. The obsolete
wrapper is removed. All nine new `rarg` cases failed before the repair in
both backends; the original 217 command checks were passing.

## An ambient fd is not a nested binding

An outer producer writing `one\ntwo\n`, with an inner piped reader in its
loop, printed only the first outer iteration and exited successfully.
The inner reader had replaced the ambient captured fd; its EOF became the
outer loop's EOF. The earlier one-record control could not reveal the loss.
An initial triple-nested fixture was invalid and is not evidence.

There was a separate lifecycle bug: both emitters waited for producer
children before executing the consumer. A producer filling the pipe could
never finish, and the waiting parent never read it. Retained reader fds
were not closed at consumer completion either.

DG directed the semantics:

> why wouldn't each consumer simply keep doing reads until EOF or sigpipe,
> and only then evaluate child status?

The implementation follows that direction, not a spool/check-first
alternative. Start the producers, consume concurrently, close the read
end, reap, then apply DD#55's existing status rules. A scalar reader
completes after its record; a bulk reader drains to EOF. Early completion
may give a producer SIGPIPE. No producer is force-killed, and no forced
termination timeout was added.

Each reader site owns a private fd and producer array in both emitters.
Negative SSA fds encode the site, not one mutable ambient value. A shared
CFG pass inserts finishes on exits from a reader region, inner first.
Ordinary continuation retains its loop reader; escaping break/continue,
returns and tail calls release the regions they leave. Conditional exits
can use bridge blocks with repaired phi predecessors. Two direct API
tests pin that structural transformation and all terminator variants.

Normal completion publishes the aggregate producer result; nonlocal
transfers preserve their prior status/test after successful cleanup.
Scalar and bulk conditions still distinguish EOF from successful data.
Abort, explicit exit and END close all outstanding readers before reaping
and preserve the triggering status. Forked children discard inherited
reader fds without adopting the parent's children. Authored `wait` retains
an owned producer's status for its later pipeline finish, rather than
turning an already reaped child into false status 127.

The C emitter now skips unreachable blocks, as native already did.
Discovery-pass reader sites must not create references to declarations
that do not exist in the emitted program. Native bulk-read truth now uses
actual data length, matching C.

## Narrower scans were still the wrong mechanism

The next seven controls exposed the limits of post-SSA repointing. A
bounded loop's bulk read missed its pipe; redirected reads, inlined
functions and anonymous captures stole the outer loop's records instead
of reading their own input. Six valid controls failed before in both
backends. The redirected bulk-read case, `pipeown.38`, was already passing
and is a preservation control, not a failed-before regression.

Reader construction now consumes a lexical `VersionState::pipe_reader`
binding. Pipeline lowering installs and restores it; function bodies,
anonymous captures, redirected commands and background children suspend
and restore it. Both reader constructors share that mechanism. The broad
repoint scans and their special loop path are deleted.

`pipeown.1`-`.41` cover nested scalar/bulk/loop ownership, capacity-sized
streaming, parent stdin and mutations, early and nonlocal exits, END,
producer suppression/negation, constructor faults, interrupted waits,
authored waits and lexical input boundaries. Together with the old cases,
the pipeline file has 104 checks per mode.

## The remaining cold contracts

The original thirteen-site cohort did not justify thirteen new source
fixtures. Producer enumeration separates the source gap from invariants
and obsolete answers:

- An unused `-> int` definition with no return is legal. `fnnoret.1` pins
  that definition without inventing semantics for calling it.
- A real disposition subject lowers to a variable or cell. A computed
  positional count cannot reach that predicate through accepted source;
  the malformed lowered value now gets a pinned invariant, not a
  permissive answer.
- Parser concat producers already unwrap singletons. The redundant
  transform special case is removed.
- Every loop header phi key survives the second lowering pass. All three
  copies now call one helper; its direct test pins loss of a back-edge
  version instead of retaining a discarded first-pass value.
- An int collection initializer is a table literal at this seam. A scalar
  cannot silently pass through the integer-cell wrapper.
- Non-command redirect classification and non-fill parameter operators
  retain their correct decline answers, each asserted directly.
- Bare map `:?` already has a present operand, including an empty one.
  The unreachable generic odd-count message branch is removed.

Reachable-block emission also exposed C's missing-collection fallback,
previously hidden by emitting dead function templates. The direct
before-control failed because no panic occurred: the lookup returned a
null C pointer. All fourteen callers now share a loud missing-slot
invariant, matching native. The independently tracked tiered
`load_var_or_empty_c` lookup is unchanged.

Eight new compiler API units include the two CFG tests. One phi test was
initially nested inside a production function and did not register;
moving it to module scope brings the actual compiler count to 216.
Final staged fmt, clippy, totality and live complexity gates pass without
bypass. Runtime formatting was checked in its separate workspace.

## Fresh measurement

Complete-scope union: **82 / 30748 production lines cold**, down **97 -> 82**.
The original cohort closes completely: expressions **2 -> 0**, pipelines
**4 -> 0**, statements **7 -> 0**. The shared loop-phi helper also closes
control flow **12 -> 11**; removal of the obsolete reader path closes
statement handlers **9 -> 8**. No cold-file regressions. All **42** measured
source blobs are current; the new cleanup module, binding resolution and
both emitters are zero cold.

Raw cargo-llvm-cov: **99.21% line / 99.57% function / 94.85% branch**,
including inline test scaffolding. Exactly **9112 CLI profiles across all
32 slots** separate source from API reachability. The six original retained
guards/declines are each **CLI 0 / union 1**. The conditional cleanup bridge
is **0 / 4**; the direct Goto and Return rewrites are each **0 / 1**.
The no-return definition is **2 / 2**, and both reader constructors, all
lexical suspension boundaries and native/C reader start/finish emission
are source-warm. Saved both exports, merged profiles and the database
before ordinary restoration.

The instrumented sweep matches the independent prediction exactly:
**9249 = 9236 passing + 13 existing C skips**, native **4727**, C **4522**.
All **9147** baseline statuses at **6175105-6184251** are preserved, with
exactly **102** intended additions and no unexpected IDs, retirements or
previous-row flips. Liveness remains **260**. Its block is
**6202732-6211980**. Only the two `fnnoret.1` mode IDs are first-ever in
this sweep; the earlier 100 additions were already recorded by intermediate
sweeps. Source plans and both global collision checks agree, including
`coverage.sh`'s known two-check expansion.

## Ordinary publication gates

Restored with `mk cov-reset`, then ran the full ordinary gates:
**216 sash + 27 convert + 20 expand** units and the same **9249** specs.
The final block is **6211981-6221229**, with zero first-ever IDs. All 9147
baseline statuses, the exact 102 additions and the no-retirement/no-flip
gates pass again; liveness remains 260.

Default and configured native/C compiler/runtime canaries preserve both
outer records, independent nested readers and a called function's stdin;
the final public wait reports no pending child. Neither compiler nor
program emits a profile, and existing target profiles remain unchanged.
The separate runtime formatting check and live staged gates pass without
bypass. All 42 measured source blobs remain current after restoration.
Task executables, including the old buggy controls, are removed; evidence
logs, source fixtures and coverage snapshots are retained.

DD#122, DD#55 and GUIDE document the ownership, streaming and lexical
boundaries. The existing run-3 review scope includes the CFG/phi, fork,
wait and shutdown seams. No known coupled bug is deferred.
