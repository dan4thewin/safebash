# External C++ transpiler experiment

This project is independent of the Rust compiler and its existing C backend.
Code lives here; journal, tasks, decisions, and status live in `../notes-exp/`.
Do not put experiment records in `../notes/`.

Use C++17, `std::map`, ECMAScript `std::regex`, and Linux memfd capture.
DG makes consequential design decisions; see `../notes-exp/decisions/`.
The runtime task is `../notes-exp/tasks/cpp-transpiler-runtime-before-ir.md`.

Build and exercise changes with `mk -p 1 test` in this directory. Use distinct
`BUILD` directories when changing compilers or sanitizer flags. `mk -p 1
sash-example` additionally compiles both sash counterparts with the real
CLI. Ordinary C++ tests do not require a Rust build or IR input.
`mk -p 1 fault-test` adds Linux strace probes; run it without sanitizer
instrumentation. The native command helper is deliberately unsanitized.

`mk -p 1 error-demo` runs the error-path showcase without sanitizers.
`bash examples/error_paths.sh --basic BUILD` needs no strace; `--list`
lists cases and `--case NAME` selects one. Traced cases use a Linux
subreaper so fatal cleanup examples cannot orphan their helper children.
Calibrate fault sites from baseline traces after the entry marker, verify
the actual injected syscall, and keep negative controls capable of failing.

Use the selected single-threaded fork/exec model. Prepare argument storage
before fork; the pre-exec child must not allocate or unwind parent objects.
Keep normal exit, signal termination, and setup failure distinguishable.

Keep generated-code examples source-shaped: real functions, ordinary control
flow, and standard C++ values. Preserve command outcomes for caught/discarded
uses; checking an outcome is what raises an aborting exception.
