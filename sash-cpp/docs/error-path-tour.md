# Error paths: a low-level tour from Bash through C++

This is a reading guide to the error showcase for programmers comfortable
with C, Unix processes, and ordinary strace output, but not necessarily
with C++ or syscall fault injection. It follows representative cases
through both harnesses and the runtime, rather than cataloguing all 103.

The C++ program directly exercises the runtime that future transpiled
programs will use; it is hand-written, not generated. The Bash program
launches it, supplies faults, and checks its observable behavior. Neither
is the sash compiler or the native Rust runtime.

## Get oriented

Use Linux and an unsanitized build. Starting at the repository root:

```sh
cd sash-cpp
mk -p 1 error-demo
bash examples/error_paths.sh --list
```

All remaining commands run from `sash-cpp/`.
The build tool is `mk`, not `make`. The `error-demo` target builds the
library and examples, prepares the command fixtures, builds the Linux
supervisor, then runs the Bash catalog. To build just the prerequisites
for basic cases, without requiring strace:

```sh
mk -p 1 fixtures
bash examples/error_paths.sh --basic --case exit7-caught build
```

Use the full target before the injected examples in this tour: `fixtures`
alone does not build the supervisor. The final `build` argument to the
shell runner selects the output directory; change it for another `BUILD`.
Each invocation has an outer ten-second timeout. That is harness
containment, not a timeout imposed by the runtime.

| File | Responsibility and useful entry points |
|---|---|
| [examples/error_paths.sh](../examples/error_paths.sh) | Catalog and assertions: `basic`, `fault`, `invoke`, `ordinal`, `child_close` |
| [examples/error_paths.cpp](../examples/error_paths.cpp) | Scenario dispatch and handling: `main`, `scenario`, `Demo::process`, `Demo::operation` |
| [include/sash/runtime.hpp](../include/sash/runtime.hpp) | Public result/error types; the `run` program boundary |
| [src/runtime.cpp](../src/runtime.cpp) | `try_execute`, `execute`, `check`, and the capture callback wrapper |
| [src/platform.hpp](../src/platform.hpp) | Descriptor and capture owners, including member declaration order |
| [src/platform_linux.cpp](../src/platform_linux.cpp) | Syscalls: `Child`, `SharedChildFailure`, `StdoutCapture`, `write_all`, `cleanup_failed` |
| [tests/command_helper.cpp](../tests/command_helper.cpp) | Controlled exits, signals, and the ignored-SIGCHLD wrapper |
| [tests/error_supervisor.cpp](../tests/error_supervisor.cpp) | Test-only subreaper that contains orphaned descendants |

Use those responsibilities and function names as locators; line numbers
will move. Trace excerpts below are condensed from recorded runs.
Addresses and some arguments are omitted. Descriptor numbers and syscall
ordinals are observations, not constants to embed in another probe.

## The C++ needed to read the code

There is no interpreter or general object framework hidden behind the
syntax. Most of the unfamiliar parts provide counted storage, tagged
outcomes, callbacks, and cleanup.

| Spelling | C-oriented reading |
|---|---|
| `namespace rt = sash` | A shorter name for the same namespace. `rt::write` names `sash::write`, not a function pointer in an object. |
| `auto value = expression` | The compiler infers a fixed type. It is not a dynamically typed variable. |
| `const T &value` | A borrowed, read-only reference, roughly like a non-null `const T *` used without explicit dereferencing. No `T` is copied. |
| `std::string` | An owning, length-counted byte string. Embedded NUL is possible; it is not just a `char *` measured with `strlen`. |
| `std::string_view` | A borrowed pointer/length view. It neither owns nor extends the lifetime of the bytes. |
| `std::vector<T>` | An owning growable array, with its allocation released automatically. |
| `std::variant<A, B, C>` | A managed tagged union. `std::get_if<A>(&v)` returns a pointer to the `A` payload, or null if another alternative is selected. |
| `[&] { statements; }` | A callable closure borrowing surrounding locals by reference: think callback plus context structure. `[]` captures no surrounding locals. |
| `template <typename Body>` | Compile this helper for the supplied callable type. It does not interpret the body at runtime. |
| `[[nodiscard]]` | Warn if an outcome is ignored accidentally. An explicit `(void)` marks intentional discard. |

For example, `main` has this shape, with setup omitted:

