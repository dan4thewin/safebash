---
title: "DD#13 - Owned NUL record reads"
status: accepted
---

DG authorized this bounded unit after the proposal to retain native DD#99's
NUL-only, no-flags model: a stream or counted string produces an owned string
array. There is no new delimiter option or declaration-dependent target kind.

## Values and boundaries

The C++ entry points are:

```cpp
Array<std::string> read(std::string_view source);
Array<std::string> read(int fd = 0);
Array<std::string> LineReader::read();
```

Every result owns its bytes and has dense indices beginning at zero.
Scalar strings and string-cell values use the same operation. NUL alone
terminates records; newline, CR, whitespace, and high bytes remain data.

Both native DD#99 forms use terminator semantics, which this target retains:

| input bytes | records |
|---|---|
| empty | none |
| `NUL` | one empty record |
| `a NUL` | `a` |
| `a NUL NUL` | `a`, empty |
| `NUL a NUL` | empty, `a` |
| `a NUL b` | `a`, `b` |

A final NUL terminates a real record without inventing another. A nonempty
unterminated tail survives. Assigning a completed result replaces every old
cell; shorter input leaves no suffix, and empty input or EOF clears the target.
The caller states any other separator by converting it upstream, for example
with a counted-NUL `gsub` format. Maps remain separate array-to-map conversions.

## Buffering and ownership

The borrowed-fd overload defaults to stdin and never closes or reflags that
descriptor. A command-backed callback uses its existing `LineReader`: bulk
reading consumes its prefetched tail before any further syscall. It does not
construct a fresh raw-fd reader that could discard buffered data.

The AI chose a reversible, simple first implementation: gather the remaining
buffered/stream bytes into a local string, then invoke the same counted-string
splitter. This temporarily holds the whole input and the owned fields, rather
than introducing a second incremental parser or stream framework. Dense indices
reuse the checked collection-count helper. A private refill operation shares
the existing EINTR retry and labels bulk I/O failures `read NUL records`;
existing command-output read diagnostics are unchanged.

DD#3's command/pipeline owners still close the pipe before waiting, including
consumer unwinding. Producer outcomes remain separate from the returned records.
A producer failure after completed consumer assignment does not roll back that
assignment.

## Failure boundary

Empty input is an ordinary empty result, not an I/O error or an automatic
abort. I/O, allocation, and length failures escape before any partial result
is assigned; a destination-borrowed source remains alive until evaluation
completes. Already-consumed stream bytes are not rewound or restored, and a
retry after failure has no transaction guarantee.

These value helpers introduce no source taint/admission checks, flags,
cancellation, checkpoints, timeout, END policy, or hidden child verdict.
Source statement/condition disposition remains a future consumer call-site
responsibility. There is still no IR reader, emitter, or producer change.
