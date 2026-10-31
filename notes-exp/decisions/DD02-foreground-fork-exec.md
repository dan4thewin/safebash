---
title: "DD#2 - Single-threaded fork/exec and foreground outcomes"
status: accepted
---

DG selected single-threaded fork/exec for the next runtime milestone,
after comparing it with a hybrid using posix_spawn for externals and fork
for C++ stages.

One launch model suits the planned external, builtin, and C++ function
stages. It requires our own child-setup/error-channel code; the hybrid
would reduce external child setup but introduce a second path and would
not remove the threading constraints on C++ stages.

## Initial implementation

The next pair is a job report with ordinary functions, sparse records,
for/while loops, an ECMAScript filter, external stdout capture, conditional
command handling, and explicit result discard. It does not add pipelines
or whole-function suppression.

The AI's interface implementation keeps normal exit, signal termination,
and setup failure as distinct alternatives in an inspectable result.
Checked calls turn failure into the corresponding exception. Normal
exit codes are preserved; signal-to-128-plus-number conversion happens
when the outcome is checked for propagation. The raw result keeps the
signal number. An exec error is not an ordinary exit 127.

All strings and argument pointers are prepared before fork. Executables
are explicit paths: no PATH search, shell quoting, command-string
interpreter, or ENOEXEC shell fallback. The environment and non-private
descriptors are inherited. Until the signal lifecycle is implemented,
signal disposition and mask inheritance follow fork/exec itself.

The AI used a close-on-exec pipe for completion notification and a
preallocated anonymous shared cell for the setup errno. A lock-free store
publishes failure without a child write: an ordinary error-payload pipe
can otherwise turn a failed reporting write into apparent exit 127.
EOF marks exec or child exit; the parent inspects the cell before accepting
setup, and the child owner reaps before returning its final outcome.
The extra mapping is private implementation machinery, not a generated
program value or a new source-language rule.

The child uses the prebuilt arguments, then execve or _exit. It does not
unwind inherited parent objects.
Private descriptors avoid 0, 1, and 2 even when those slots start closed.
The foreground caller owns its child and reaps it on success or setup
failure; cleanup does not force-kill or impose a runtime timeout.

Background handles, public wait, replacing exec, forked C++ stages,
script-fd reservation, signal policy, and END remain separate work.

## Streaming-reader extension

The third pair reuses this launch path for one external producer, connecting
only the child's stdout to a close-on-exec data pipe. Shared setup status
is checked after completion EOF, without waiting for producer exit first.
The consumer runs in the parent with an explicit reader; its pipe closes
before the child owner waits, including during partial setup and unwinding.
This applies DD#122's reference ownership rule and introduces no pipeline
aggregation or whole-function suppression choice.

## Multiple-external-stage extension

The fourth pair uses the same launch path for several external stages,
adding child stdin wiring while keeping the terminal reader in the parent.
Known pre-exec failure is returned separately inside the launcher so a
pipeline can retain it as a stage outcome without mistaking a parent's
setup/close failure for that child's exit or signal.
All remaining parent endpoints close before any completion wait.
[DD#3](DD03-external-pipeline-outcomes.md) records the independently
selected aggregation policy; signal inheritance and no-force-kill remain
unchanged.
