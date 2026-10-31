# External C++ transpiler experiment

This project is independent of the Rust compiler and its existing C backend.
Code lives here; journal, tasks, decisions, and status live in `../notes-exp/`.
Do not put experiment records in `../notes/`.

Use C++17, `std::map`, ECMAScript `std::regex`, and Linux memfd capture.
DG makes consequential design decisions; see `../notes-exp/decisions/`.
The runtime task is `../notes-exp/tasks/cpp-transpiler-runtime-before-ir.md`.

Build and exercise changes with `mk -p 1 test` in this directory. Use distinct
`BUILD` directories when changing compilers or sanitizer flags. `mk -p 1
sash-example` additionally compiles all five sash examples with the real
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

External pipelines retain every stage outcome. Checking uses the rightmost
failed stage, not the largest numeric status; setup/cleanup errors are
recorded separately and the first takes precedence (experiment DD#3).
Unstarted stages have no invented exit status. Close every remaining parent
endpoint before waiting for any child, including partial construction and
consumer unwinding.

External-only capture drains a pipe into exact counted bytes and returns
them separately from pipeline outcomes; it does not rebind parent stdio
or depend on memfd. General in-process capture still uses memfd. Checking
precedes assignment; consumer read/allocation exceptions escape even from
the inspectable form instead of becoming EOF or a child verdict.

Command-backed reader callbacks receive an explicit `LineReader &`; do not
rebind parent stdin. Close their pipe before waiting for the producer,
including during unwinding and partial launch failure.

Later-fork/read injection uses a live producer that cannot exit before the
selected entry. strace counts restarted syscall entries too. Higher child
close ordinals use the static supervisor's `--close-prelude`; injecting them
directly into a dynamically loaded parent can fail before `main`.

`tests/capture_faults.sh` selects the first/second positive data read by fd
after the marker, using one finite large binary producer. Its no-memfd
probe first proves injection works on a real memfd capture, then requires
no memfd call from pipe capture; an uncalled syscall was not injected.
The runtime test executable alone overrides allocation to fail a collector
request without exhausting host memory; matching delete overloads remain
paired and the fault is disabled outside its scope.

Keep generated-code examples source-shaped: real functions, ordinary control
flow, and standard C++ values. Preserve command outcomes for caught/discarded
uses; checking an outcome is what raises an aborting exception.
