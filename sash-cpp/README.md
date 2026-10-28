# sash-cpp

An independent C++17 runtime for a future source-shaped sash transpiler.
There is no IR reader or emitter yet. The examples are hand-written:
[greetings.cpp](examples/greetings.cpp) corresponds to
[greetings.sash](examples/greetings.sash), retaining a function, sparse
collection, loop, regex condition, and stdout capture.

[job_report.cpp](examples/job_report.cpp) and
[job_report.sash](examples/job_report.sash) add a bounded while loop,
external commands inside a captured function, a command used as a
condition, and explicitly discarded command results. Two rounds report
three workers and skip a fourth record. Expected bytes live beside each
pair in its `.expected` file.

On Linux:

```sh
cd sash-cpp
mk -p 1 test
./build/greetings
./build/job_report
```

`greetings` prints `Hello, Ada!`; `job_report` prints the worker rounds.
The build produces a normal C++ static library, `build/libsash_cpp.a`;
it does not build Rust or link sash-rt.
For example, the C++ program can also be linked directly:

```sh
c++ -std=c++17 -Iinclude examples/greetings.cpp build/libsash_cpp.a -o build/greetings
```

Use separate output directories for different compilers and flags:

```sh
mk -p 1 CXX=g++ BUILD=build/gcc test
mk -p 1 CXX=clang++ BUILD=build/clang test
mk -p 1 'CXX=g++ -B/usr/bin' BUILD=build/asan \
  'SANITIZE=-fsanitize=address -fno-omit-frame-pointer' test
```

The ASan command selects the system linker as well as GCC on this Linux
development host; mixing its GCC sanitizer libraries with Homebrew's linker
fails. Full UBSan execution has a separate zero-fd fixture/tooling conflict,
recorded in [the sanitizer task](../notes-exp/tasks/sanitizer-host-compatibility.md).

Additional Linux fault probes require `strace` and an ordinary,
non-sanitized build:

```sh
mk -p 1 CXX=g++ BUILD=build/gcc fault-test
```

They exercise failed fork, interrupted wait, partial error-pipe setup,
and exec-error reporting without child writes. The probes check the
observed `clone`/`wait4` syscalls and trace evidence, not just a matching
surface diagnostic. Keep this target separate from sanitizer runs under
ptrace. The command helper is an ordinary unsanitized external executable;
the library, test driver, and examples receive `SANITIZE`.

With the real sash CLI and runtime already built in the parent checkout,
`mk -p 1 sash-example` compiles and runs both sash counterparts against
their expected output. Override `SASH` and `SASH_RT_LIB` for other locations.
This additional target is not a prerequisite for native C++ tests.

For DG's existing native compiler and its companion runtime:

```sh
mk -p 1 SASH="$HOME/src/safebash/_tmp/sash" \
  SASH_RT_LIB="$HOME/src/safebash/sash-rt/target/release/libsash_rt.a" sash-example
```

## Error-path showcase

[error_paths.cpp](examples/error_paths.cpp) demonstrates the current
runtime's failure handling, one operation at a time. The Linux runner
isolates each invocation, displays stdout, stderr, and exit status, and
continues to the next demonstration. It succeeds only when the observed
outcomes match its assertions.

```sh
mk -p 1 error-demo
```

The full catalog has 103 cases: 52 basic cases, 38 injected-fault cases,
and 13 interrupted-operation recovery cases.

| Area | What is demonstrated |
|---|---|
| Command outcomes | Checked, caught, and discarded failures; exit 7; real exit 127 versus failed exec; signal termination |
| Input and library errors | Interior NUL, missing or non-executable commands, ENOEXEC, invalid descriptors, malformed ECMAScript regex, string capacity, allocation exception |
| Capture | Failed inspection, memfd creation, fd relocation/backup, installation, restoration and flags, rewind, read, and close; initially closed stdout |
| Launch and wait | Pipe and shared-status allocation, fork, child pre-exec close, completion reads/protocol, endpoint close, unavailable child status, wait failure |
| Fatal cleanup and diagnostics | Restore, close, reap, and unmap failures during cleanup; stderr failures while reporting the original error |
| Recovery | EINTR at each implemented retry loop, preserving the uninjected output and status |