```cpp
return rt::run("error_paths", [&] {
    scenario(demo, name);
    demo.say("continued");
});
```

The closure runs synchronously. Likewise, capture callbacks finish before
their enclosing locals disappear; their borrowed references do not escape.
The capture implementation accepts a `std::function<void()>`, a
type-erased callable value, conceptually a callback/context pair. The
public templates require a void body: an integer returned by a callback
is not silently interpreted as a command status.

### Destruction is cleanup, not garbage collection

A `throw` transfers control toward the nearest matching `catch`, rather
than returning an error value. `catch (const CommandError &error)` borrows
the thrown error object for the handler. Scope cleanup happens on the way
to that handler, not because the handler explicitly frees each resource.

A local object's destructor runs when its scope ends, both on ordinary
return and during exception unwinding. This is RAII: put acquisition and
release into an object's lifetime rather than repeat a C `goto cleanup`
chain at every return.

`FileDescriptor` owns an integer fd. Copying it is forbidden, preventing
two ordinary copies from both owning the same descriptor. Moving it
transfers ownership and sets the old owner's fd to -1. `std::move` enables
that move operation; it does not call `dup` or create another kernel
descriptor.

Unwinding differs importantly from C `longjmp`: fully constructed local
objects in the exited C++ scopes are destroyed, in reverse construction
order. A constructor that fails does not get its own class destructor,
but its already constructed members are destroyed.

`noexcept` means an exception must not escape the function; a violation
invokes `std::terminate`. It does **not** mean its syscalls cannot fail
or that errors are converted automatically. The `noexcept` cleanup paths
here either return a `std::error_code` or diagnose a fatal cleanup failure
and call `std::_Exit(2)`. They do not throw a second exception out of a
destructor.

`std::error_code` is an error value, not an exception. Its boolean
conversion is true for an error, so `if (const auto error = restore())`
means "if restoration failed."

## The shell harness keeps three statuses separate

There can be a helper process, a C++ demonstration process, and the Bash
runner's result. These are not interchangeable:

| Case | Helper outcome | C++ demo status | Bash runner status |
|---|---|---|---|
| `exit7-caught` | Exit 7 | 0, after reporting the caught result | 0 |
| `exit7-checked` | Exit 7 | 7, after reporting at the program boundary | 0 |
| `fork` | No helper was created | 2 | 0 |
| `fork --without-injection` | Exit 0 | 0 | 1: the required fault never happened |

A successful showcase deliberately contains many failed C++ invocations.
The runner's success means their outcomes matched its assertions.

The catalog's names are also distinct from C++ scenario names.
`exit7-caught` selects scenario `exit7` and handling `caught`; the shell
passes those as separate arguments. `fork` selects the otherwise healthy
`process` scenario in checked mode, adding an injected syscall failure.

### `invoke`: argv construction and process containment

`invoke` starts with a Bash array:

```bash
command=("$program" "$scenario" "$handling" "$helper")
```

For traced cases it wraps that argv with strace options, then with the
supervisor. This is argument-vector construction, not `eval` or a shell
command-string interpreter. The ordinary traced process tree is:

```text
error_paths.sh
  timeout
    error-supervisor
      strace
        error_paths
          command-helper
```

The C++ demo's stdout and stderr go to files through the shell's
redirections. Contexts such as `stdout-closed` then close fd 1 inside the
invocation subshell. The corresponding C++ scenarios report through fd 2,
so closing stdout does not also destroy the observation channel.

`invoke` runs the command in an `if` condition. Thus `set -e` does not
abort the runner on an expected nonzero demo status; the `else` arm
immediately saves `$?` as `observed`. A timeout or an unexpected status
still fails the case's explicit assertions.

For a selected case `NAME`, evidence is under `build/error-demo/`:

| Suffix | Contents |
|---|---|
| `NAME.out` | Demo stdout |
| `NAME.err.raw` | Demo stderr, including the entry marker |
| `NAME.err` | Stderr with the exact marker line removed |
| `NAME.trace` | Syscall trace, for traced cases |
| `NAME.baseline.*` | The uninjected run used to select a fault site |
| `NAME.injected` | Extracted injected syscall records, where applicable |

