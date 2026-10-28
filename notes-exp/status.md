## Runnable error-path showcase

`sash-cpp/examples/error_paths.cpp` and its shell runner now demonstrate
103 isolated cases: 52 basic cases, 38 injected faults, and 13 interrupted
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

GCC and Clang pass the full showcase, fourteen existing fixture groups,
and the unchanged greetings/job-report goldens. Host-GCC ASan passes the
existing gates and three selected exception/capture demonstrations.
Ordinary/fatal negative controls reject runs with injection removed.
The full showcase deliberately runs without sanitizers. Allocation
failure is an explicitly thrown exception, not host memory exhaustion;
arbitrary internal corruption and future features are not claimed.
See [journal 003](journal/003-error-path-showcase.md) and
the [runnable catalog](../sash-cpp/README.md#error-path-showcase).

No runtime behavior, compiler, producer, or native dependency changed.
The first greetings and second job-report pairs remain available; their
native comparison and DG's single-threaded fork/exec choice are recorded
in [journal 002](journal/002-job-report-and-foreground-processes.md) and
[DD#2](decisions/DD02-foreground-fork-exec.md).

The [runtime task](tasks/cpp-transpiler-runtime-before-ir.md) stays open.
The first Linux launch/capture slice is complete, making the macOS storage
choice timely. Remaining work includes call-context propagation,
background/public wait, replacing exec, forked C++ stages, general
redirection, streaming pipelines/readers, END/signals, and most value
operations.

Existing [sanitizer tooling limitations](tasks/sanitizer-host-compatibility.md)
and the separate native
[rustix issue](../notes/tasks/bug-fresh-clone-rustix-runtime-private.md)
are unchanged.

Work remains on `cpp-runtime-first-slice`. Experiment records stay here,
not under the native compiler's `notes/`. The independent producer plan is
still the local, uncommitted draft
`tasks/emit-ir-json-for-c-transpiler.md`, as requested. No IR reader or
emitter has been implemented.
