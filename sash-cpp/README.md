# sash-cpp

An independent C++17 runtime for a future source-shaped sash transpiler.
There is no IR reader or emitter yet. The first example is hand-written:
[greetings.cpp](examples/greetings.cpp) corresponds to
[greetings.sash](examples/greetings.sash), retaining a function, sparse
collection, loop, regex condition, and stdout capture.

On Linux:

```sh
cd sash-cpp
mk -p 1 test
./build/greetings
```

The program prints `Hello, Ada!`. The build produces a normal C++ static
library, `build/libsash_cpp.a`; it does not build Rust or link sash-rt.
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

With the real sash CLI and runtime already built in the parent checkout,
`mk -p 1 sash-example` compiles and runs the sash counterpart against the
same expected output. Override `SASH` and `SASH_RT_LIB` for other locations.
This additional target is not a prerequisite for native C++ tests.

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

Setup failures are separate `SetupError` exceptions. Do not implement
suppression by catching every exception around a whole function: that
would skip commands after its first failure and could hide setup errors.
Function checkedness and pipeline aggregation are not implemented yet.

`capture_stdout` redirects actual fd 1 to an owned Linux memfd and runs
its void body in-process. Nested frames and exception unwinding restore
the previous destination, including an originally closed stdout and its
descriptor flags. This low-level operation returns exact bytes, including
NUL and trailing newlines; source-level newline treatment remains an open
choice. The example uses `write` inside its capture, so that choice does
not affect its equivalence.

The runtime is initially single-threaded and its output helpers use raw
descriptor writes. Captured code must not mix in buffered stdio/iostream
output or retain background writers. Script-fd reservation, arbitrary
redirection, external processes, pipelines, END, and signals are later
work. Failure to restore descriptor state during unwinding is diagnosed
and terminates rather than continuing on the wrong stdout.

Linux operations are isolated in `src/platform_linux.cpp`; the shared
interface and example have no Linux headers. macOS capture storage is the
next platform choice, not a silent fallback for a failed Linux memfd.

Project [status](../notes-exp/status.md),
[tasks](../notes-exp/tasks/cpp-transpiler-runtime-before-ir.md),
[decisions](../notes-exp/decisions/DD01-runtime-starting-points.md), and
[journal](../notes-exp/journal/001-first-cpp-program.md) live in
`../notes-exp/`, not the native compiler's `notes/`.
