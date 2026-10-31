---
title: "DD#4 - macOS adapter behind shared POSIX ownership"
status: accepted
---

DG chose macOS bring-up as the next milestone and supplied access to an
existing Mac. This follows [DD#1](DD01-runtime-starting-points.md), rather
than postponing the port until the Linux runtime is complete.

The AI's implementation preserves the existing public C++17 API,
[single-threaded fork/exec model](DD02-foreground-fork-exec.md), and
[pipeline outcome policy](DD03-external-pipeline-outcomes.md). Shared
launch, completion reporting, byte I/O, descriptor restoration, and child
ownership live in `platform_posix.cpp`. Small host adapters supply capture
storage, pipe creation, and access to the inherited environment.
This avoids two copies of the ownership machinery.

## Capture storage

Linux retains `memfd_create(MFD_CLOEXEC)`, without a filesystem fallback.
Darwin uses `mkstemp` in the standard library's temporary directory,
immediately unlinks the private file, and sets `FD_CLOEXEC`. Unlinking
precedes every body write and the later fallible flag operation.
The shared layer relocates runtime-private descriptors above 2.

The file supports ordinary fd writes, seeking, and counted reads, so
in-process capture retains its existing byte and restoration contract.
Directory lookup, creation, unlinking, and flag failures are explicit
`SetupError`s; they do not execute the body or select a different backing.
A failed unlink can leave an empty private file, but no captured bytes
have been written to a named file.

Darwin POSIX shared memory is not an alternative for this contract:
XNU's `pshmops` uses `fo_no_read` and `fo_no_write`. Mapping a shared-memory
object does not make its descriptor usable as captured fd 1.
See [Darwin shared-memory operations](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/posix_shm.c).

External-only capture still drains a FIFO into a string on both hosts.
It neither uses capture storage nor depends on a valid temporary directory.
Source newline treatment and background-writer completion remain separate.

## Other platform boundaries

Linux creates pipes atomically with `pipe2(O_CLOEXEC)`. Darwin creates a
pipe, owns both endpoints immediately, then sets their close-on-exec flags.
The selected single-threaded model permits this non-atomic setup; either
flag failure releases both endpoints before the associated child starts.
Darwin obtains the environment through `_NSGetEnviron`; preparation
remains before fork.

Ordinary descriptor close is not retried on either supported host.
Darwin's `close_nocancel` reaches `fp_close_and_unlock`, which releases
the fd before returning the underlying file-close result. This supports
the existing ownership-consumption rule even when close reports an error.
See [Darwin descriptor operations](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_descrip.c).
Guarded descriptors and thread cancellation are not part of this runtime.

The build selects the host adapter and recreates its archive so an old
backend member cannot satisfy symbols instead of the shared implementation.
Unsupported hosts and empty output-directory settings fail explicitly.
The strace catalog and static subreaper remain Linux-only targets.

No native compiler, IR producer, call-context policy, END behavior, or
signal-response decision is changed by this port.
