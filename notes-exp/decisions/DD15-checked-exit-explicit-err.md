---
title: "DD#15 - Checked EXIT and explicitly handled ERR"
status: accepted
---

DG directed the lifecycle migration after native DD128 was implemented at
`09a5bd497cae238b7412ea8357943e541b590c1d`. This adopts its terminal model,
not the native allocator or direct-dispatch implementation.
It supersedes [DD#5](DD05-planned-end-explicit-handling.md). END is removed;
its former body is not automatically equivalent to either new handler.

## Causes, status, and progression

| Cause | Selected handler |
|---|---|
| Normal fall-through, including a nonzero saved status | EXIT |
| Explicit full-integer `exit 0`, including argumentless exit | EXIT |
| Cooperative EXIT signal dispatched at a checkpoint | EXIT |
| Explicit full-integer nonzero exit | ERR |
| Uncaught operational failure | ERR |
| First fatal arithmetic/heap fault in main or EXIT | ERR |
| Caught/suppressed failure followed by continuation | Neither at that point |

Each handler is optional. Absence never substitutes the other handler.
The only progressions are main -> EXIT -> termination, main -> ERR ->
termination, and main -> EXIT -> ERR -> termination.

EXIT retains ordinary checking, including inside called functions and
operand/setup evaluation. Its uncaught failure replaces the incoming
status and enters ERR. Explicit zero exit in EXIT terminates; explicit
nonzero exit enters ERR. Any explicit exit in ERR terminates.
Successful cleanup preserves the handler's entry status.

Status remains `std::int64_t` through routing and callbacks. Only the
terminating process-status result is reduced to its low byte:
`exit 256` selects ERR with 256 although its parent sees zero.
An arithmetic/heap fault in ERR is reported and terminates without retry,
re-entry, an invented result, or a fresh cleanup allocation.

## C++ seam and actual storage

The existing borrowed-callback pattern extends directly:

```cpp
run(program, body, on_exit, on_error);
run(program, {source_fds...}, body, on_exit, on_error);
```

`nullptr` denotes an absent handler. Shorter forms omit trailing handlers;
the three-argument callback form means EXIT only, not an END alias.
Handlers return void and receive `const Shutdown &`. A body returning
`std::int64_t` supplies its fall-through status; an existing void body
completes with zero. `run` returns the final process byte for `main`.
Callback references and terminal entry allocate nothing.

Source globals use actual process-global backing storage and generated
handler functions, not a copied hypothetical final state. Default static
storage is ready before the body; source initializer expressions execute
inside the protected program entry. Completed writes remain visible;
an interrupted assignment publishes no result.

C++ exceptions unwind owned readers, pipelines, and captures before
dispatch. Consequently a handler cannot reuse the interrupted capture's
borrowed frame. There is no need to port native's linked capture stacks or
nonlocal dispatch. Invoker stdio is restored before each handler, including
a second restoration for EXIT -> ERR. External/raw fork children do not
run their parent's handlers. Future forked-C++/job ownership is still open.

## Bootstrap and descriptor invariants

Compiler-supplied source descriptors must remain reserved independently of
invoker snapshots. The synchronous driver borrows and validates its
initializer-list storage without allocation, then creates the snapshots.
Membership does not require sorting or a heap copy.

If heap failure interrupts snapshot/error construction after metadata is
ready, ERR is attempted with the still-active reservations and unchanged
invoker stdio. The fixture keeps allocation denied through entry before
explicitly recovering resources. Invalid host metadata is not admitted
source; its validation failure does not start a program or handler.

Ordinary channel-initialization failure still runs neither body nor handler;
failed invoker restoration prevents invoking a callback on knowingly wrong
channels. Broken cleanup/signal-handoff guarantees retain the existing
nonallocating fatal policy. ERR is a cleanup opportunity, not a promise to
survive unavailable resources or broken runtime invariants.

## Signals and consumer obligations

`checkpoint(bool deferred = false)` mirrors the compiler-owned dispatch
decision. A true flag defers EXIT dispatch, not signal receipt:
`signum()` still updates. Failure and explicit exit keep their own routes.
Checkpoints during either handler cannot redispatch.

The future consumer must protect acquisition through legal adjacent
attest/resolution, including called single-use returns, reader conditions,
false edges, function-return polls, and reader completion. Helpers add no
hidden polls or runtime guesses about taint. Signal-free generated code
must not acquire unconditional checkpoint clutter; the
[future regression task](../tasks/ir-signal-checkpoint-emission.md) covers
both conditional emission and protected placement.

Source function definitions are legal only directly in the program's
top-level statement list, never in either handler or another nested scope.
Generated C++ callbacks/lambdas are implementation machinery, not nested
source definitions.

ERR must explicitly handle actual operational failures, including called
bodies, operands, and setup; discarding a function's returned result does
not uncheck its body. Incoming bindings potentially tainted anywhere in
main/EXIT require `:!`; they cannot be blessed by a late attest or unrelated
overwrite. Genuinely fresh ERR input retains its adjacent caught-attest
opportunity. These are future front-end/consumer obligations, not runtime
taint checks or a temporary source validator.

An unhandled operational exception in a hand-written host ERR callback is
invalid under that source contract and escapes the C++ caller, rather than
receiving implicit catch-and-continue. Fatal arithmetic/heap faults remain
the explicit terminal exceptions.

Native schema 2 has optional `:exit-block` and `:err-block`, with an empty
handler distinct from absence, and no `:end-block`. This unit implements no
IR reader, emitter, or producer change. Persistent redirects remain next.
