---
title: "The third pair: streaming records into parent state"
date: 2026-10-29
---

DG asked to proceed with the next pair of programs. The AI took the next
bounded step on the runtime critical path: one external producer feeding
a parent-side line reader. This adds useful streaming and ownership work
without settling multi-stage aggregation, whole-function suppression,
background jobs, or shutdown. No native compiler or producer changed.

## The pair

`stream_report.cpp` and `stream_report.sash` retain two real functions.
`collect_workers` consumes Ada, Grace, guest, and Edsger from an external
printf through `while readline`, applies the explicit letters-only gate,
and keeps capitalized worker names. `worker_label` runs an external printf
inside memfd capture during the loop.

The collection is the caller's: a C++ reference corresponds to sash's
`=workers` / `A` parameter slot. Mutations survive both the parent reader
loop and the function call. The saved workers occupy indices 1, 2, and 4,
so no missing-cell or negative-index choice is needed. The bounded counts
finish at four seen and three kept; a later for loop prints the retained
names. Both binaries match the independent nine-line `.expected` file,
as do the first two pairs against their unchanged goldens.

The sash program uses canonical `attest person [[ ... ]]`; the C++ program
spells its gate as an ordinary regex check and `CommandError`. This is not
a new taint checker. The capture remains newline-free, so the open
source-level capture-newline question is still avoided.

## Reader and child ownership

`read_from(path, argv, body)` is checked; `try_read_from` returns the same
normal-exit, signal, or setup alternatives as foreground execution. The
body receives a borrowed `LineReader &` and runs in the parent, with normal
references to its surrounding variables. A return ends that callback,
not the enclosing function.

The callback is a reversible implementation choice, like the existing
capture callback: it bounds the borrowed reader and permits explicit,
fallible completion outside a destructor. It is not a decision about
eventual emitted return/continue handling.

The line reader follows the DD#99/DD#122 reference rules. A newline ends
the record but is not part of its value; all other bytes, including NUL
and CR, survive. An empty terminated record succeeds, an unterminated
nonempty tail succeeds once, and EOF clears the destination. A read error
throws instead of becoming EOF and does not replace the target.

Foreground execution and this reader share command preparation, NUL
checks, private pipe creation, and one launch implementation. The child
connects its stdout, closes its unused endpoints, then execs; it still
does not allocate or unwind inherited ownership. Completion EOF and the
shared setup cell establish setup before the consumer starts, without
waiting for producer exit. Child setup failure skips the consumer.

Each reader owns its own data pipe and child. Neither fd 0 nor parent
stdout is rebound. An ordinary command in the loop sees program stdin;
nested readers and captures have separate streams. Private fds are
close-on-exec and stay above the standard slots, including initially
closed ones.

The child owner is declared before the pipe owner so reverse destruction
closes the pipe before waiting, including partial launch failure and
consumer unwinding. Explicit completion does the same. An early consumer
can cause SIGPIPE; the raw outcome retains the signal. Successful cleanup
preserves a consumer exception rather than repackaging it as a producer
failure. Already-consumed data is not rolled back when a producer exits
nonzero. No path adds force-kill or a runtime timeout.

## Evidence

GCC and Clang pass nineteen native fixture groups, including five new
reader groups, and all three C++ expected-output comparisons. Host-GCC
ASan passes that same gate. DG's supplied native compiler and companion
runtime compile all three sash counterparts, also matching the goldens.
The reference uses the real CLI without disabling compiler passes;
the standalone C++ gate does not require a Rust build.
A separate CLI probe also confirms empty lines, an unterminated final
record, and EOF target clearing in the supplied native compiler.

The new fixtures cover empty lines and input, binary records, CR,
unterminated tails, repeated EOF, and a 256 KiB line. A helper writes
`ready`, then blocks for an explicit consumer reply before writing `done`:
waiting or spooling to EOF before running the consumer would deadlock.
Other cases cover producer exits 7/127, failed exec, invalid argv, nested
readers and captures, original stdin, close-on-exec inheritance, initially
closed standard fds, exhausted descriptors, repeated completion, early
SIGPIPE, and preservation of a consumer's own `SetupError`.

Running the fixture gate with stdin initially closed exposed a test-setup
bug: its original-stdin pipe could already occupy fd 0, and the fixture
closed that endpoint after a no-op `dup2(0, 0)`. The uncorrected fixture
failed its stdin assertion. Keeping an already-correct fd 0 fixes the
fixture; all nineteen GCC groups then pass with stdin closed too.

The error catalog grows from 103 to 112: 53 basic cases, 45 injected
faults, and 14 EINTR recoveries. The nine additions cover successful
streaming, both pipe allocations, two partial-launch parent closes,
child stdout installation, data read, reader close, and interrupted read.
Both compilers pass the full catalog, and the existing focused foreground
fault probes still pass. The child-fault parser is shared with the
earlier first-close demonstration and still checks the actual child's
failure before exec, including resumed syscall lines.

Every ordinary injected line must now also match its baseline site
selector. This distinguishes the new 8192-byte data read from the
one-byte exec-completion read; an unchanged output alone would not prove
the new EINTR loop. Five negative controls (output-pipe allocation, data
read, partial-launch close, child stdout installation, and retry read)
reject uninjected runs. Their evidence is separate from the positive
traces. Formatting passes.

## Boundary

This completes a single-producer terminal parent-reader slice, not the
whole pipeline roadmap. Multiple external stages, forked C++ stages,
public wait and cached status, whole-function checkedness, general
redirects, END/signals, and most value operations remain open. macOS
capture storage remains the next platform choice. Existing sanitizer-host
limitations and the independent native rustix issue are unchanged.
The IR producer draft remains uncommitted; there is still no IR reader
or emitter.