These files are overwritten when that case is rerun. The entry marker is
`__SASH_CPP_DEMO_BEGIN__`, emitted by C++ when `SASH_CPP_ERROR_TRACE` is set.
It is a calibration aid, not a runtime diagnostic. The shell removes only
that complete line from stderr.

## Path 1: a child exits 7, then context decides what happens

```sh
bash examples/error_paths.sh --basic --case exit7-caught build
bash examples/error_paths.sh --basic --case exit7-checked build
bash examples/error_paths.sh --basic --case exit7-discarded build
```

In C++, all three begin with the same operation:

```cpp
demo.process(rt::try_execute(demo.helper, {"exit", "7"}));
```

The brace list supplies the argument strings. `try_execute` builds owned
command strings, rejects interior NUL, and calls the Linux implementation.
That implementation prepares the `char *argv[]` pointers before fork,
creates the completion pipe and shared status cell, forks, then reaps.
The helper's exit 7 becomes:

```text
ProcessResult {
    executable: "build/command-helper",
    outcome: ProcessExit { status: 7 }
}
```

That is explanatory notation, not a second representation in the code.
The actual outcome is a `std::variant` holding `ProcessExit`.
Returning it does not print a diagnostic or throw merely because 7 is
nonzero. A child's own stderr is still inherited; caught or discarded
handling does not silence it. `Demo::process` supplies the consequence:

| Handling | C++ action | Observable result |
|---|---|---|
| Caught | Inspect the variant and print its exit payload | stdout: `caught exit: 7`, then `continued`; status 0 |
| Discarded | Explicitly ignore the already computed result | stdout: `continued`; status 0 |
| Checked | Call `rt::check(result)` | stderr: `error_paths: build/command-helper: command failed (exit 7)`; status 7 |

Discard does not skip launching or reaping the child. It discards the
outcome after the work has happened.

In the checked case, `check` constructs a `CommandError` containing the
status and message, then throws it. Execution does not resume after that
call. It unwinds to `rt::run`, whose matching catch prints the diagnostic
and returns 7 to `main`. The later `demo.say("continued")` is not executed.

Here an "uncaught" operation failure still has a deliberate program
boundary catch. It is not an unhandled C++ exception invoking
`std::terminate`, nor a SIGABRT.

### Why not return one integer everywhere?

`ProcessResult` distinguishes a normal exit, a terminating signal, and
setup failure. A normal exit 143 and SIGTERM are different outcomes even
though checked propagation encodes the latter as 128 + 15.

Likewise, a program genuinely returning 127 is not failed `execve`.
The latter is a `SetupError` carrying errno and operation context, not a
`ProcessExit`. The child-reporting path later in this tour explains how
that distinction survives a pre-exec child exiting 127.

`try_execute`'s name does not promise that no C++ exception can escape:
building strings and vectors can still throw allocation exceptions. Its
ordinary command and OS setup outcomes are what it exposes as values.

## Path 2: failed capture, stack unwinding, unchanged assignment

```sh
bash examples/error_paths.sh --basic --case capture-command-caught build
```

This path uses exception handling directly rather than inspecting a
`ProcessResult`. The essential scenario, with its fixed failing branch
substituted, is:

```cpp
std::string target = "unchanged";
demo.operation([&] {
    target = rt::capture_stdout([&] {
        rt::write("partial");
        rt::execute(demo.helper, {"exit", "7"});
    });
});
demo.say("target: " + target);
```

The shared capture wrapper is only:

```cpp
platform::StdoutCapture capture;
body();
return capture.finish();
```

The `StdoutCapture` constructor records fd 1's flags, creates a memfd,
saves stdout with `F_DUPFD_CLOEXEC`, and installs the memfd on fd 1 with
`dup2`. Private descriptors are kept above 2, including when standard
slots start closed. This redirects actual fd 1, not just runtime string
output; the exec'd helper inherits that captured stdout.

For an ordinary open stdout, an illustrative descriptor state is:

```text
Before capture:  fd 1 -> runner's NAME.out
During capture:  fd 1 -> memfd
                 fd 3 -> same memfd
                 fd 4 -> saved runner output
```

`write("partial")` writes seven bytes to the memfd. The external command
then exits 7. `execute` is `check(try_execute(...))`, so it throws
`CommandError`, unlike the unexamined `try_execute` in the previous path.
The process-launch owners have already finished their foreground cleanup
before this checking step throws.

