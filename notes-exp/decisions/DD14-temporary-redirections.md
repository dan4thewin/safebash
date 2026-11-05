---
title: "DD#14 - Temporary redirections and invoker channels"
status: accepted
---

DG authorized the temporary-redirection trial after clarifying compiler
ownership of descriptor metadata: mirror semantic source descriptors and
channel setup, and expose compiler choices in the IR where necessary.
The AI implemented this runtime-first slice without changing the producer
or its protected proposal.

## Compiler/runtime seam

The native producer already records redirect `:fd`, `:op`, and `:target`.
Its compiler also computes script-referenced descriptor numbers from
redirect destinations, duplication sources, and `close` operands, including
references in functions, captures, and terminal handlers, and supplies that set to its
runtime. An omitted redirect fd is an operator default, not a private
channel identity.

Private pipe, backup, capture, and diagnostic copies are allocated by the
runtime/OS. Native's minimum-100 allocation floor does not assign fixed
channel numbers. This target need not reproduce those private numbers or
export them in the IR.

The C++ entry forms are:

```cpp
run(program, {source_fds...}, body);
run(program, {source_fds...}, body, on_exit, on_error);
```

The old forms delegate with an empty explicit set. Standard descriptors
0/1/2 are always reserved. As updated by
[DD#15](DD15-checked-exit-explicit-err.md), the synchronous driver borrows
and validates the supplied initializer-list storage without allocating.
Membership ignores duplicate entries and needs no sorting. The reservation
scope outlives invoker snapshot failure and both handlers. Private allocation
skips every active reserved number, including originally closed ones.
A late reservation conflicting with a still-owned private descriptor is
an explicit setup error rather than permission to overwrite it.

The future producer should export its already-computed set so that the
consumer does not duplicate compiler analysis. No metadata field spelling,
IR reader, emitter, or producer implementation is introduced here.

## Temporary scope API

Owned `Redirect` values have these factories:

```cpp
Redirect::read(path, fd = 0);
Redirect::write(path, fd = 1);
Redirect::append(path, fd = 1);
Redirect::read_write(path, fd = 0);
Redirect::duplicate(destination, source);
```

Duplication is destination first, matching `2>&1` as `(2, 1)`. Both native
duplication directions use the same primitive after the destination is
resolved. Paths retain counted bytes until the OS boundary.

`redirected({specifications...}, void_body)` gives a command/function call
or ordinary C++ block temporary descriptor bindings. Results remain
ordinary caller-owned variables, as in the existing capture/reader APIs.
The callback introduces no source function, persistent redirect, or hidden
checkpoint. A future consumer sequences path evaluation according to its
IR; this runtime receives already-owned paths.

Specifications apply left to right: output then `2>&1` merges stderr into
the new output, while `2>&1` then output preserves stderr's earlier channel.
Nested scopes restore their immediate caller's bindings.

Input opens existing files. Output creates/truncates; append creates/appends;
read-write creates without truncation. Creation uses 0666 filtered by umask,
and temporary owned opens are close-on-exec. Installed source descriptors
inherit ordinary dup2 semantics, including self-duplication.

## Failure and restoration

All destination/source metadata, operation tags, and NUL-bearing paths
are checked before file effects. Every unique destination is snapshotted
before applying any redirect. Missing metadata is an explicit setup error.
Private opens cannot steal another source slot.

Normal completion and body unwinding restore descriptor identity, original
closed state, and fd flags. Partial setup runs no body and restores earlier
bindings; it does not undo truncation or completed writes. Snapshot allocation
failure occurs before effects. Ordinary completion reports `SetupError`;
failed unwind restoration retains the existing nonallocating fatal cleanup
policy. Restore attempts cover all snapshots and retain the first error.
Ordinary owned close is not blindly retried.

The AI used named owners for fallible pipe relocation and directly initialized
invoker snapshot members. On the used GCC toolchain, fallible aggregate
initialization leaked an earlier owner when a later operation threw,
leaving a stale intrusive ownership entry. Both partial-construction shapes
are now exercised through real descriptor exhaustion.

## Invoker channels

After installing metadata, `run` saves invoker stdin/stdout/stderr.
Runtime diagnostics and fatal cleanup use saved invoker stderr throughout
temporary redirections, following native DD#120's channel rule.
A closed original stderr never falls back to a temporary command file.

Invoker channels are restored before EXIT or ERR and again on EXIT -> ERR.
Ordinary failed channel initialization runs neither body nor handler;
failed invoker restoration prevents the selected callback.
DD#15 adds an allocation-free attempt at ERR on heap failure after source
metadata is ready, without losing the descriptor reservations.
Its checked EXIT/explicit ERR routing replaces the old END contract.

Persistent `exec` redirects/channel rebinding, public wait, forked-C++ or
background ownership, source admission, and IR consumption remain separate.
