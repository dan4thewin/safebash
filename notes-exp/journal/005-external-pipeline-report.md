---
title: "The fourth pair: external pipelines with retained outcomes"
date: 2026-10-31
---

DG asked to continue. Before the multi-stage slice, the AI presented
completion alternatives; DG selected:

> Retain every stage outcome; checked pipelines fail on any stage; use the
> rightmost failure for numeric status.

[DD#3](../decisions/DD03-external-pipeline-outcomes.md) records that choice.
The AI implemented it without changing the compiler, producer, native
runtime, or their dependencies.

## The pair

`pipeline_report.cpp` and `pipeline_report.sash` retain two real functions,
a parent-side while reader, a reference collection parameter, captures
inside the loop, and a later for traversal. External printf emits six
unsorted, duplicated records; env supplies `LC_ALL=C` to sort, and uniq
reduces the stream to Ada, Edsger, Grace, and guest.

The function keeps the three workers, skips guest, and finishes at four
seen and three kept. A separate condition detects failure from
`/bin/false | /bin/true`, demonstrating that a successful last stage does
not hide upstream failure. Both binaries match the independent ten-line
expected report. All four pairs now run through the real sash CLI as well
as ordinary C++ builds.

This uses the existing explicit letters-only gate and newline-free capture.
It does not add a taint system, environment-override framework, collection
semantics, or source-level capture-newline choice.

## Runtime and ownership

`Command` owns an executable and argument vector. `try_read_pipeline`
returns `PipelineResult`; `read_pipeline` checks it. Both pass an explicit,
borrowed `LineReader &` to the parent callback.

The reversible implementation choices are fixed-size optional child slots
and one preallocated data pipe per stage, including the terminal reader.
They keep ownership addresses stable without moving child owners.
All command words are checked before any fork. Launch reuses the existing
completion pipe and shared setup cell, wires stdout first, then stdin for
later stages. Parent stdin is never rebound.

The optional stage results distinguish unstarted stages from actual
outcomes. A known pre-exec failure remains setup, not ordinary exit 127.
Setup/cleanup errors are separately recorded with zero-based stage indices,
in encounter order. Checking first raises the earliest setup error,
otherwise the rightmost failed process outcome; only then is a signal
encoded as 128 plus its number.

Normal, partial-construction, and exceptional exits close all remaining
parent endpoints before waiting. Explicit cleanup attempts each endpoint
even when another close fails. During consumer unwinding, the pipe-owner
vector is destroyed before the child-owner vector. Successful cleanup
preserves the consumer's original exception. All ordinary stage verdicts
are checked only after completion; consumed state is not rolled back.
There is no automatic force-kill or runtime timeout.

## Findings and corrections

The initial result representation let a parent close/wait error occupy a
stage's sole outcome slot. That could hide the child's eventual exit or
signal, contradicting the selected retention policy. Separate setup-error
storage removes that loss. The launcher returns observed child setup
failure distinctly inside the shared implementation, preserving the old
single-command API while allowing pipelines to distinguish parent errors.
After an ordinary wait error, owned cleanup reaping retains any recovered
status; genuinely unavailable ECHILD status remains an error, not zero.

The new higher-ordinal child-close injections initially failed the
parent's dynamic loader while closing libstdc++, before main or any fork.
The recorded trace and loader diagnostic identified the harness bug.
The fix statically links only the small Linux supervisor and adds a
controlled `--close-prelude N` mode: N invalid close calls consume the
parent's injection before exec, while fork children receive fresh counters.
All intended pre-exec failures are still required in the actual child's
trace, including resumed records. Ordinary examples do not become static.
Both compilers support this fault-only helper.

The relay fixture flushes each chunk instead of inheriting the old stdin
helper's EOF buffering, so the two-way producer/consumer handshake can
pass through multiple stages. Original stdin behavior is unchanged.
The stdin-install fixture helper was extracted with its existing
already-fd-zero guard intact.

Later-fork and initial-read injection uses producers that cannot exit
before the selected entry. This prevents an earlier SIGCHLD-induced
restart from consuming strace's later ordinal. Finite, small records are
used for close-error probes because strace suppresses the actual close:
its artificial leaked reader must not hold an endless producer alive.
Pipe endpoint selectors come from baseline pipe2 results, not hard-coded
fd numbers.

## Evidence

GCC and Clang pass twenty-three fixture groups, all four C++ goldens,
focused first/later-fork, retained-wait, and existing foreground probes.
Host-GCC ASan passes the same fixture/golden gate without ptrace.
DG's supplied sash compiler and companion runtime pass all four native
goldens without disabling compiler passes.

The pipeline fixtures exercise a 256 KiB binary line, empty/final records,
three-stage streaming through a two-way handshake, exits `[7,0,0]`,
`[7,3,2]`, and `[0,4,0]`, signal identity, late invalid argv before launch,
late failed exec with unstarted stages, partial pipe allocation, original
stdin, nested pipelines/captures, private descriptor inheritance, closed
standard slots, repeated completion under a small fd limit, consumer
exception preservation, and genuine unavailable child statuses.

The error catalog was predicted at 151 and lists/runs exactly 151 unique
cases: 73 basic, 61 faults, and 17 recoveries. Both ordinary compilers
pass all of them. The 39 additions show pipeline outcomes, construction,
parent and child wiring/ownership, and EINTR. A caught parent-close probe
separately confirms the original EIO and the producer's actual exit zero
are both retained.

Eight new catalog controls reject runs with injection removed. Both new
focused selectors reject uninjected execution. Three implementation
negative controls fail for the intended reason:

1. Selecting the first failure instead of the rightmost turns the exact
   stage-outcome fixture red.
2. Storing a parent wait error in the stage slot masks recovered exit zero
   and turns the retained-wait fixture red.
3. Waiting before pipe closure deadlocks the restart-safe later-fork
   fixture; an external timeout below an unaffected subreaper contains it.

After each control the passing source was restored; the final runtime
and platform blobs match the GCC/Clang/ASan-tested source byte-for-byte.
Formatting passes. Positive traces stay in compiler-specific build
directories; negative-control logs/traces stay in session artifacts.

## Boundary

This is multiple external stages plus a terminal parent reader, not forked
C++ function stages, call-context propagation, stage/function suppression,
negation, public wait, general redirects, background jobs, END/signals, or
the remaining value-operation library. Signal inheritance and no-force-kill
remain DD#2's selected model. macOS backing storage is still the next
platform choice.

The separate producer task remains the local, uncommitted draft, as DG
requested. There is still no IR reader or emitter.