That exception exits `body()` before `capture.finish()` runs. The local
capture owner is nevertheless destroyed. Its destructor restores fd 1
with `dup2`, restores the original descriptor flags, then its member
owners close the saved fd and memfd in reverse construction order.

No string is returned from capture, so the assignment to `target` never
occurs. This is ordinary C++ evaluation order: obtain the right-hand value
before assigning it. The partial bytes existed only in the discarded
capture storage, not in the runner's output file.

`Demo::operation` catches this one operation's `CommandError`. Its caught
mode prints the error through the now-restored stdout and returns:

```text
caught command 7: build/command-helper: command failed (exit 7)
target: unchanged
continued
```

In discarded mode, the same unwind and restoration happen without the
caught-error line. In checked mode, the exception passes through to
`run`, so neither the target report nor `continued` runs.

This is not whole-function suppression. Catching around a complete
source function would skip every later command in that function after
the first throw. Here the catch is deliberately around one capture
operation; whole-function checkedness remains separate work.

## What strace fault injection actually changes

An option such as:

```text
-e inject=read:error=EIO:when=7
```

asks strace to suppress the seventh `read` syscall of the traced task and
substitute failure with EIO. This is not a request for the filesystem to
produce a real I/O error. The libc wrapper sees the injected Linux syscall
result and exposes the familiar C result, -1 with `errno == EIO`.

The counter is per syscall and per tracee, not "line seven in the trace"
or "the seventh syscall of any kind." `when=7` selects that occurrence
once; `7+` would select it and every later occurrence, while `7+2` would
select every other occurrence starting there. The showcase's ordinary
probes use one occurrence so cleanup or a retry can subsequently succeed.

`-e trace=...` chooses what to observe; `-e inject=...` changes execution.
Ordinary showcase fault traces do not use `-f`: they follow the C++ demo,
not the helper it forks. The child-close probe explicitly adds `-f`.

Fault injection is powerful precisely because it is synthetic:

| Injection | What it proves, and what it does not |
|---|---|
| `error=EIO` | Exercises handling of that returned error. It does not reproduce the device or filesystem failure behind a real EIO. |
| `error=EINTR` | Exercises the retry arm without performing that syscall on the injected occurrence. It does not reproduce every possible signal timing. |
| `retval=0` on a nonempty write | Exercises the runtime's zero-progress guard; no bytes were written. |
| `retval=1` on a read | Supplies a count, not a byte in the caller's buffer. The protocol case checks the count and does not use the byte. |
| Error on `close` | Skips the close. This differs from Linux real-error cases where the descriptor has already been released. An injected descriptor leak alone is not evidence of a runtime leak. |

In particular, faking a positive short write would not actually write the
claimed prefix. A subsequent correct retry could then appear to lose
data. The showcase does not use that as proof of short-write correctness.

## Path 3: select the intended fork failure, not a startup syscall

```sh
bash examples/error_paths.sh --case fork build
```

The catalog entry is:

```bash
fault fork process clone '^clone[(]' 1 error=EAGAIN \
    '^error_paths: fork '
```

Read that as: select case `fork`; run C++ scenario `process`; inject EAGAIN
at the first matching `clone` site after entry; require a fork diagnostic.
On the current Linux host, libc `fork()` appears as `clone` and
`waitpid()` as `wait4`. The harness targets observed kernel calls, not
their C wrapper names.

### Baseline, site selection, injection, assertion

`fault` first runs an uninjected baseline. Its expected status is normally
zero, but intentionally failing scenarios such as `capture-command` and
`exit7` have a baseline status of 7.

The `ordinal` awk routine then maintains two different counts:

1. Count every occurrence of the requested syscall, including loader
   activity before the entry marker.
2. After the marker, count only calls matching the site's selector until
   the requested site occurrence is found.

It returns the first count at that point. That absolute syscall ordinal,
not the second count, becomes strace's `when=N`.

This matters especially for `read` and `close`: the loader has already
used them before `main`. "First capture read" is not "first read since
exec." The marker write itself also counts when selecting later writes,
but is not eligible as a post-marker site.

`'^clone[(]'` is an extended regular expression for a line beginning
`clone(`. The bracket spelling makes the literal parenthesis survive the
shell/awk quoting layers without a backslash ambiguity.

