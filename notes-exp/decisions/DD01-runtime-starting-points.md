---
title: "DD#1 - External C++ runtime starting points"
status: accepted
---

DG selected these starting points for the external-transpiler experiment:

- Linux first, with macOS following soon.
- C++17 and the normal host C++ standard library.
- Sparse indexed arrays and maps represented by `std::map`; sorted key
  order is acceptable instead of native sash's insertion order.
- `std::regex` with the C++ standard library's ECMAScript grammar.
- Linux `memfd_create` for general in-process stdout capture.
- Exceptions fit abort-on-uncaught-error, but commands must also support
  caught outcomes and explicit suppression.

The implementation lives in `sash-cpp/` in this checkout, on a dedicated
branch. Its journal, tasks, decisions, and status live in `notes-exp/` to
avoid collisions with the native compiler's records.

## Error handling consequence

Do not make every nonzero command outcome an unconditional exception.
Keep an outcome representation that can be inspected in a condition,
discarded explicitly, or checked to raise an exception. Setup failures
must remain distinguishable from the command verdict.

A catch around an entire function is not a complete implementation of
suppression: it would stop the body at the first failing command rather
than allowing later commands to run. Call-context propagation and pipeline
aggregation remain later work.

## Scope of the first implementation

DG asked for a runnable hand-written C++ equivalent of a quasi-trivial
sash program. Start with real functions and control flow, sparse standard
maps, an ECMAScript predicate, and memfd-backed capture. No IR reader or
compiler changes are required.

These are external-target choices, not amendments to native sash's DDs.
