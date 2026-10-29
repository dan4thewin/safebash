---
title: "Making the error paths visible"
date: 2026-10-28
---

DG asked for a demonstration of all the error paths. The AI added a
hand-written C++ example and a Linux runner for the implemented runtime
slice. No runtime policy, producer, compiler, or native dependency changed.

## What runs

`cd sash-cpp && mk -p 1 error-demo` demonstrates 103 cases, each in its
own process: 52 basic cases, 38 injected faults, and 13 EINTR recoveries.
The runner shows output, diagnostics, status, and the injected syscall.
`--basic` omits fault injection, `--list` lists names, and `--case NAME`
selects one. Raw evidence stays in the build directory's `error-demo/`.

The basic examples show checked, caught, and discarded outcomes, real
exit 127 versus failed exec, signals, NUL rejection, write and regex
errors, string capacity, and allocation exceptions. Capture writes partial
bytes before a command fails; caught/discarded handling then prints
`target: unchanged` and continues.

Fault probes cover capture inspection/storage/backup/install/restore,
including initially closed descriptors, reads and closes; pipe/shared
status/fork setup; child pre-exec close; completion reads and wait;
diagnostic writes; and fatal cleanup. Retry cases must retain the
uninjected output and exit status.

## Evidence that the intended path ran

Each ordinary probe first traces an uninjected invocation. An explicit
entry marker separates the program body from loader activity; the runner
derives the intended syscall's ordinal, then requires actual injection
evidence and the correct error site, not merely a nonzero exit.

The child-close probe is the exception: first-close injection is per
tracee, so the parent loader's first close also fails. Both injections
are shown. The trace must identify the actual fork child, its failed
close, and the absence of child exec. The first runner incorrectly missed
a close split into unfinished/resumed trace lines, although the runtime
already reported the right setup error. The parser now accepts both forms.

The fatal-wait probe faults both the completion read and the destructor's
wait. A small Linux subreaper supervises traced cases and reaps descendants
after the demonstration exits. A separate control confirmed it preserves
the direct command's exit 7, adopts and reaps its orphaned grandchild,
then reaches ECHILD. This is fixture containment, not a new runtime
shutdown policy.

Disabling injection makes both the fork and fatal-restore probes fail.
Re-enabling it makes the same cases succeed, with traces retained.
GCC and Clang run all 103 cases and preserve the fourteen existing fixture
groups and both golden outputs. GCC's prior Linux fault probes and
formatting pass. Host-GCC ASan passes the existing gates plus selected
capture-unwind, initially-closed-stdout, and allocation-boundary examples.
The full strace showcase is deliberately unsanitized.

## Limits

The allocation case explicitly throws `std::bad_alloc` rather than
exhausting a shared host. The completion-protocol case injects a synthetic
read count; it does not demonstrate a legitimate child writing that pipe.
Injected close errors skip the syscall, unlike Linux's real close-error
release semantics, so these probes do not claim to measure that release.
No shared-status corruption or uncaught programming error is manufactured.

There is no matching sash program for the entire catalog: API misuse and
synthetic OS faults are not accepted source forms. Whole-function
checkedness, pipelines/readers, general redirects, background/public wait,
END/signals, macOS storage, and the rest of the runtime remain on the
existing roadmap. No new design decision was needed for exposing the
already implemented failure handling. The producer draft stays uncommitted.

## Follow-up: a low-level tour (2026-10-29)

DG asked for a walkthrough for experienced C programmers and strace users
new to C++ and fault injection. The AI added
[`sash-cpp/docs/error-path-tour.md`](../../sash-cpp/docs/error-path-tour.md)
and linked it from the README and status.

The tour follows outcome checking, failed capture and RAII unwinding,
baseline-calibrated injection, EINTR recovery, ordinary versus destructor
restoration failure, fatal reap containment, child setup reporting, and
partial diagnostics. It explains the C++ syntax where it matters and
distinguishes helper, demo, and runner statuses. The examples follow the
current code and recorded traces, including the parent-loader side effect
of per-tracee first-close injection. This is documentation only; runtime
behavior and the uncommitted producer draft are unchanged.