The runner repeats the same scenario and fd context with the injection
option, then requires all of the following: an `(INJECTED)` record for
the syscall, demo status 2, no leaked stdout, and the expected diagnostic
site. A nonzero exit alone is insufficient.

### Through the C++ owners

At the failed fork the completion pipe and shared cell already exist,
but the `Child` owner has not been constructed: there is no child pid to
own. The implementation saves errno before allocating diagnostic strings,
throws `SetupError`, and unwinds the mapping and pipe owners.

The body-level trace shows the acquisition and reverse-order cleanup:

```text
pipe2([3, 4], O_CLOEXEC)                          = 0
mmap(..., MAP_SHARED|MAP_ANONYMOUS, ...)          = address
clone(...)                                      = -1 EAGAIN (...) (INJECTED)
munmap(address, 8)                               = 0
close(4)                                        = 0
close(3)                                        = 0
write(2, "error_paths", 11)                       = 11
...
```

`try_execute` catches that `SetupError` and returns it in `ProcessResult`.
The demo is in checked mode, so `check` throws the stored error again.
`run` reports it and returns 2. Capturing errno in the error object keeps
later allocation and cleanup from changing the reported cause.

### Inspect the evidence and remove the fault

```sh
awk '/__SASH_CPP_DEMO_BEGIN__/ { body=1 } body' \
    build/error-demo/fork.trace
bash examples/error_paths.sh --case fork --without-injection build
```

The second command is a negative control. Expect runner status 1 and
`fork did not inject its intended syscall`, not a successful demonstration.
The healthy C++ invocation returning 0 is not enough to satisfy a probe
that claims it exercised failed fork. A missing tool or usage error is
not the intended negative-control result.

That command overwrites this case's evidence. Run it again without
`--without-injection` to restore the injected trace.

## Path 4: EINTR is observed, retried, and compared with the baseline

```sh
bash examples/error_paths.sh --case retry-completion-read build
```

The C++ scenario is a successful foreground command. The injected read
is the parent's read of its completion pipe:

```text
close(4)                                        = 0
read(3, ..., 1)                                 = -1 EINTR (...) (INJECTED)
read(3, "", 1)                                  = 0
close(3)                                        = 0
wait4(child, [{... WEXITSTATUS(s) == 0}], 0, NULL) = child
```

`wait_for_exec` sees the negative result and EINTR and loops. The next
real read reaches EOF. It does not turn the interruption into a setup
error or treat it as data. The parent then waits normally and the demo
prints `continued`.

The shell takes a different assertion branch for `error=EINTR`: it
requires the baseline exit status and byte-for-byte identical filtered
stdout and stderr, using `cmp`. It still requires the injection record.
Merely running successfully without injection would not establish that
the retry arm ran.

The same approach covers interrupted writes, descriptor operations,
rewind, and wait. It does not blindly retry `close`.

## Path 5: identical `dup2` errors, different lifetime phases

Compare:

```sh
bash examples/error_paths.sh --case capture-restore build
bash examples/error_paths.sh --case cleanup-restore build
```

Both target a restoring `dup2`, but the first fails during ordinary
completion and the second during a destructor.

### Failure from `finish`: propagate the original operation error

`capture-restore` runs a successful capture body. `StdoutCapture::finish`
calls `restore`, which returns an error value after injected EIO.
`finish` converts that value into a `SetupError`.

Because restoration failed, the frame is still marked active. As that
exception unwinds, the capture destructor calls `restore` again. The
single-occurrence injection is now exhausted, so this second attempt
succeeds:

```text
dup2(3, 1)                        = 1
write(1, "captured", 8)            = 8
dup2(4, 1)                        = -1 EIO (...) (INJECTED)
dup2(4, 1)                        = 1
fcntl(1, F_SETFD, 0)              = 0
close(4)                          = 0
close(3)                          = 0
```

The successful cleanup does not erase the original error. The program
boundary reports `error_paths: restore stdout: Input/output error` and
returns 2. This is not an EINTR retry: it is a destructor making another
restoration attempt after ordinary completion failed.

Restoring `F_GETFD`/`F_SETFD` descriptor flags separately matters because
`dup2` does not preserve the old destination's close-on-exec flag.
The initially closed-stdout path instead restores the original state by
closing fd 1.

### Failure in the destructor: stop rather than continue on wrong stdout

