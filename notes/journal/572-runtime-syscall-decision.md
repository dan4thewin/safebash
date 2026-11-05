# 572 - Public rustix and a Linux syscall shim

2026-11-05. DG chose the runtime syscall-layer architecture and asked me to
record it, then present the next question. This is a decision-only step,
not authorization to start the runtime implementation.

## What changed the recommendation

The earlier discussion retained rustix partly for native macOS portability
and preferred vendored Linux nolibc for the missing calls. DG clarified:

> mac os will be handled by a C++ transpiler being simultaneously developed.
> it could also be used for any exotic target.

That removes macOS from the native runtime's requirements and reduces the
value of nolibc's broad architecture coverage. I recommended retaining the
working public rustix bindings and owning the smaller Linux Rust syscall
boundary instead. DG selected it:

> yes to public rustix and syscall shim; record and present the next question

## Recorded decision

DD129 is accepted. Public rustix remains; unsupported Linux calls go
through a shared project-owned Rust shim. The private runtime module and
its mangled replacements are not supported dependencies. Neither an old
version pin, vendored nolibc nor a musl/startup migration is the remedy.

The policy applies across the runtime, including existing raw syscall
sites. It is not limited to `exit_group`, `kernel_fork` and `execve`, the
three uses behind the recorded fresh-resolution failure. The runtime
semantics, no-libc model and handler contracts remain unchanged.

The exact native Linux architecture matrix and migration staging were not
selected. Regex setjmp/error recovery and the existing C backend's link
policy also remain separate. The C++ target path does not retire the
existing `--emit-c` differential backend.

## Task boundaries

`q-runtime-syscall-layer` is closed as a design question. The P1
`bug-fresh-clone-rustix-runtime-private` remains open for implementation,
with its fresh-lockfile build and ordinary unit/spec gates. Its gate was
not satisfied merely by documenting the chosen remedy.

The existing cross-compilation task now records the Linux scope and owns
the remaining planning questions, rather than asking again whether native
Mach-O is required. Its filename is retained for links. The regex-policy
cross-reference, GUIDE and status agree with those boundaries.

No compiler, runtime, dependency, spec or build rule changed. No source
sweep or coverage work was started. The next question is B79, integer
function results becoming pipeline process statuses; it is still open.
