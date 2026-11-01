## Collection runtime: missing-cell reads settled

DG selected [DD#6](decisions/DD06-missing-cell-reads.md): missing string
cells read `""`, missing integer cells read `0`, and reads never insert.
Stored empty/zero cells remain present, while dispositions keep their
ordinary scalar value semantics. The
[journal](journal/012-collection-missing-cell-policy.md) records the choice.

This settles one policy for the proposed collection/argument slice, not
its implementation. Negative indices, what length counts, and snapshot
versus live traversal remain open; sorted `std::map` order stays selected.

## Sixth pair: lifecycle, explicit END, and signals complete

The standalone C++ runtime now has one program boundary for normal
completion, typed explicit exit, known uncaught body failures, and
cooperative signal shutdown. END runs in the owner after successful
reader/capture unwinding, at most once, preserving the triggering status
unless it explicitly exits with another.
[Journal 011](journal/011-lifecycle-runtime.md) records implementation
and controls.

[`lifecycle_report.cpp`](../sash-cpp/examples/lifecycle_report.cpp) and
its [sash counterpart](../sash-cpp/examples/lifecycle_report.sash) retain
collection state from a captured function, discard only another function's
returned int, and explicitly catch/suppress END operations. No source
functions are defined in END. The native front end still does not enforce
DG's planned strict END contract; no producer or native code changed.

Signal handlers only record lock-free integers. Most-recent FLAG information
and pending EXIT identity stay separate. Explicit checkpoints perform
ordinary-context shutdown; reads/waits retry EINTR, without cancellation,
force-kill, or runtime timeouts. Lifecycle re-entry guards do not uncheck
called function bodies.

GCC, Clang, supported Linux ASan, real Apple Clang/libc++, and Apple ASan
pass 35 fixture groups and six C++ goldens. The supplied native compiler
matches six sash goldens; both Linux compilers retain the 169-case catalog.
Six new signal-call fault checks reject missing injections. Private
handoff diagnostics stay allocation-independent, and a failed consumer's
acknowledgement writer closes before producer reap. Three
isolated mutations fail for the intended reasons: conflated FLAG/EXIT,
automatic END handling, and child END execution.

The broader [runtime task](tasks/cpp-transpiler-runtime-before-ir.md)
remains open: value/argument operations, general redirects, public wait,
background/forked-C++ forms, and their inherited-reader ownership.
There is still no IR reader or emitter. The
[Darwin fault harness](tasks/cpp-darwin-fault-harness.md) remains follow-up
tooling; ordinary lifecycle cases now run on the actual Mac.

## Planned END contract: explicit handling, END-only

DG expects END to forbid source function definitions and require every
fallible operation to be explicitly caught or suppressed, with neither
automatic abort nor automatic suppression. DG clarified that nothing
outside END changes. See [DD#5](decisions/DD05-planned-end-explicit-handling.md)
and [journal 010](journal/010-planned-end-explicit-handling.md).

The automatic per-step cleanup-guard proposal is withdrawn. The next C++
lifecycle slice targets explicit END handling, while ordinary functions,
DD#55's no-reach rule, and non-END behavior remain unchanged. This is a
planned source contract. That recording changed only documentation;
the C++ prototype is now implemented above, while native enforcement remains future work.

## Rebased onto current hand-parser; function discard clarified

The twelve experiment commits rebased cleanly onto hand-parser `2fc6a22c`.
All replayed patches match, and the producer draft stays unchanged and
untracked. See [journal 009](journal/009-hand-parser-rebase-and-function-discard.md).

DG's 2026-10-29/30 [DD#55 ruling](../notes/decisions/DD55-error-model.md)
supersedes the complete-call suppression assumption: `- f` discards only
f's returned int or status. Function bodies, arguments, and nested calls
keep their own checks; an in-process void call has nothing to discard
(SB4076). The inherited checkedness-parameter proposal is withdrawn.
Existing C++ checked operations already keep that boundary; no runtime or
producer code changed.

The next critical-path slice remains program shutdown, END, and signals.
Function checkedness is no longer an open fork for it. Setup-failure
suppressibility and residual `-` forms remain native backlog questions.

## macOS runtime bring-up complete

The same shared C++17 runtime and five source-shaped programs now run on
an actual Apple Silicon Mac with Apple Clang/libc++. Shared POSIX code owns
launch, setup reporting, I/O, capture restoration, and child accounting.
The Linux adapter retains memfd/pipe2; the Darwin adapter supplies private
immediately unlinked capture files, owned pipe/CLOEXEC setup, and CRT
environment access. Public interfaces and pipeline outcome policy are
unchanged. See [DD#4](decisions/DD04-macos-posix-adapter.md) and
[journal 008](journal/008-macos-posix-runtime.md).

All 28 fixture groups and five C++ goldens pass on Linux GCC/Clang,
supported host-GCC ASan, Apple Clang, and Apple ASan. The native Linux
compiler matches five sash goldens; both Linux compilers retain the
169-case catalog. Real-Mac call-error controls and unlink/CLOEXEC
mutations pin failure cleanup and keep the checks capable of failing.
The [repeatable Darwin fault harness](tasks/cpp-darwin-fault-harness.md)
remains follow-up tooling.

Examples use BSD/GNU-common utility spellings, and signal-set calls tolerate
Darwin SDK macros. No producer, native language, or dependency changed.
Linux END and signal-response choices remain TBD; this port
does not choose them.

## Fifth pair and raw pipeline capture complete

[`capture_report.cpp`](../sash-cpp/examples/capture_report.cpp) and its
[sash counterpart](../sash-cpp/examples/capture_report.sash) capture a
five-stage external pipeline in a real function, nest it inside a framed
function capture, and retain two rounds of summaries for later traversal.
Both match the independent eight-line expected report.

The runtime now provides `try_capture_pipeline` and `capture_pipeline`.
Exact bytes and pipeline outcomes are separate; failed producer output and
bytes preceding completion errors are retained. The checked wrapper checks
before returning its assignment value. The raw collector shares pipeline
launch/drain/wait ownership without memfd or parent-stdio rebinding.
Read/allocation exceptions propagate after successful close-before-reap
cleanup, rather than becoming EOF or a stage verdict.

GCC, Clang, and supported host-GCC ASan pass twenty-seven fixture groups and
all five C++ goldens; DG's supplied native compiler matches all five sash
goldens. Tests pin binary bytes, final newlines, nested reader/memfd scopes,
closed standard descriptors, allocation/read cleanup, and retained
completion output. Three implementation mutations fail as intended and
rebuilt restored-source controls pass.
See [journal 007](journal/007-raw-pipeline-capture.md).

The sash capture/status/unconditional-attest sequence was repaired earlier
in [journal 006](journal/006-capture-report-repair.md). The native
[branch-policy question](../notes/tasks/q-capture-status-attest-sequence.md)
remains open; no native compiler or language rule changed.

## Fourth pair: sorted, deduplicated pipeline records

[`pipeline_report.cpp`](../sash-cpp/examples/pipeline_report.cpp) and its
[sash counterpart](../sash-cpp/examples/pipeline_report.sash) put printf,
C-locale sort, and uniq before a parent-side `while readline`. A real
function fills its caller's collection by reference, captures labels in
the loop, and leaves three workers for a later for traversal. An
upstream-failure condition succeeds in detecting a failed first command
despite a successful last command. Both produce the independent ten-line
report.

DG selected all-stage outcome retention, failure on any stage, and
rightmost-failure checked status in
[DD#3](decisions/DD03-external-pipeline-outcomes.md). The runtime adds
`read_pipeline` and `try_read_pipeline`, reusing launch and `LineReader`.
Setup/cleanup errors stay separate from known child exits/signals;
unstarted stages have no invented status. All remaining parent endpoints
close before any child wait, including partial construction and
consumer unwinding. Parent stdin is never rebound.

GCC, Clang, and host-GCC ASan pass twenty-three fixture groups and all
four C++ goldens. DG's supplied native compiler passes all four sash
goldens. Streaming is pinned by a multi-stage two-way handshake; later
fork and retained-wait probes check actual syscall/outcome evidence.
Three implementation negative controls pin rightmost status, outcome
preservation, and close-before-wait.
See [journal 005](journal/005-external-pipeline-report.md).

## Runnable error-path showcase

`sash-cpp/examples/error_paths.cpp` and its shell runner now demonstrate
169 isolated cases: 85 basic cases, 65 injected faults, and 19 interrupted
operation recoveries. Each displays stdout, stderr, and status.

```sh
cd sash-cpp
mk -p 1 error-demo
```

Checked errors stop their operation's program; caught/discarded errors
continue. A real exit 127 stays distinct from failed exec, signal outcomes
retain their identity, and a failed capture leaves its target unchanged.
The catalog includes descriptor/capture setup, reads/writes, multi-stage
construction and stdin wiring, process completion, failed diagnostics,
and fatal restoration/reap/close/unmap.
A Linux subreaper supervises traced examples so the fatal-wait case leaves
no orphaned fixture.

GCC and Clang pass the full showcase, whose predicted and actual totals
are both 169. Six new capture catalog controls reject uninjected runs.
Capture read probes calibrate first/second positive data reads by fd;
the no-memfd probe has a separate injection canary. Earlier pipeline
controls remain in place. Higher child-close probes
use the small static supervisor's controlled prelude to avoid failing the
parent loader before main; generated-program examples remain ordinary
C++ builds.
The full showcase deliberately runs without sanitizers. Its allocation
boundary case explicitly throws; the separate capture fixture denies a
real collector allocation using a test-only override, not host memory
exhaustion. Arbitrary internal corruption and future features are not claimed.
See [journal 003](journal/003-error-path-showcase.md) and
the [runnable catalog](../sash-cpp/README.md#error-path-showcase).

The [low-level tour](../sash-cpp/docs/error-path-tour.md) now follows
representative paths through both harnesses for C programmers new to C++
and strace injection. It covers result versus exception propagation,
capture unwinding, fault-site calibration, retries, fatal cleanup, and
the pipeline/close-prelude paths, raw byte capture, retained failure output,
consumer read exceptions, and the allocation/absence controls.

No compiler, producer, or native dependency changed.
The first three pairs remain available. Their earlier milestones are in
journals [001](journal/001-first-cpp-program.md),
[002](journal/002-job-report-and-foreground-processes.md), and
[004](journal/004-streaming-worker-report.md).
[DD#2](decisions/DD02-foreground-fork-exec.md) remains the selected
single-threaded fork/exec model, including signal inheritance and
no-force-kill.

The [runtime task](tasks/cpp-transpiler-runtime-before-ir.md) stays open.
The initial macOS slice is now complete alongside the Linux runtime.
Remaining work includes call-site result handling,
background jobs, replacing exec, forked C++ stages, general redirection,
further pipeline forms, escaping reader control transfers and public wait,
END/signals, and most value operations.

Existing [sanitizer tooling limitations](tasks/sanitizer-host-compatibility.md)
and the separate native
[rustix issue](../notes/tasks/bug-fresh-clone-rustix-runtime-private.md)
are unchanged.

Work remains on `cpp-runtime-first-slice`. Experiment records stay here,
not under the native compiler's `notes/`. The independent producer plan is
still the local, uncommitted draft
`tasks/emit-ir-json-for-c-transpiler.md`, as requested. No IR reader or
emitter has been implemented.
