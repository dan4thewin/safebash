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

[capture_report.cpp](examples/capture_report.cpp) and
[capture_report.sash](examples/capture_report.sash) capture a five-stage
external pipeline in a real summary function, nest it inside a framed
function capture, and retain two rounds of summaries for later traversal.
The sash failure probe saves status with `||`, attests once, then branches;
the C++ probe keeps captured bytes separate from pipeline outcomes.
Both produce the adjacent eight-line expected report.

[lifecycle_report.cpp](examples/lifecycle_report.cpp) and
[lifecycle_report.sash](examples/lifecycle_report.sash) retain two workers
from a captured function, discard a separate function's returned int,
and explicitly exit into END. END catches one failed command, discards
another, and traverses the retained collection. All source functions are
defined outside END; both reports match the adjacent eight-line golden.

[option_report.cpp](examples/option_report.cpp) and
[option_report.sash](examples/option_report.sash) add sparse cell operations,
forwarded and aliased collection parameters, population length, joins,
pairwise array-to-map conversion, independent key snapshots, and argument
construction. The report retains empty arguments while explicitly eliding
others; its real argument-reporting function invokes external printf.
Both match the adjacent seventeen-line golden.

On Linux or macOS, with a C++17 compiler, Plan 9 `mk`, and `timeout` on PATH:

```sh
cd sash-cpp
mk -p 1 test
./build/greetings
./build/job_report
./build/stream_report
./build/pipeline_report
./build/capture_report
./build/lifecycle_report
./build/option_report
```

`greetings` prints `Hello, Ada!`; `job_report` prints the worker rounds;
`stream_report` prints streamed records and the retained worker list;
`pipeline_report` prints sorted, deduplicated records and retained state;
`capture_report` prints comma-joined summaries and their captured frames.
`lifecycle_report` prints the captured count and explicitly handled END report.
`option_report` prints the sparse options, copied keys, and exact argument list.
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

On macOS, the real Apple Clang/libc++ build and its ASan variant use:

```sh
mk -p 1 CXX=clang++ BUILD=build/apple test
mk -p 1 CXX=clang++ BUILD=build/apple-asan \
  'SANITIZE=-fsanitize=address -fno-omit-frame-pointer' test
./build/apple/capture_report
```

Both hosts run the same 41 fixture groups and seven expected C++ reports,
including large binary output, nested captures/readers, closed standard
descriptors, partial setup, child/fd cleanup, typed exit, main-only END,
and signal delivery during foreground work and capture. Collection fixtures
exercise live insertion/deletion through aliases, copied loop values, lazy
dispositions, binary joins, and allocation failures before argument publication.
The Darwin build has been
exercised on Apple Silicon; this is not a claim of testing every Mac SDK
or architecture. Tests need GNU `timeout`, which is not a stock macOS tool.
The Linux fault/catalog targets below explicitly reject Darwin.

Additional Linux fault probes require `strace` and an ordinary,
non-sanitized build:

```sh
mk -p 1 CXX=g++ BUILD=build/gcc fault-test
```

They exercise failed first/later forks, interrupted wait, recovered wait
errors with retained child outcomes, partial error-pipe setup,
exec-error reporting without child writes, and raw capture read/cleanup.
Six lifecycle checks inject signal installation in the body or END,
mask setup within END, fatal disposition restoration, and lifecycle
entry/restoration handoff failures with allocation denied. END catches
its setup faults explicitly; failed restoration remains a broken-runtime
guarantee, not a suppressed source command.
Capture probes calibrate the first and second positive data reads by fd,
verify retained bytes after a wait error, and prove independence from memfd
with an injection canary followed by an absence check. The probes check the
observed `clone`/`wait4` syscalls and trace evidence, not just a matching
surface diagnostic. Keep this target separate from sanitizer runs under
ptrace. The command helper is an ordinary unsanitized external executable;
the library, test driver, and examples receive `SANITIZE`.

With the real sash CLI and runtime already built in the parent checkout,
`mk -p 1 sash-example` compiles and runs all seven sash examples against
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

