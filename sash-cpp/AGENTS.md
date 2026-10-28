# External C++ transpiler experiment

This project is independent of the Rust compiler and its existing C backend.
Code lives here; journal, tasks, decisions, and status live in `../notes-exp/`.
Do not put experiment records in `../notes/`.

Use C++17, `std::map`, ECMAScript `std::regex`, and Linux memfd capture.
DG makes consequential design decisions; see `../notes-exp/decisions/`.
The runtime task is `../notes-exp/tasks/cpp-transpiler-runtime-before-ir.md`.

Build and exercise changes with `mk -p 1 test` in this directory. Use distinct
`BUILD` directories when changing compilers or sanitizer flags. `mk -p 1
sash-example` additionally compiles the example's sash counterpart with the
real CLI. Ordinary C++ tests do not require a Rust build or IR input.

Keep generated-code examples source-shaped: real functions, ordinary control
flow, and standard C++ values. Preserve command outcomes for caught/discarded
uses; checking an outcome is what raises an aborting exception.