`cleanup-restore` starts with the failing capture from Path 2. The helper
has already exited 7 and been reaped. That `CommandError` is unwinding;
`finish` is never called. Now the destructor's own `dup2` fails:

```text
write(1, "partial", 7)             = 7
... helper exits 7 and is reaped ...
dup2(4, 1)                        = -1 EIO (...) (INJECTED)
write(2, "sash-cpp: cleanup failed: ", 26) = 26
write(2, "restore stdout", 14)     = 14
...
```

`cleanup_failed` formats the errno number into a stack buffer with
`std::to_chars`, writes directly to fd 2, and calls `std::_Exit(2)`.
There is no heap-built cleanup message, second thrown exception, outer
catch, or `continued`. `_Exit` also bypasses remaining C++ destructors
and stdio flushing. The kernel closes descriptors and unmaps memory;
it does not thereby kill or reap all children.

The fatal diagnostic is `sash-cpp: cleanup failed: restore stdout (errno 5)`,
not the original command-error diagnostic. The original status 7 does not
survive as the program status. The runtime cannot safely continue while
stdout may still point at capture storage.

This is the destructor-failure policy even outside unwinding; this case
arranges for it to happen while another exception is active. The same
distinction applies to explicit `FileDescriptor::close`, which can throw
`SetupError`, versus failure from its destructor, which is fatal.

## Path 6: failed cleanup wait and the supervisor above the tracer

```sh
bash examples/error_paths.sh --case cleanup-wait build
```

This case injects **two** failures. The ordinary `fault` machinery targets
the completion read; its special `cleanup-wait` branch also selects the
baseline's wait syscall ordinal:

```text
clone(...)                         = child
close(4)                           = 0
read(3, ..., 1)                    = -1 EIO (...) (INJECTED)
wait4(child, ..., 0, NULL)         = -1 EIO (...) (INJECTED)
```

The read error starts unwinding before normal `child.wait()` is reached.
The live `Child` owner therefore calls its reap routine from its
destructor. The second injection makes that reap fail too.
`cleanup_failed("wait for child", ...)` exits 2 immediately.

The fixture child might still be running, or might already be a zombie.
A controller outside the dying demo must own what happens next.
`error-supervisor` sets `PR_SET_CHILD_SUBREAPER`, forks strace, then waits
with `waitpid(-1, ...)` until ECHILD. Orphaned descendants are reparented
to this subreaper rather than escaping to the system's normal reaper.
It collects their statuses but returns the status of its direct child,
strace, which propagates the demo's status.

This is why the supervisor is **above** strace. It is not itself subject
to this invocation's injected waits, nor is the outer timeout monitor.
It is a separate executable, not linked to the runtime it is containing.
Its `execvp` finds the strace utility through PATH; the runtime's own
`execve` still uses explicit command paths without PATH search.

To inspect the supervisor alone, without starting a nested tracer:

```sh
strace -f -e trace=prctl,clone,wait4 \
    build/error-supervisor sh -c 'sleep 0.05 & exit 7'
```

The controller returns the direct command's 7, not the sleeper's 0.
When the sleeper outlives its shell, the trace also shows its separate
reap as an adopted child; the sleep encourages that ordering but is not
a scheduling guarantee. Do not wrap the full showcase in another
`strace -f`: the inner tracer needs to trace its own demo. Read the
harness-produced `.trace` files instead.

The supervisor and timeout are fixture containment. They do not implement
production background jobs, force-kill policy, END, or general shutdown.

## The child-close probe: per-process counters and truthful exec errors

```sh
bash examples/error_paths.sh --case child-reader-close build
```

This is intentionally not an ordinary `ordinal` probe. It adds `-f` and
injects `close:error=EIO:when=1`. Each new tracee has its own counter, so
the fork child fails its first close, before exec. The parent's first
loader close is also affected; this is not a child-only fault selector.
Both injections are shown.

With PID labels substituted, part of a trace can look like:

```text
P clone(...)                     = C
C close(4 <unfinished ...>
P read(4, <unfinished ...>
C <... close resumed>)           = -1 EIO (...) (INJECTED)
P <... read resumed>"", 1)        = 0
P wait4(C, [{... WEXITSTATUS(s) == 127}], 0, NULL) = C
```

