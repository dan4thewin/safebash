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

[stream_report.cpp](examples/stream_report.cpp) and
[stream_report.sash](examples/stream_report.sash) add an external producer
feeding a parent-side `while readline`. A function fills its caller's
sparse collection by reference, captures labels inside the loop, and
leaves the saved workers available afterward. Four records produce three
workers at indices 1, 2, and 4 and a nine-line report.

[pipeline_report.cpp](examples/pipeline_report.cpp) and
[pipeline_report.sash](examples/pipeline_report.sash) put three external
stages before that reader: printf, C-locale sort, and uniq. The function
retains three workers from four unique records; a later condition detects
an upstream failure even when the last command succeeds. Both produce
the adjacent ten-line expected report.

On Linux:

```sh
cd sash-cpp
mk -p 1 test
./build/greetings
./build/job_report
./build/stream_report
./build/pipeline_report
```

`greetings` prints `Hello, Ada!`; `job_report` prints the worker rounds;
`stream_report` prints streamed records and the retained worker list;
`pipeline_report` prints sorted, deduplicated records and retained state.
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

They exercise failed first/later forks, interrupted wait, recovered wait
errors with retained child outcomes, partial error-pipe setup, and
exec-error reporting without child writes. The probes check the
observed `clone`/`wait4` syscalls and trace evidence, not just a matching
surface diagnostic. Keep this target separate from sanitizer runs under
ptrace. The command helper is an ordinary unsanitized external executable;
the library, test driver, and examples receive `SANITIZE`.

With the real sash CLI and runtime already built in the parent checkout,
`mk -p 1 sash-example` compiles and runs all four sash counterparts against
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

The full catalog has 151 cases: 73 basic cases, 61 injected-fault cases,
and 17 interrupted-operation recovery cases.

The [low-level tour](docs/error-path-tour.md) follows representative cases
through the Bash runner, strace fault injection, C++ outcomes, and resource
cleanup. It assumes C and Unix experience rather than C++ familiarity.

| Area | What is demonstrated |
|---|---|
| Command outcomes | Checked, caught, and discarded failures; exit 7; real exit 127 versus failed exec; signal termination |
| Input and library errors | Interior NUL, missing or non-executable commands, ENOEXEC, invalid descriptors, malformed ECMAScript regex, string capacity, allocation exception |
| Capture | Failed inspection, memfd creation, fd relocation/backup, installation, restoration and flags, rewind, read, and close; initially closed stdout |
| Streaming input | Output/completion pipe allocation, partial launch cleanup, child stdout installation, reader errors/close, and interrupted reads |
| External pipelines | Every stage's exit/signal, rightmost checked failure, unstarted stages, late setup failures, stdin wiring, endpoint ownership, and retained outcomes after parent errors |
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
Higher child-close ordinals use the static supervisor's controlled close
prelude so an earlier parent loader failure cannot prevent the intended
child from being created. The full catalog requires static development
archives for this small Linux helper; ordinary examples remain normal
C++ programs and do not require static linking.
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
checkedness, stage/function suppression, background launch, and replacing
`exec` are not implemented yet.

### Command-backed line readers

The third pair uses the checked form:

```cpp
sash::read_from("/usr/bin/printf", {"Ada\nGrace\n"}, [&](sash::LineReader &input) {
    std::string person;
    while (input.readline(person)) {
        sash::echo({person});
    }
});
```

The callback runs in the parent, so mutations to captured variables and
reference parameters remain visible. Only the producer child's stdout
is connected to the pipe. Parent stdin and stdout are not rebound: an
ordinary command in the loop still inherits program stdin, and captures
or nested readers have their own destinations and ownership.

`readline` removes one newline delimiter, retaining all other bytes,
including NUL and carriage return. An empty terminated line succeeds;
an unterminated final nonempty record succeeds once. EOF clears the
target and returns false. A read error raises `SetupError`, not EOF,
and leaves the target unchanged. These are the DD#99/DD#122 reference
rules, not whitespace splitting or a new interpretation of `read`.

`try_read_from` returns the producer's `ProcessResult`; `read_from`
checks it. Setup failure before the consumer starts does not invoke it.
Consumer exceptions propagate as themselves, not as producer outcomes.
Already-consumed records and parent mutations are not rolled back if
the producer subsequently fails.

Completion closes the reader before waiting, including when a callback
returns early or throws. Early closure can give the producer SIGPIPE;
the unchecked form retains that signal outcome. During unwinding the
consumer's original exception survives successful cleanup. There is no
force-kill policy or runtime timeout: waiting can still block if a
producer ignores the closed pipe and keeps running.

The `LineReader` is borrowed only for the callback's lifetime; a `return`
in the callback ends that region, not its surrounding C++ function.
The same explicit reader is also used by the multi-stage API below.

### External pipelines

The checked form takes an ordered vector of commands:

```cpp
sash::read_pipeline(
    {{"/usr/bin/printf", {"Grace\nAda\nGrace\n"}},
     {"/usr/bin/env", {"LC_ALL=C", "/usr/bin/sort"}},
     {"/usr/bin/uniq", {}}},
    [&](sash::LineReader &input) {
        std::string person;
        while (input.readline(person)) {
            sash::echo({person});
        }
    });
```

`try_read_pipeline` instead returns a `PipelineResult`. Its `stages` vector
has one optional `ProcessResult` per command; empty slots mean unstarted
stages, never invented exit zero. Observed pre-exec failures stay setup
outcomes rather than ordinary exit 127. `setup_errors` separately records
setup/cleanup errors with zero-based stage indices, in encounter order,
without overwriting known child exits or signals.

An empty command list is an invalid API request and raises `SetupError`,
including in the inspectable form. Allocation failures and consumer
exceptions may also escape rather than becoming stage outcomes.

Success requires every stage to succeed and no setup error. Checking first
raises the earliest setup error, otherwise the rightmost failed stage's
exit/signal. All started children are accounted for before ordinary
verdicts are checked; consumed parent state is not rolled back.
[DD#3](../notes-exp/decisions/DD03-external-pipeline-outcomes.md) records
DG's choice.

Command strings are checked before any fork. All data pipes are allocated
before launch, and each later child's stdin comes from the preceding pipe.
Parent stdin is never rebound. Setup failure skips the consumer, closes all
remaining parent endpoints before waiting, and leaves unstarted stages
explicit. Each endpoint is attempted even if another close fails.
Consumer exceptions propagate unchanged after successful cleanup.

This slice is external stages plus a terminal parent reader, not forked
C++ stages, public wait, stage suppression, negation, or general redirects.

### Capture and remaining boundaries

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
redirection, further pipeline forms, END, and signal policy are later work.
Failure to restore descriptor state during unwinding is diagnosed
and terminates rather than continuing on the wrong stdout.

Linux operations are isolated in `src/platform_linux.cpp`; the shared
interface and example have no Linux headers. macOS capture storage is the
next platform choice, not a silent fallback for a failed Linux memfd.

Project [status](../notes-exp/status.md),
[tasks](../notes-exp/tasks/cpp-transpiler-runtime-before-ir.md),
[decisions](../notes-exp/decisions/DD03-external-pipeline-outcomes.md), and
[journal](../notes-exp/journal/005-external-pipeline-report.md) live in
`../notes-exp/`, not the native compiler's `notes/`.