Failed captures demonstrate that partial output does not replace the
assignment target. Caught and discarded operations continue; checked
failures reach the program boundary instead. Fatal cleanup cannot be
suppressed: it diagnoses the broken ownership/restoration guarantee and
terminates with status 2. This exposes existing runtime policy; it does
not add whole-function suppression.

After `mk -p 1 test`, the basic subset needs neither strace nor the
Linux fault supervisor. Individual cases can also be selected:

```sh
bash examples/error_paths.sh --basic build
bash examples/error_paths.sh --list
bash examples/error_paths.sh --case cleanup-restore build
./build/error_paths exit7 caught ./build/command-helper
```

The full target requires Bash, `strace`, and `timeout`, and must use an
unsanitized build. Raw output and traces stay in `$BUILD/error-demo/`.
Fault locations are calibrated against an uninjected run after an explicit
entry marker, rather than assuming dynamic-loader syscall counts.
The child-close probe uses per-tracee first-close injection, which also
affects the parent's first loader close; both injections are shown, and
the trace must prove the actual fork child failed before exec.
A Linux subreaper supervises traced cases so even the fatal-wait example
does not leave an orphaned fixture behind.

`bad-alloc-injected-*` deliberately throws `std::bad_alloc`; it does not
exhaust the shared host. `completion-protocol-synthetic` injects an
unexpected read count. Arbitrarily corrupted internal shared status and
uncaught programming errors are not forced. Nor does this catalog cover
runtime features that do not exist yet. There is no sash counterpart:
many cases are API misuse or OS faults, not accepted source programs.

## Current boundary

`write` and `echo` check their command outcomes and raise `CommandError`
on failure. `try_write` and `try_echo` return a `CommandResult` without
printing a diagnostic:

```cpp
if (auto result = sash::try_echo({"ready"}); result.success()) {
    // The operation succeeded.
}
(void)sash::try_write("ignored result"); // Explicit discard of one command.
sash::echo({"checked"});                // Uncaught failure aborts through run().
```

`try_execute(path, arguments)` returns a `ProcessResult` with distinct
`ProcessExit`, `ProcessSignal`, and `SetupError` alternatives. Only a
normal exit zero succeeds. `execute` and `check` raise the corresponding
exception; checking a signal result encodes it as `128 + signal`, while
the raw outcome retains the actual signal. Failed exec is not exit 127.
Caught setup failures do not print runtime diagnostics, and a child's own
stderr is still inherited.

Launch uses single-threaded fork/exec with prebuilt argument storage:
no PATH lookup, shell parsing, or ENOEXEC shell fallback. The child does
not allocate or unwind parent objects. Private capture and completion
descriptors are close-on-exec and avoid standard descriptor slots,
including slots initially closed. The foreground caller reaps its child
on ordinary completion and error paths; cleanup does not force-kill or
set a runtime timeout.

Do not implement suppression by catching every exception around a whole
function: that would skip commands after its first failure. Function
checkedness, pipeline aggregation, background launch, and replacing
`exec` are not implemented yet.

`capture_stdout` redirects actual fd 1 to an owned Linux memfd and runs
its void body in-process. Nested frames and exception unwinding restore
the previous destination, including an originally closed stdout and its
descriptor flags. This low-level operation returns exact bytes, including
NUL and trailing newlines; source-level newline treatment remains an open
choice. The examples use newline-free `write` or `printf` inside capture,
so that choice does not affect their equivalence.

The runtime is initially single-threaded and its output helpers use raw
descriptor writes. Captured code must not mix in buffered stdio/iostream
output or retain background writers. Script-fd reservation, arbitrary
redirection, pipelines, END, and signal policy are later work.
Failure to restore descriptor state during unwinding is diagnosed
and terminates rather than continuing on the wrong stdout.

Linux operations are isolated in `src/platform_linux.cpp`; the shared
interface and example have no Linux headers. macOS capture storage is the
next platform choice, not a silent fallback for a failed Linux memfd.

Project [status](../notes-exp/status.md),
[tasks](../notes-exp/tasks/cpp-transpiler-runtime-before-ir.md),
[decisions](../notes-exp/decisions/DD02-foreground-fork-exec.md), and
[journal](../notes-exp/journal/003-error-path-showcase.md) live in
`../notes-exp/`, not the native compiler's `notes/`.
