---
title: "Raw pipeline capture and the fifth program pair"
date: 2026-10-31
---

DG asked the AI to finish capture-runtime after the separate sash example
repair in [journal 006](006-capture-report-repair.md). The AI completed the
raw byte collector, fifth C++ companion, focused fixtures, fault probes,
and experiment records without changing the producer or native compiler.

## Runtime and example

`try_capture_pipeline` returns `PipelineCapture`: an owning byte string
beside `PipelineResult`. `capture_pipeline` checks the result before
returning the string. Nonzero producers and completion errors do not erase
already collected output; a checked failure cannot replace the caller's
assignment target. Consumer read/allocation exceptions instead propagate,
without returning a partial capture or inventing a child outcome.

The implementation factors shared command preparation into `consume_pipeline`
and reuses the existing platform launch/drain/wait path. The terminal child
writes to a FIFO, the parent drains concurrently, and all remaining endpoints
close before reaping. No second child tracker, memfd dependency, parent-stdio
rebinding, buffer limit, or newline trimming is introduced. General in-process
capture still uses Linux memfd.

`capture_report.cpp` retains a summary function, a framed function capture,
two while-loop rounds, sparse saved summaries, and later for traversal.
It matches the repaired sash program's independent eight-line expected
report. The source probe saves status before its unconditional attest;
DG's native branch-policy question remains separate and open.

## Gates and evidence

The prediction was 27 fixture groups, five C++ goldens, five native goldens,
and 169 catalog cases: 85 basic, 65 faults, and 19 EINTR recoveries.
Those totals were observed. GCC and Clang pass ordinary/fault/catalog
gates and formatting; supported host-GCC ASan passes ordinary gates.
The native goldens use DG's supplied compiler and companion runtime.

Four new fixture groups pin exact NUL/CR/newline bytes, output larger than
pipe capacity, failed output with typed outcomes, setup versus consumer
failure, nested reader/memfd scopes, closed standard descriptors, and fd/
child lifetime. An initial fixture incorrectly passed a binary record
through OS argv; the fixture was corrected, not the deliberate NUL barrier.

The allocation fixture scopes a test-executable-only `operator new` fault
to collector-sized requests. A real string growth throws `std::bad_alloc`.
The trace confirms data reads before pipe closure and SIGPIPE/reaping of
both producers; descriptor snapshots and the unchanged target are asserted.
No shared-host memory exhaustion or runtime allocation override is used.

First/second read faults use one finite large binary producer that remains
live through the selected entries. Both harnesses calibrate the actual data
fd after an entry marker, avoiding exec-completion reads and restarted-entry
drift. Read EIO throws; EINTR preserves the complete binary result. A wait
EIO retains full bytes and recovered exit zero while keeping the first
completion error checked.

Six new catalog controls fail when injection is removed. Three source
mutations also fail for their intended reasons:

| Mutation | Rejected by |
|---|---|
| Trim captured final newlines | Raw large-byte fixture |
| Clear bytes when pipeline completion fails | Failed-output/outcome fixture |
| Add unnecessary memfd capture around the collector | No-memfd fault harness |

Each source restoration matched the original checksum. Rebuilt positive
byte/outcome/resource and fault controls then passed. The memfd probe first
requires a real injected memfd failure; the raw path subsequently succeeds
with no memfd call. Absence is not misreported as an injected syscall.

## Remaining boundary

This closes the raw external-capture milestone, not the entire
[runtime roadmap](../tasks/cpp-transpiler-runtime-before-ir.md).
Source newline policy, background writers, call-context suppression,
forked C++ stages, general redirects, public wait, END/signals, macOS
storage, and most value operations remain. The native branch-policy task
and existing sanitizer/native-build limitations are unchanged.

The IR producer plan remains the protected, uncommitted local draft.
No IR reader, emitter, or native dependency repair is included.