The awk check in `child_close` records failures and exec attempts by PID,
recognizes both complete and resumed close records, and checks at end
that the actual fork child's close failed and that child never attempted
exec. Neither another process's failed close nor an arbitrary exit 127
is enough. Tracer scheduling can split and interleave syscall lines, so
assuming the next line belongs to the child would be wrong.

### EOF is completion, not a success verdict

The child closes the pipe reader, then attempts `execve`. Its writer is
close-on-exec. The parent closes its own writer and reads:

```text
successful exec -> child writer closed by exec -> parent sees EOF
pre-exec failure -> child exits, closing writer -> parent also sees EOF
```

Therefore EOF cannot distinguish the two paths.

`SharedChildFailure` allocates an anonymous `MAP_SHARED` mapping before
fork. Placement `new` constructs a `ChildFailure` in that already allocated
storage; this use of `new` does not allocate another heap block. The
child's inherited mapping is shared, not a private copy-on-write error
variable.

On failure, `child_failed` stores the operation, publishes errno with a
release store to a lock-free atomic, then calls `::_exit(127)`. The parent
reaps and acquire-loads that errno before interpreting the result. A
nonzero error selects the stored operation and reconstructs `SetupError`,
overriding interpretation as an ordinary exit 127. The atomic publication
makes the preceding operation field available with the error.

This avoids a fallible child write of an error payload. The pipe signals
completion only; the error data is in the preallocated shared cell.
The child uses prebuilt arguments and does not allocate or unwind
inherited parent owners. The selected single-threaded model is recorded
in [DD#2](../../notes-exp/decisions/DD02-foreground-fork-exec.md).

## When printing the original error fails too

```sh
bash examples/error_paths.sh --case diagnostic-message build
```

The helper really exits 7. `run` catches `CommandError`, but its reporter
then encounters an injected EIO writing the message. The reporter has
four complete-write operations:

```cpp
if (write_all(2, program) || write_all(2, ": ") ||
    write_all(2, diagnostic) || write_all(2, "\n")) {
    return 2;
}
return status;
```

The C-like short circuit is significant: after the message write fails,
there is no newline attempt or recursive attempt to report the reporting
failure. The status becomes 2 rather than 7. Stderr contains only
`error_paths: `, without a final newline.

The shell's pretty-printer adds display newlines, so inspect the actual
bytes when that distinction matters:

```sh
od -An -tx1 build/error-demo/diagnostic-message.err
```

`diagnostic-output`, `diagnostic-separator`, and `diagnostic-newline` select
the other write positions. They demonstrate that a printed prefix is not
proof that the complete original diagnostic reached the caller.

## Reading the remaining cases

The same machinery explains the rest without requiring a second tutorial:

| Case family | Point to watch |
|---|---|
| `capture-read` | `finish` has already restored stdout and closed its backup before rewind/read. A read error still prevents assignment; the storage owner closes during unwind. |
| `wait-no-status-*` | The helper sets SIGCHLD to ignored before execing the demo, producing genuine kernel auto-reaping and ECHILD. `Child::reap` clears its owned pid and reports the error. Faking ECHILD could instead abandon a child the kernel still expects someone to reap. |
| `capture-closed-*` and `process-pipe-relocate*` | Closing standard slots lets the kernel allocate a private fd in 0..2; the runtime raises it with `F_DUPFD_CLOEXEC` and restores the original closed slot. |
| `regex-*` | `compile_regex` translates `std::regex_error` into `CommandError`; `regex-raw` reaches the dedicated standard-exception catch without that adapter. |
| `bad-alloc-injected-*` | Explicitly throws `std::bad_alloc`; it is not host memory exhaustion and is different from the syscall-level mmap/ENOMEM case. |
| `length-*` | Requests a string size beyond `max_size`, reaching a real `std::length_error` without attempting to exhaust memory. |
| `completion-protocol-synthetic` | Fakes a positive pipe read count. The runtime rejects unexpected data without reading the uninitialized byte. No legitimate runtime child writes this payload. |

The program boundary catches specific expected error classes, not every
possible C++ programming error. Arbitrary corruption of shared status
and uncaught programming exceptions are not manufactured by this suite.
Nor do these cases claim coverage of runtime features not yet implemented.

For any additional case, follow the same chain: identify the failed
primitive, the returned value or thrown type, the owners still alive,
the layer that prints, and the process whose status is being asserted.
Then require trace evidence that the intended primitive actually failed.
