---
title: "macOS bring-up with shared POSIX ownership"
date: 2026-10-31
---

DG chose macOS bring-up next and supplied an existing Mac for validation.
The AI connected over SSH, created an isolated workspace, and implemented
the port without changing an existing checkout, SSH settings, or installed
dependencies. The real host is Apple Silicon, using Apple Clang 21 and
libc++; Linux Clang was not treated as evidence of macOS compatibility.

## Implementation

The former Linux implementation is now shared `platform_posix.cpp` plus
small Linux and Darwin adapters. Only capture storage, pipe creation, and
inherited-environment access differ. Child tracking, shared setup-error
publication, drain-before-wait, close-before-reap, and stdout restoration
remain one implementation. Public C++ interfaces are unchanged.

Linux retains memfd and pipe2. Darwin uses a private mkstemp file,
immediately unlinks it before the body can write, then marks it
close-on-exec. It creates owned pipes before setting each endpoint's
close-on-exec flag and obtains the environment through `_NSGetEnviron`.
[DD#4](../decisions/DD04-macos-posix-adapter.md) records the implementation
and its contract.

The earlier shared-memory option was removed: XNU's shared-memory file
operations cannot service ordinary read/write calls. Darwin's close
implementation was also inspected, not guessed from the manual's EINTR
listing: it releases ordinary descriptors before reporting file-close
errors, so the existing non-retry ownership rule applies.

Real-host validation exposed two example issues: BSD paste requires an
explicit `-` stdin operand, and false/true live under `/usr/bin`, not
`/bin`. Both C++ and sash companions now use the common spellings.
Their independent expected reports did not change.

The first build also found Darwin SDK function-like macros for signal-set
operations. Parenthesized global POSIX function calls preserve explicit
qualification without platform conditionals in shared example/test code.
This is API spelling repair, not a new signal lifecycle.

## Gates and controls

The prediction was 28 fixture groups (one added to the previous 27),
five C++ reports per build, five native sash reports, and the unchanged
169-case Linux catalog. Those totals were observed.

| Host/build | Result |
|---|---|
| Linux GCC and Clang | 28 groups, five C++ goldens, existing strace probes, 169 catalog cases each |
| Linux host-GCC ASan | 28 groups and five C++ goldens |
| macOS Apple Clang/libc++ | 28 groups and five C++ goldens |
| macOS Apple ASan | 28 groups and five C++ goldens; independent heap-overflow detection canary rejected |
| DG's supplied Linux native compiler/runtime | All five sash goldens |

The new storage fixture asserts an unlinked regular file during the
in-process body and private Darwin permissions. Invalid TMPDIR prevents
the Darwin body from executing, while Linux memfd capture still works;
raw external capture succeeds under that invalid directory on both hosts.
Existing groups retain binary/newline, nested-scope, closed-fd, partial
construction, streaming-handshake, allocation-failure, and child/fd
ownership assertions without blanket macOS skips.

Isolated Darwin adapter objects, not runtime hooks or privileged tracing,
provided four additional call-error controls: pipe reader/writer flag
failure, capture flag failure, and unlink failure. They retained operation
and errno, left stages unstarted, preserved the assignment target and
stdout, and restored descriptor snapshots with no unreaped children.

Two implementation mutations turned the intended existing checks red:
pretending unlink succeeded left `st_nlink` nonzero; pretending pipe
close-on-exec succeeded let the helper observe a private descriptor.
Two missing-injection controls failed for their expected reason, and
inactive-shim storage/descriptor controls passed afterward.
Canonical-source checksums stayed unchanged throughout.

Actual mk invocations rejected the Linux-only fault targets on Darwin.
Empty BUILD and unsupported-platform controls failed before creating
artifacts. The rebuilt Linux archive contains only runtime, shared POSIX,
and selected-adapter members, not the old full Linux implementation.

Session logs and probe sources retain the evidence; mutation executables,
objects, and the deliberately retained empty capture files are temporary.
[Automating the Darwin call-fault harness](../tasks/cpp-darwin-fault-harness.md)
is follow-up tooling, not a missing public runtime feature.

## Remaining scope

The [runtime roadmap](../tasks/cpp-transpiler-runtime-before-ir.md) stays
open. Call-context suppression, forked C++ stages, background/public wait,
general redirection, END/signals, and most value operations remain.
DG left Linux END/call-context and signal-response choices TBD; none was
inferred from form defaults or implemented here.

The native attestation policy question and Linux sanitizer/native-build
limitations are unchanged. The producer plan remains the protected local
uncommitted draft. There is still no IR reader or emitter.
