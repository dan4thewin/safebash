# 518 - Close and wait

2026-10-17. DG asked to continue coverage from `18b8b10`, with 204 compiler
production cold lines. Selected all nine in native `codegen/inst_handlers.rs`.
The first ten real-CLI probes failed seven native cases and six C cases.

## Construction failure is not a child pid

Both backends consumed pipe outputs and recorded fork results without checking
failure first. Native pipeline stages also had a private redirect implementation:
it ignored fd dups, opened read/write redirects incorrectly and could execute
the body after setup failed. C discarded its canonical redirect helper's
failure result. Native pipeline echo ignored write failure.

Pipe/fork results are now checked before use. Failure closes only successful
pipes and waits only already-started positive pids. Runtime pipe creation
records errno, relocates owned endpoints away from authored descriptors before
publishing either, and closes both endpoints if relocation fails. Wait retries
EINTR. Stage pipe dups are checked; redirects reuse canonical guarded setup.
Failed setup exits the child without executing its body, and failed echo writes
become stage failures. Explicit `3<>file` also needed the missing `LtGt` case
in adjacent fd-prefix recognition.

DG settled the consequential cleanup fork:

> "we can't know the harm a SIGKILL could cause - close and wait"

There is no kill, escalation or kill timeout. Waiting for a started stage can
take as long as that stage needs. DD#55 records the decision. A deterministic
regression fails the second parent clone while the first child sleeps and then
writes a marker: the caught handler must see that marker. Another interrupts
the wait first, proving the retry instead of an early return.

## A failed capture is not EOF

Lastpipe consumers originally ran after a failed constructor. Passing fd -1
to the reader, or manufacturing an empty read, would hide the distinction and
could clear an existing target. Shared `lower_pipe_consumer` now emits a real
branch around the whole reader/loop, joining version state with the established
two-branch phi helper. `SetTestFromPipe` tests readiness without overwriting the
construction status.

The private capturing-stage builder had also lost suppressed producer stages,
including the only producer of a capturing pipeline. It could construct a
zero-stage pipeline and ICE. Ordinary and capturing pipelines now share
`build_pipeline_inst` and the existing stage classifier, retaining suppression,
negation and inlined functions. Suppressing one producer is not suppressing
the whole pipeline; whole-operation tests use `- { ... }`.

Actual EOF exposed a separate, coupled read defect. Splitting empty input
returned without clearing the array; shorter input left old suffix cells.
The runtime now clears through `sash_table_clear` before splitting. C's
NULL-at-EOF guard had to go too, so empty input reaches that reset in both
backends. DD#99 now makes replacement, rather than append, explicit.

## FLAG was being mistaken for EXIT

Polling the latest `signum` against the program's compile-time EXIT set lost
the disposition at delivery. A FLAG could shut down, historical FLAG delivery
could become an EXIT when the signal was later rearmed, and EXIT without END
did nothing.

FLAG now writes only `sash_signum`; EXIT writes it and the separate pending-EXIT
atomic `sash_exit_signum`. The runtime ABI is
`sash_signal_flag(signum, exiting)`, selecting two handlers through canonical
sigaction registration and its naked restorer. Native polls atomically; C uses
volatile loads. Pending EXIT sets `128+signal`, enters END or exits directly
when absent. Both backends use canonical END-region membership to prevent
re-entry, not numeric block ordering.

DD#56's loop/function checkpoints were also missing. They now cover while,
until, static/array/argv loops, ordinary/captured/integer functions, tail-loop
boundaries and pipeline functions, alongside external/pipeline boundaries and
replacing exec. The first implementation inserted instructions immediately;
two-pass loop discovery cleared them. Injecting SIGTERM on the first authored
write exposed that, without an external command masking the missing check.
The transform now records block IDs and materializes `SignalCheckpoint` only
after lowering and END installation. FLAG-only programs get no polling
instructions.

## Contracts and gates

Removed the impossible pipe-index fallback and optional abort-message relay.
Close uses shared exact-arity, literal-integer and OS-fd-range decoding even
when unchecked. Signal's internal EXIT bit is decoded strictly too. Two new
direct-SSA units and extensions to the existing operand matrices pin malformed
backend inputs: validation and lowering cannot produce those shapes from source.

The staged complexity gate caught `intern_literals` at cc 5 / cog 6 after its
diagnostic strings changed. Flattening the instruction traversal keeps the same
order without a new helper and brings it to 5 / 4; no bypass.

The ordinary unit run exposed a fail-open gate: sash reported 158 passing /
1 failing, but `mk test` returned success after convert and expand passed.
Its three Cargo phases, and those of its build prerequisite, are now chained
with `&&`. Session-only Cargo wrappers deliberately fail sash's build or test
phase: mk must return nonzero and neither later crate phase may run.
No production Rust is mutated for those negative controls.

`run_sash_under` wraps only the compiled binary, not the compiler or gcc.
`strace` and util-linux `prlimit` are documented prerequisites; there is no
tracked CI provisioning file to update. The actual Linux syscalls are `pipe2`,
`clone` and `wait4`; strace does not follow children for these injections.
Low-fd probes account for the harness's inherited TAP/TSV descriptors. Separate
traces prove successful `pipe2([4, 5])` followed by EMFILE when relocating each
endpoint, not just a surface error that could belong to pipe2 itself.

## Measurement and gates

Added 29 pipeline and 17 signal specs, all dual-mode: 46 source checks /
92 database IDs. Historical prefix checks and both source collision checks
were clean. Per-file plans agree with literal counts except coverage.sh's
established three-iteration variable-ID loop.

Ordinary and instrumented full sweeps produce exactly the predicted **8789**:
**8776 passing / 13 existing C skips**. The first sweep introduced exactly 92
IDs; reruns introduce zero. All **8697 baseline IDs**, selected strictly at or
before saved rowid **5828251**, retain their statuses. The first-vs-current
comparison reports **260** differences, proving the regression machinery is
live rather than self-comparing one dirty-tree hash.

Final positive `mk test`: **159 sash + 27 convert + 20 expand** Rust units,
plus 152 C regex, 26 sash-grep and 5 sash-getopt checks. Both deliberate
build/test failures return 1 and stop their later crate phases. Formatting,
clippy, totality, whitespace and staged touched-function complexity gates pass.

Fresh serialized union, database load and provenance check:

| Cohort | Before | After |
|---|---:|---:|
| Native `codegen/inst_handlers.rs` cold production lines | 9 | 0 |
| C emitter | 25 | 21 |
| Transform statement handlers | 11 | 9 |
| Compiler total | 204 | 189 |

No other file gains cold production lines; earlier zeroed cohorts remain zero.
The database records 29850 production lines. Raw cargo-llvm-cov coverage is
**99.00% line / 99.43% function / 93.98% branch**.

API-only attribution uses **8624 CLI profiles**, matching the compiler
signature across all 32 merge slots and excluding the unit binary. The
nonliteral-close refusal is **0** in CLI profiles and **8** in the union,
while its valid continuation executes **92** times in both. EXIT-bit checking
is entered/returned **54/54** times in CLI profiles versus **66/54** in the
union: the twelve malformed decodes are direct-SSA API assertions, not rival
source tests.

After the final measurement, `mk cov-reset` restored ordinary release tools.
The positive unit gate, final ordinary full sweep and a persistent-directory
no-profile canary all pass. `mk cov-stale` still confirms current worktree
blobs. GUIDE, DD#55/#56/#99, AGENTS and status document the implementation and
gate corrections; no coupled finding remains deferred.
