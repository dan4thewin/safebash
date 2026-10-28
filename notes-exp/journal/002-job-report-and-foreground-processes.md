---
title: "The second pair: job reporting and foreground processes"
date: 2026-10-28
---

DG asked for the next pair of programs with more functionality. After
comparison with posix_spawn for externals plus fork for C++ stages, DG
selected single-threaded fork/exec. DD#2 records that decision. The AI
implemented the foreground slice and its fixtures; this does not change
the native language or compiler.

## A more capable pair

`job_report.cpp` and `job_report.sash` keep a real `worker_label` function,
sparse people at indices 2, 7, 11, and 20, a while loop, for traversal,
and a regex branch. The function runs `/usr/bin/printf`, and its real
stdout is captured in a memfd. `/bin/test` supplies a caught condition;
another test result is explicitly discarded, including nonzero results
for workers after Ada. Later work still runs.

The programs print two rounds of ready/queued worker labels, skip guest,
and finish with `complete`: eleven lines in the adjacent golden file.
Both native and C++ outputs match those bytes and each other. The
greetings pair also still matches. Native compilation uses DG's supplied
compiler and its companion runtime, without disabling any compiler pass.

The countdown stays in 2..0. The sample reads populated cells and captures
newline-free printf output, avoiding decisions about overflow, missing
cells, or newline trimming. A caught command is not a caught whole
function; call-context propagation remains open.

## Outcomes and ownership

`try_execute` returns a typed normal exit, signal, or setup error.
Conditions inspect success, explicit discard ignores the result, and
`execute`/`check` raise on failure. Raw signal identity is retained;
checked propagation explicitly encodes 128 plus the signal number.
Failed exec remains separate from a real exit 127.

Argument strings and pointers are prepared before fork; interior NUL is
rejected rather than truncated. There is no PATH search, shell quoting,
or ENOEXEC shell fallback. The child inherits the actual environment
and non-private descriptors, does not allocate or unwind, and uses
execve or _exit. A private close-on-exec completion pipe avoids slots
0..2 even when closed. The foreground caller reaps its child, including
error paths, without adding force-kill or runtime timeouts.

The first development implementation sent errno down that pipe. A
controlled ENOMEM on the reporting write exposed a bug: the parent saw
EOF and reported an ordinary exit 127, while the child's fallback printed
prematurely. The old object passed the normal exec-error probe and failed
the injected case. The revised implementation stores errno in a
preallocated shared cell with lock-free publication; the pipe carries
only completion by EOF. Reporting no longer needs a child write. The
same injected probe passes, and its trace verifies no private-channel
write. This is a private representation choice, not another language rule.

## Evidence and remaining work

GCC and Clang, using host libstdc++, pass fourteen fixture groups and
both C++ examples. Host-GCC ASan passes the same set. The additional cases
cover literal/empty arguments, binary external output over pipe capacity,
capture unwind without partial assignment, environment inheritance,
ordinary exit 127 versus failed exec, signal versus encoded exit,
permissions/ENOEXEC, closed standard descriptors, close-on-exec
ownership, fd exhaustion, repeated launches, and unavailable child status.

Linux fault probes first establish the observed clone/wait4 calls, then
pin injected EAGAIN/EINTR in their traces. The fork-failure assertion also
fails without injection. The partial-setup trace shows pipe2 obtaining
0 and 3, then F_DUPFD_CLOEXEC failing with EMFILE; both ends close and
stdin stays closed. Formatting passes. The command helper is deliberately
an ordinary unsanitized executable, while the runtime and driver receive
the sanitizer flags.

Existing full-UBSan and Clang-ASan host limitations remain in their
tooling task; no checks were weakened to erase them. The native fresh-build
rustix issue is untouched.

The first Linux launch/capture slice now works. macOS storage can be
chosen next rather than waiting for the whole Linux runtime. Pipelines,
parent readers, background/public wait, replacing exec, forked C++ stages,
whole-function checkedness, redirects, END/signals, and most value
operations remain on the runtime roadmap. The producer draft remains
uncommitted, and there is still no IR reader or emitter.
