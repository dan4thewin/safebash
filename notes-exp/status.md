## First runnable C++ slice

DG directed the external-transpiler work to start with a hand-written C++
equivalent of a quasi-trivial sash program.

`sash-cpp/` now contains an independent C++17 library and a runnable
`examples/greetings.cpp`. It retains a real function, sparse `std::map`,
ordinary loop and branch, ECMAScript regex, and Linux memfd capture:

```sh
cd sash-cpp
mk -p 1 test
./build/greetings
```

Output is `Hello, Ada!`. Commands have explicit outcomes plus checked
wrappers; caught and discarded failures do not print or stop later work.
Nested capture, unwind restoration, binary output, closed stdout, and
descriptor exhaustion are exercised natively.

GCC and Clang pass the eight fixture groups and the example. Host-GCC
AddressSanitizer passes too. Full UBSan is blocked by its metadata probing
under the zero-fd limit; see [the tooling task](tasks/sanitizer-host-compatibility.md).

The sash counterpart compiles and runs with DG's supplied
`~/src/safebash/_tmp/sash` and the runtime archive from that same checkout.
Its output matches the C++ executable and the expected `Hello, Ada!\n`
byte-for-byte. The local fresh-build
[rustix issue](../notes/tasks/bug-fresh-clone-rustix-runtime-private.md)
is unchanged, but no longer blocks this comparison. The experiment has
not changed or pinned native dependencies.

The checkout was fast-forwarded to `899179d3`, then branch
`cpp-runtime-first-slice` was created. Project records stay here, not in
the native compiler's `notes/`. See [the first journal entry](journal/001-first-cpp-program.md)
and [DD#1](decisions/DD01-runtime-starting-points.md).

The [runtime task](tasks/cpp-transpiler-runtime-before-ir.md) remains open:
external launch, redirections, pipelines, END/signals, and most value
operations are not implemented. Next is a single external command, with
call-context and launch choices settled before extending suppression or
forking C++ function bodies. macOS follows the first Linux launch/capture slice.

The independent producer plan is still the local, uncommitted draft
`tasks/emit-ir-json-for-c-transpiler.md`, as requested. No IR changes or
consumer have been implemented.