The full catalog has 169 cases: 85 basic cases, 65 injected-fault cases,
and 19 interrupted-operation recovery cases.
The newer lifecycle fixtures and their six injected-fault checks belong
to `test` and `fault-test`, not this command/capture catalog.

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
| Raw pipeline capture | Exact binary bytes, failed producer output retained with outcomes, unchanged checked targets, pipe allocation/read/wait errors, and first/second read recovery |
| Launch and wait | Pipe and shared-status allocation, fork, child pre-exec close, completion reads/protocol, endpoint close, unavailable child status, wait failure |
| Fatal cleanup and diagnostics | Restore, close, reap, and unmap failures during cleanup; stderr failures while reporting the original error |
| Recovery | EINTR at each implemented retry loop, preserving the uninjected output and status |

Checked captures demonstrate that partial output does not replace the
assignment target; inspectable pipeline capture instead retains the bytes
with failed producer outcomes. Caught and discarded operations continue; checked
failures reach the program boundary instead. Fatal cleanup cannot be
suppressed: it diagnoses the broken ownership/restoration guarantee and
terminates with status 2. This exposes existing runtime policy; it does
not add source-level function-result handling.

On Linux, after `mk -p 1 test`, the basic subset needs neither strace nor the
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
exhaust the shared host. The separate runtime fixture fails a real collector
allocation using a test-executable-only allocation override, then checks
descriptor and child cleanup. It applies no host-wide memory limit.
`completion-protocol-synthetic` injects an
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

