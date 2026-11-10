title: DD#129 - Public rustix and a Linux Rust syscall shim
status: accepted

Decided by DG on 2026-11-05:

> yes to public rustix and syscall shim; record and present the next question

This settles the runtime's syscall-layer architecture. Implementation is
pending; it does not claim that the fresh-clone build defect is fixed.

## Decision

Keep `sash-rt` a `#![no_std]`, no-libc runtime:

- Use rustix's public APIs where they meet the runtime's requirements,
  including its existing allocation discipline.
- Put Linux calls unavailable through those APIs behind one project-owned
  Rust syscall shim. Architecture-specific syscall numbers, register
  conventions and raw-result handling belong at that boundary, not in
  independent copies throughout the runtime.
- Stop depending on `rustix::runtime`, including its deliberately mangled
  replacements. An exact old-version pin is not the architectural remedy.
- Do not add vendored nolibc or adopt musl/libc startup as the solution to
  this problem. Retain the existing native startup and no-libc model.

This is the policy for the whole runtime, not an exemption applying only to
the three calls that first exposed the dependency problem. Migration can be
staged, but existing hand-written syscall sites must be accounted for under
the same public-API/shim split.

The syscall-layer change preserves the language and runtime contracts:
forked-child initialization, descriptor ownership, errno reporting,
program replacement, process exit and DD#128 handler routing. Changing
which binding issues a syscall is not permission to change those semantics.

## Target split

DG clarified the portability boundary before choosing the layer:

> mac os will be handled by a C++ transpiler being simultaneously developed.
> it could also be used for any exotic target.

Native `sash-rt` therefore need not acquire macOS support to satisfy the
project's macOS target. The separately developed C++ transpiler owns that
path and is also an option for exotic targets.

DG confirmed the **native Linux output targets** on 2026-11-10
(journal 612):

> yes to x86-64 and AArch64

The native compiler/runtime therefore commits to **x86_64 and AArch64**,
including Raspberry Pi systems running 64-bit Linux. A 32-bit ARM system
is not an AArch64 target. Other architectures are outside this native
commitment; the C++ transpiler remains a possible route for them, not a
claim that its platform support has already been implemented or verified.

Both native targets must preserve the same language and runtime contracts.
The shared syscall boundary must account for both architectures' actual
ABIs; an x86-specific implementation cannot be treated as portable merely
because an AArch64 startup routine exists.

This is a target commitment, **not an announcement of working end-to-end
AArch64 support**. Completion requires target-specific runtime and emitted
program evidence, not just a successful compiler build. The current runtime
still has x86-only syscall and regex jump assembly, and native codegen
selects the host ISA.

Compiler-host support, the cross-compilation interface, target/runtime/linker
selection and delivery order remain separate planning work. The native ARM
cross-compilation goal remains; this ruling does not promise every
host/target combination or select new flag spellings. The existing
`--emit-c` differential backend is not the separate C++ transpiler and is
not retired by this decision.

## Native regex recovery

DG separately selected the regex compiler's recovery mechanism on
2026-11-10 (journal 613):

> jump support for both

Retain the Plan 9 engine's regex-local `setjmp`/`longjmp` recovery and
provide ABI-correct jump support for **x86_64 and AArch64 Linux**.
Rewriting the shared regex compiler to propagate ordinary error returns
was the alternative, not the selected approach. Keeping jumps retains
its existing error-control structure at the cost of per-architecture
recovery support.

The jump must return to a live recovery point inside regex compilation,
preserving cleanup and the failure result delivered to the caller.
Jump-buffer layout, saved registers/stack, C declarations and build
selection must agree for each target. This does not introduce general
nonlocal unwinding through Rust or replace DD128's terminal-handler
dispatch.

The choice retains the no-libc runtime model. It does not select a
particular implementation provider, adopt musl/libc startup, change the
regex engine/dialect, or choose a compiled-regex cache or new public ABI.

Recovery and reporting remain separate. Under DD132's provisional model,
an invalid `r?` candidate must return quiet false; resource faults remain
fatal under DD130. Keeping the jump does not authorize an unconditional
diagnostic or process exit for every regex compilation failure. The
current runtime's printing and caller-side abort behavior still need
the separately owned trial's changes.

This is an accepted implementation direction, not delivered AArch64
support or authorization to begin the implementation.

## Why this syscall layer

The runtime uses rustix's unsupported runtime interface for `exit_group`,
`kernel_fork` and `execve`. The fresh-resolution failure recorded on
2026-10-25 came from rustix 1.1.5 hiding that interface; local lockfiles
pinning 1.1.4 masked the failure.

The earlier recommendation was public rustix plus vendored Linux nolibc
for the missing calls. Its wide architecture coverage is less valuable
when the C++ path can cover exotic targets. A Rust shim avoids another
vendored C layer and C UAPI-header build dependency, at the cost of owning
the native architectures' ABI details.

Keeping rustix's working public bindings is about reusing those bindings,
not retaining its macOS backend for native sash. A nolibc-only runtime is
no longer excluded by macOS, but migrating all the working bindings is a
larger change. Adopting musl would additionally change startup and the
no-libc model. DG chose the public-rustix/Rust-shim boundary instead.

## Implementation and remaining questions

`notes/tasks/bug-fresh-clone-rustix-runtime-private.md` remains open for the
repair and its fresh-lockfile, unit and dual-mode spec gates. Runtime
comments and GUIDE must describe the actual layer after it is implemented;
this recording alone changes no dependency, syscall, binary or build rule.

`notes/tasks/cross-compile-and-setjmp-on-mac.md` owns delivery of the chosen
native target set and the selected regex jump support. Compiler-host
support and cross-compilation, migration staging, and whether the existing
C backend's link command should forbid accidental libc references remain
separate planning questions. The syscall-layer, target-set and regex
recovery choices do not settle those remaining mechanisms.

`notes/tasks/q-glob-semantics.md` retains DD131/DD132's integrated
separate-branch trial, including quiet invalid-candidate reporting and
the distinction from fatal faults. It must coordinate with the jump
support work; the accepted native recovery mechanism is not a
trial-only choice.

The research and resolution are in `notes/tasks/q-runtime-syscall-layer.md`;
journals 572, 612 and 613 record the layer, target-set and regex recovery
choices, respectively.
