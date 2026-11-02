# Collection operations and owned arguments

**2026-11-02.** DG directed implementation of the bounded seventh unit
after selecting missing-cell typed zeros in
[DD#6](../decisions/DD06-missing-cell-reads.md) and nonnegative indices,
population length, and live sorted traversal in
[DD#7](../decisions/DD07-collection-indices-length-traversal.md).
The AI implemented the runtime helpers and harmless
[C++](../../sash-cpp/examples/option_report.cpp)/[sash](../../sash-cpp/examples/option_report.sash)
option-report pair.

## Implementation

Containers remain ordinary `std::map` values through transparent
`Array<T>`/`Map<T>` aliases. `cell` copies a found value or returns typed
zero without insertion; `set_cell` and `unset` validate numeric indices
before mutation, while `.clear()` remains standard C++.
Length is population, including stored empty strings and integer zeros.

The AI chose by-value reads and loop variables to avoid dangling references
after mutation. One shared live range saves an independent key, calls
`upper_bound` after each body, and copies the next key/value. It observes
later insertions and updates, skips deletions, and safely handles current
erasure, clearing, aliases, `continue`, and `break`.
Borrowed ranges reject temporary containers. Key snapshots remain copies.

Joins preserve counted binary values/separators and integer rendering
without a 4 KiB buffer. Pairwise array-to-map helpers retain native DD#99's
odd-tail choices: elide, lazy fallback key, or lazy requirement failure.
Duplicate keys overwrite, and an empty even array stays an empty map.
Map values are not advertised as an inverse pairwise array.

Scalar and cell defaults/requirements use the same lazy functions.
The subject is its typed value, not membership: empty string and integer
zero trigger dispositions, string `"0"` does not. Required values raise
the existing `CommandError`; invalid indices raise `SetupError`.
Allocation errors escape to the existing allocation-safe program reporter.

Argument helpers build ordinary owned string vectors before launch.
An empty scalar/default is one argument; explicit scalar elision is none.
Splats keep stored empty/zero cells; an empty splat defaults to exactly one
argument, elides to none, or explicitly requires a value.
Bulk preparation does not publish partial arguments on allocation failure,
and completed vectors do not become views of their source collections.
The existing exec boundary still rejects interior NUL before launch.
There are no new unchecked contexts or implicit lifecycle checkpoints.

The pair retains real preparation and argument-reporting functions,
forwarded and doubly aliased collection references, sparse cells, integer
values, pair construction, a copied key array, and a sorted key loop.
Its stable order matches both targets; mutation controls pin the selected
C++ next-greater-key rule rather than equating it with native table traversal.
The independent expected output has seventeen lines.

## Evidence

- GCC and Clang: 41 fixture groups, seven C++ goldens, existing Linux
  fault gates, and the unchanged 169-case catalog.
- Supported Linux GCC ASan, real Apple Clang/libc++, and Apple ASan:
  the same 41 groups and seven C++ goldens.
- DG's supplied real native CLI and companion archive: seven sash goldens.
  The seventh pair's exact stdout and empty runtime stderr were checked
  separately on both hosts and the native reference.
- Five isolated final-source mutations each return status 1 for the
  intended failure: strict missing-cell reads, extent length, snapshot
  traversal, dropping stored empty/zero splat cells, and incomplete
  negative-index rejection. None rely on a timeout or compilation failure.

The focused selector passes unmodified first; implementation checksums
remain unchanged after the mutations. Temporary mutation executables,
objects, and transfer archives are removed; sources and logs stay in the
session workspace.

No IR reader, emitter, producer, native compiler/runtime, or native spec
changed. The protected JSON producer draft remains uncommitted and was
excluded from Mac transfers. The
[runtime roadmap](../tasks/cpp-transpiler-runtime-before-ir.md) still tracks
argv/environment ingress, remaining text/arithmetic/regex/read families,
general redirects, public wait, and forked-C++/background ownership.