Discard a function's returned int/status at the call site; do not catch its
body failures merely because its answer is ignored. Function bodies and
argument evaluation keep their own checks, including under conditions
(DG's 2026-10-29/30 native DD#55 ruling). No inherited unchecked mode is
needed for this. Per-stage result suppression, background launch, and
replacing `exec` are not implemented yet.

### Collections and owned arguments

`Array<T>` is an alias for `std::map<std::int64_t, T>`; `Map<T>` is
`std::map<std::string, T>`. Source element types use `std::string` or
`std::int64_t`. These remain ordinary containers and reference parameters,
not handles into a runtime variable table. Whole-container assignment copies;
passing a reference, including two aliases or forwarded references, does not.

Use `cell(collection, key)` for non-inserting reads, `set_cell` for
assignment, `unset` for cell deletion, and ordinary `.clear()` for clearing.
`cell` returns an owned value: `""` for a missing string or `0` for a
missing integer. `len` counts present cells, including stored empty/zero
values, not the highest array index plus one.
Negative array indices raise `SetupError`; string map keys such as `"-1"`
remain valid. These helpers enforce source cell rules; direct C++ container
access is not a substitute for them in emitted code.

`values(collection)` and map-only `keys(map)` are live sorted ranges:

```cpp
for (const auto &value : sash::values(items)) {
    sash::checkpoint();
    sash::echo({value});
}
```

They borrow an lvalue container, copy the current key, and advance with
`upper_bound` after the body. Values and key loop variables are copies too,
so erasing the current cell cannot leave a dangling loop variable.
Later insertions are visited, earlier/current keys are not revisited,
deleted pending cells are skipped, and pending value updates are observed.
Continually inserting later keys can extend the walk indefinitely.
Do not replace this with a plain map range-for when the body can erase
the current cell. Temporary containers are rejected by the live-range API.
Collection helpers do not insert lifecycle checkpoints.

`snapshot_keys(map)` returns a separate dense string array, unchanged by
later map mutations. `join(collection, separator)` joins sorted values,
rendering integers and preserving counted binary bytes. Its default
separator is a space; an explicitly empty separator stays empty, and there
is no fixed-size result buffer.

`map_pairs_elided(array)`, `map_pairs_or(array, key_callback)`, and
`map_pairs_required(array, message_callback)` consume array values as
alternating keys and values, retaining native DD#99's explicit odd-tail
dispositions. They respectively drop an odd tail, use it as the value of
a fallback key, or raise a requirement failure. An empty array is a valid
empty map; callbacks run only for an odd count. Integer keys stringify,
values keep their type, and later duplicate keys overwrite earlier ones.
This is not an inverse of map value traversal.

`default_value(value, fallback)` and `required_value(value, message)` use
lazy callbacks, shared by scalars and cell reads. Their subject is the typed
value, not membership: `""` or integer `0` triggers the operator, whereas
string `"0"` does not. Requirements raise `CommandError` with status 2.
Key-evaluation, allocation, and conversion failures are not missing values.

Argument construction uses an ordinary owned `std::vector<std::string>`:

| Helper | Contribution |
|---|---|
| `append_argument(args, value)` | One argument, including an empty string or integer zero rendered as `"0"` |
| `append_elided(args, value)` | Zero arguments for `""` or integer `0`; otherwise one |
| `append_splat(args, collection)` | Sorted present values, including empty/zero cells; zero for an empty collection |
| `append_splat_or(args, collection, fallback)` | Present values, or exactly one lazy fallback argument, even if empty |
| `append_splat_required(args, collection, message)` | Present values, or a lazy requirement failure on an empty collection |

Splat preparation publishes no partial extension on allocation failure.
Already-built arguments are independent of later source mutation; scalar
appends are also safe when their source aliases the destination vector.
Binary bytes are retained internally, but the existing exec boundary
rejects NUL in executable/argument strings before any launch.
There is no word splitting or inherited unchecked function mode.
These helpers do not implement argv/environment ingress, numeric parsing,
or the remaining string/read operation families.

### Program lifecycle and END

`run(program, body)` keeps the original void-body entry form.
`run(program, body, end)` additionally invokes a void END handler with
`const Shutdown &`: its `reason` is `Completed`, `ExplicitExit`, `Failure`,
or `Signal`; `status` is the incoming status, and `signal` retains the
triggering signal separately. Normal void-body completion is zero.

`exit_program(status)` raises a dedicated `ExitRequest`, not a command
verdict or a function return. Successful body unwinding restores captures
and closes pipeline endpoints before reaping, then END runs once on the
program channels. Returning from END preserves the incoming status;
an explicit `exit_program` in END overrides it without re-entry.
The callbacks are borrowed rather than copied into allocating closures.

END follows DG's [planned contract](../notes-exp/decisions/DD05-planned-end-explicit-handling.md):
every fallible operation is explicitly caught or suppressed. The driver
inserts no automatic catch-and-continue, diagnostic, or abort handling
around END operations. Ordinary function bodies and argument evaluation
remain checked, even when called from END. The example also explicitly
catches setup/allocation/capacity failures and chooses exit 2 for them.

An unhandled fallible END operation is invalid source under that planned
contract. The hand-written C++ API lets its exception escape to the C++
caller; it does not invent a successful result or automatic END policy.
Enforcing no source function definitions in END and mandatory handling
belongs to the future front end, not a temporary C++ parser. The named
generated END handler itself is not a source function definition in END.
Neither native compiler policy nor producer output changed here.

The lifecycle checks the owning PID before END. External fork children
still exit without unwinding inherited C++ objects, and explicit program
exit in a forked child uses `_exit`, not parent END. This does not add
forked C++ stages or solve their future inherited-reader ownership.
Broken descriptor, reap, signal-restoration, or lifecycle-handoff guarantees
retain the existing nonallocating fatal-cleanup policy.

### Signals and cooperative checkpoints

Within `run`, the owner calls `signal(number, SignalDisposition::Ignore)`,
`Default`, `Flag`, or `Exit`. Registration failures are `SetupError`;
allocation may also fail before the kernel disposition changes.
Original dispositions and the incoming mask are restored when the driver
leaves, including after an escaping END exception.

The two signal handlers perform only lock-free integer stores.
`signum()` reads the most recently delivered handled signal;
`clear_signum()` clears that information, not a pending EXIT request.
FLAG cannot become EXIT merely because another signal declares EXIT or
the same signal is later rearmed. `Shutdown::signal` retains the pending
EXIT identity even if a later FLAG changes `signum()`.

`checkpoint()` raises a signal exit request in ordinary execution context.
Hand-written code places it before/after external commands and pipelines,
and at function/loop boundaries. The command, reader, and capture APIs
do not secretly insert it; future generated placement belongs to the
consumer. `run` checks body entry and normal completion too.
Checkpoints while END is active prevent lifecycle re-entry; they do not
disable command checks or change called function bodies.

Blocking reads and waits retry EINTR. Signal recording does not interrupt
ownership cleanup or automatically kill children: shutdown waits for the
next checkpoint, which can be delayed by foreground work or pipe EOF.
The fixtures use finite helpers and a two-way pipeline handshake.
Linux traces confirm interrupted reads/waits before ordinary-context END;
the same behavior fixtures run on the real Mac using host signal constants.
There is still no runtime timeout or force-kill policy.

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

### Raw external-pipeline capture

The fifth pair uses the checked byte collector:

```cpp
const auto text = sash::capture_pipeline(
    {{"/usr/bin/printf", {"Ada\nGrace\n"}}, {"/bin/cat", {}}});
```

`try_capture_pipeline` instead returns `PipelineCapture`, with a counted
`output` string and a separate `PipelineResult` in `pipeline`. Failed
producer exits and completion errors do not erase already collected bytes.
The checked wrapper checks the pipeline before returning the string, so a
failed right-hand capture does not replace an existing assignment target.

The parent drains the terminal pipe concurrently into the string, preserving
NUL, CR, separators, and every final newline. It does not use capture storage or
rebind parent stdin/stdout; it shares preparation, launch, endpoint ownership,
and completion with the line-reader pipeline. The terminal command observes
a FIFO on stdout. Raw capture can run inside an in-process capture body or a parent-reader
loop without adopting their channels.

Command validation and setup failure before consumption leave output empty
and retain unstarted stages. An empty command list throws `SetupError`.
Consumer read errors, allocation failures, and string-capacity errors also
escape the inspectable form: they are not EOF or invented child outcomes,
and no partial `PipelineCapture` is returned. Successful cleanup closes all
parent endpoints before reaping; a cleanup failure retains the existing
fatal policy.

Completion means pipe EOF followed by accounting for every owned stage.
A descendant retaining the output pipe can delay EOF; there is no runtime
timeout or automatic force-kill. Background-writer language semantics remain
separate work. Binary capture does not permit NUL in OS argument strings.

### In-process capture and remaining boundaries

`capture_stdout` redirects actual fd 1 to owned seekable storage and runs
its void body in-process. Nested frames and exception unwinding restore
the previous destination, including an originally closed stdout and its
descriptor flags. This low-level operation returns exact bytes, including
NUL and trailing newlines; source-level newline treatment remains an open
choice. The examples use newline-free `write` or `printf` inside capture,
so that choice does not affect their equivalence.

Linux uses memfd without a filesystem fallback. Darwin uses a private
mkstemp file, unlinked before the body writes and marked close-on-exec.
Temporary-directory, creation, unlink, and flag failures are explicit
setup errors; a failed setup never executes the body. Raw external capture
still uses a pipe and succeeds even if TMPDIR is invalid.
Darwin shared-memory fds cannot provide ordinary read/write operations.
See [DD#4](../notes-exp/decisions/DD04-macos-posix-adapter.md).

The runtime is initially single-threaded and its output helpers use raw
descriptor writes. Captured code must not mix in buffered stdio/iostream
output or retain background writers. Script-fd reservation, arbitrary
redirection and further pipeline forms remain later work.
Failure to restore descriptor state during unwinding is diagnosed
and terminates rather than continuing on the wrong stdout.

Shared POSIX operations and ownership live in `src/platform_posix.cpp`.
`src/lifecycle.cpp` owns the shared POSIX signal and program boundary.
`src/platform_linux.cpp` supplies memfd, pipe2, and environment access;
`src/platform_darwin.cpp` supplies unlinked capture files, pipe/CLOEXEC,
and CRT environment access. Public interfaces and examples have no
Linux-only headers. The build chooses the host adapter and recreates its
archive so a stale backend member cannot remain active.

Project [status](../notes-exp/status.md),
[tasks](../notes-exp/tasks/cpp-transpiler-runtime-before-ir.md),
[decisions](../notes-exp/decisions/DD03-external-pipeline-outcomes.md), and
[journal](../notes-exp/journal/011-lifecycle-runtime.md) live in
`../notes-exp/`, not the native compiler's `notes/`.
