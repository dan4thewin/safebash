## Third pair: a streaming worker report

[`stream_report.cpp`](../sash-cpp/examples/stream_report.cpp) and its
[sash counterpart](../sash-cpp/examples/stream_report.sash) stream four
records from an external producer through a parent-side `while readline`.
A real function fills its caller's sparse collection by reference,
captures worker labels inside the loop, and leaves entries 1, 2, and 4
available afterward. Both produce the independent nine-line report.

The runtime adds checked `read_from`, inspectable `try_read_from`, and
an explicit `LineReader`. It reuses foreground launch rather than forking
a second implementation. Only child stdout is piped; commands inside
the reader still inherit program stdin. Cleanup closes the pipe before
waiting, including partial launch failure, early return, and unwinding.
Consumer exceptions do not become producer results.

GCC, Clang, and host-GCC ASan pass nineteen fixture groups and all three
C++ example goldens. DG's supplied native compiler passes all three sash
goldens. Streaming is pinned by a two-way handshake rather than a sleep;
nested readers/captures and early SIGPIPE are exercised too.
See [journal 004](journal/004-streaming-worker-report.md).

## Runnable error-path showcase

`sash-cpp/examples/error_paths.cpp` and its shell runner now demonstrate
112 isolated cases: 53 basic cases, 45 injected faults, and 14 interrupted
operation recoveries. Each displays stdout, stderr, and status.

```sh
cd sash-cpp
mk -p 1 error-demo
```

Checked errors stop their operation's program; caught/discarded errors
continue. A real exit 127 stays distinct from failed exec, signal outcomes
retain their identity, and a failed capture leaves its target unchanged.
The catalog includes descriptor/capture setup, reads/writes, process
launch and wait, failed diagnostics, and fatal restoration/reap/close/unmap.
A Linux subreaper supervises traced examples so the fatal-wait case leaves
no orphaned fixture.

GCC and Clang pass the full showcase. The nine new cases cover streaming
setup, reads, completion, child stdout installation, and EINTR. Five
new negative controls reject runs with injection removed, alongside the
original ordinary/fatal controls.
The full showcase deliberately runs without sanitizers. Allocation
failure is an explicitly thrown exception, not host memory exhaustion;
arbitrary internal corruption and future features are not claimed.
See [journal 003](journal/003-error-path-showcase.md) and
the [runnable catalog](../sash-cpp/README.md#error-path-showcase).

The [low-level tour](../sash-cpp/docs/error-path-tour.md) now follows
representative paths through both harnesses for C programmers new to C++
and strace injection. It covers result versus exception propagation,
capture unwinding, fault-site calibration, retries, and fatal cleanup.

No compiler, producer, or native dependency changed.
The first greetings and second job-report pairs remain available; their
native comparison and DG's single-threaded fork/exec choice are recorded
in [journal 002](journal/002-job-report-and-foreground-processes.md) and
[DD#2](decisions/DD02-foreground-fork-exec.md).

The [runtime task](tasks/cpp-transpiler-runtime-before-ir.md) stays open.
The first Linux launch/capture slice is complete, making the macOS storage
choice timely. Remaining work includes call-context propagation,
background jobs, replacing exec, forked C++ stages, general redirection,
multi-stage pipelines, reader exits and public wait integration,
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
