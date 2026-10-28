---
title: "The first C++ program"
date: 2026-10-28
---

DG set the initial direction: Linux, C++17, sparse `std::map`, ECMAScript
`std::regex`, and memfd capture. When implementation started, DG selected
exceptions for abort-on-uncaught-error and emphasized that caught and
suppressed commands must still work. DG also directed a pull first, work
under `sash-cpp/` on a branch, and separate experiment records under
`notes-exp/`. These choices are recorded in DD#1.

The checkout fast-forwarded to `899179d3`; the implementation is on
`cpp-runtime-first-slice`. The AI implemented the first vertical slice,
without changing the compiler or linking its Rust runtime.

## A program that runs

`sash-cpp/examples/greetings.cpp` is the hand-written counterpart of the
adjacent sash program. A real `greet` function writes its argument, a
sparse map holds Ada and Grace, an ordinary loop and regex branch select
Ada, and a capture collects the function's stdout. The final echo prints
`Hello, Ada!`.

```sh
cd sash-cpp
mk -p 1 test
./build/greetings
```

The runtime is a normal C++ static library. Its shared interface has no
Linux headers. Raw output and capture ownership live behind the private
platform boundary; there is no IR reader, temporary interpreter, variable
registry, or generated dispatcher.

## Outcomes before consequences

`try_write` and `try_echo` return a status and owned diagnostic without
printing. A condition can inspect the result; an explicit discard can
ignore it; `check`, `write`, and `echo` raise `CommandError` for failure.
Setup failures have a distinct exception type. `run` catches specific
expected failures and reports through a nonallocating final-output path.

This keeps the distinction DG asked for. It does not yet solve propagation
through source functions or pipelines. Catching around a whole suppressed
function would skip its remaining commands, so that is explicitly not the
planned implementation.

Capture redirects actual fd 1 to a close-on-exec memfd, invokes a void
body in-process, restores stdout, then reads the file into a counted
string. Nested captures and exceptions restore the enclosing destination.
Originally closed stdout and its descriptor flags survive the operation.
The low-level result retains NUL and final newlines. The example uses
`write` inside capture, avoiding a premature decision about source-level
newline trimming.

The initial implementation deliberately uses raw, single-threaded output.
Buffered C/C++ output and surviving background writers are outside its
contract. Failure to restore state during unwinding is diagnosed and
terminates; full END and original-error/secondary-cleanup coordination
still belong to the lifecycle work.

## Evidence and limits

GCC 9 and Clang 20, both using the host libstdc++, pass eight native
fixture groups and the independently specified example output. The cases
cover checked/caught/discarded failures, program-name diagnostics, 256 KiB
binary capture, empty echo words, nested capture and failed assignment,
stdout restoration, actual memfd allocation failure, descriptor lifetime,
and ECMAScript backreferences versus malformed patterns. Formatting passes.

GCC ASan also passes with the system linker selected explicitly. Full
UBSan does not: its metadata probing attempts pipes under the deliberate
zero-fd limit and misreports a valid standard exception's vptr. An
independent standard-exception control reproduces it; strace records
EMFILE on those pipe attempts. Clang's sanitizer runtime separately fails
at startup even for an empty program. The tooling task retains these
limitations rather than weakening the fixture or claiming a green run.
Independent use-after-free and overflow controls prove the respective
sanitizers can report real errors.

The first build recipes exposed a local mk pitfall: rule targets using
derived default paths did not follow command-line `BUILD` overrides.
Simple phony recipes now expand variant paths directly. GCC, Clang, and
ASan use separate directories, and the documented default command works.

The actual sash CLI caught one mistake in the hand-written counterpart:
a for-in source uses bare `@names`, not an empty-disposition splat.
The corrected file passes `--emit-ir`. Initially, native execution could
not be compared: the separate sash-rt build resolves rustix 1.1.6 and
fails on the already-filed private `runtime` module. No dependency pin or
other native workaround was added. The producer plan remains uncommitted
as DG originally requested.

The first runnable milestone is reached; the runtime roadmap is not
complete. External launch, general redirection, pipelines and readers,
END/signals, most value operations, and macOS remain. Call-context
propagation and launch strategy still need their consequential choices
settled before the next process milestone.

## Native reference follow-up

DG supplied `~/src/safebash/_tmp/sash` as the usable native compiler.
With that binary and the runtime archive from the same checkout, the
existing `sash-example` target compiles and runs `greetings.sash`.
Its output matches both `greetings.expected` and the separately run C++
executable byte-for-byte: `Hello, Ada!\n`. Both programs exit successfully.

The README now gives the exact reference command. Native comparison is
no longer blocked; the local fresh-build rustix issue remains unchanged
and no native dependencies were edited.
