## Foreground commands and the second program pair

`sash-cpp/examples/job_report.cpp` is the hand-written counterpart of
`job_report.sash`. It adds external commands to a real captured function,
sparse records, two rounds of ordinary while/for control flow, a regex
filter, a command used as a condition, and explicitly discarded failures.

```sh
cd sash-cpp
mk -p 1 test
./build/job_report
```

Both job-report executables produce the independently specified eleven
lines; the first greetings pair still produces `Hello, Ada!`. The native
reference uses DG's `~/src/safebash/_tmp/sash` with the runtime archive
from the same checkout. No compiler, producer, or native dependencies
were changed.

DG selected single-threaded fork/exec over a hybrid launch model.
Foreground outcomes distinguish normal exit, signal termination, and
setup failure; checked calls raise, while caught/discarded failures stay
inspectable and quiet. Private descriptors avoid closed standard slots,
and the caller owns reaping. Exec errno is published through a preallocated
shared cell, avoiding a failed reporting write becoming apparent exit 127.
See [DD#2](decisions/DD02-foreground-fork-exec.md) and
[journal 002](journal/002-job-report-and-foreground-processes.md).

GCC and Clang pass fourteen fixture groups and both examples.
Host-GCC AddressSanitizer passes too. Linux strace probes cover fork
failure, interrupted wait, partial error-pipe setup, and write-independent
exec-error reporting. The previous
[sanitizer tooling limitations](tasks/sanitizer-host-compatibility.md) remain
documented; the separate native
[rustix issue](../notes/tasks/bug-fresh-clone-rustix-runtime-private.md)
is unchanged and does not block reference comparison.

The [runtime task](tasks/cpp-transpiler-runtime-before-ir.md) stays open.
The first Linux launch/capture slice is complete, making the macOS storage
choice timely. Remaining work includes call-context propagation,
background/public wait, replacing exec, forked C++ stages, general
redirection, streaming pipelines/readers, END/signals, and most value
operations.

Work remains on `cpp-runtime-first-slice`. Experiment records stay here,
not under the native compiler's `notes/`. The independent producer plan is
still the local, uncommitted draft
`tasks/emit-ir-json-for-c-transpiler.md`, as requested. No IR reader or
emitter has been implemented.
