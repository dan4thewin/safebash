title: DD#129 - Public rustix and a Linux Rust syscall shim
status: accepted

1. Keep the native runtime `no_std` and no-libc, using public rustix APIs and
   one project-owned Rust syscall boundary for uncovered Linux operations.
2. Confine architecture-specific syscall numbers, registers and raw-result
   decoding to that boundary; preserve existing process, fd and handler contracts.
3. Support x86_64 and AArch64 Linux compiler hosts and native targets in all
   four same-architecture/cross-compilation combinations.
4. Select target ISA, ABI, runtime and linker independently of the compiler
   host; command-resolution sysroots do not select machine-code targets.
5. Keep regex-local jump recovery with correct support for both Linux ABIs;
   do not unwind generally through Rust or replace terminal-handler dispatch.
6. The separately developed C++ transpiler owns the macOS route and may serve
   exotic targets; it is distinct from the existing C differential backend.

---

Decided by DG on 2026-11-05:

> yes to public rustix and syscall shim; record and present the next question

This settles the runtime's syscall-layer architecture. The layer and
fresh-clone build repair were implemented on 2026-11-11 (journal 619).
The full native host/cross matrix and AArch64 regex jump support remain
separately owned work.

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
program evidence, not just a successful compiler build. The syscall shim
now has implementations compiled to objects for both architectures.
The runtime's regex jump assembly is still x86-only, and native codegen
still selects the host ISA. No AArch64 execution is claimed by the
syscall-layer repair.

The existing `--emit-c` differential backend is not the separate C++
transpiler and is not retired by this decision.

## Native compiler hosts and cross-compilation

DG confirmed the compiler-host and cross-compilation scope on 2026-11-10
(journal 614):

> yes to x86-64 and AArch64 and each to the other

Support **x86_64 Linux and AArch64 Linux as compiler hosts**, each able
to produce either native Linux output target. The required matrix is:

| Compiler host | x86_64 Linux output | AArch64 Linux output |
|---|---|---|
| x86_64 Linux | same-architecture compilation | cross-compilation |
| AArch64 Linux | cross-compilation | same-architecture compilation |

This includes compiling directly on a 64-bit Linux Pi/ARM server and
cross-compiling in both directions. There is no x86-only compiler-host
restriction or one-way cross-compilation restriction. Other host operating
systems and architectures are outside this commitment; the C++ target
route does not by itself establish native compiler-host support.

The compiler must select the output ISA, compatible ABI/settings, runtime
archive and linker independently of its own architecture. The regex
validator linked into the compiler is built for the compiler's own
architecture; the runtime linked into generated programs is built for
their target. Existing `--sysroot` command resolution does not select
the machine-code target.

This is a support commitment, not delivered cross-compilation or AArch64
compiler-host support. Providing C compilers/linkers does not supply
Cranelift target selection or a matching Rust/C runtime archive. The
discussion considered user-provided toolchains, but this ruling selects
no toolchain-distribution or runtime-packaging mechanism, new flag
spelling, target default or delivery order. Those remain implementation
planning, not a reason to reopen the selected matrix.

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

The runtime previously used rustix's unsupported runtime interface for
`exit_group`, `kernel_fork` and `execve`. The fresh-resolution failure recorded on
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

The 2026-11-11 implementation puts the unsupported fork, execve and
exit_group calls in `sash-rt/src/linux_syscalls.rs`, together with the
existing raw fcntl/dup2/close/getcwd/signal operations and signal restorer.
Architecture-specific registers, syscall numbers and negative-result
decoding are confined there. Linux ABI constants come from
`linux-raw-sys`; errors use public rustix `Errno`. Native startup remains
in `lib.rs`.

The former directory open/getdents64 assembly instead uses public rustix
`open` and allocation-free `RawDir`. Raw-fd helpers stay in the shim:
they must accept possibly closed descriptors and return kernel errors
without fabricating a valid `BorrowedFd` or `OwnedFd`. Public rustix's
allocating getcwd does not fit the runtime's fixed-buffer contract.
Directory selection and its separately owned failure-policy changes
were not redesigned by this migration.

`notes/tasks/bug-fresh-clone-rustix-runtime-private.md` is complete.
An isolated before build still failed on fresh rustix 1.1.6; the fixed
runtime and full workspace built with fresh locks, and the documented
`mk -p 1 build` succeeded. Existing native/C specs passed against that
fresh build, followed by the ordinary unit/static/full-spec/history gate.
Both shim variants were assembled and their object architectures checked;
that is component evidence, not an AArch64 runtime or cross-compiler.
Journal 619 records the commands, counts and preservation controls.

DG separately chose tracked lockfiles and locked normal builds/tests on
2026-11-10 (journal 615). AGENTS.md defines that build policy, with
`notes/tasks/track-cargo-lockfiles.md` owning enforcement. It does not
replace this decision's public-API/shim repair: its compatibility control
must deliberately resolve fresh dependencies in an isolated source copy,
even once the ordinary build uses a checked-in lockfile.

`notes/tasks/cross-compile-and-setjmp-on-mac.md` owns delivery of the chosen
native host/target matrix and the selected regex jump support. The
cross-compilation interface and artifact/tool selection,
and whether the existing C backend's link command should forbid accidental
libc references remain separate planning work. All existing hand-written
runtime syscall sites were accounted for in the repair rather than left
for a later migration. This repair does not select the remaining
mechanisms or authorize the cross-compilation/regex implementation.

`notes/tasks/q-glob-semantics.md` retains DD131/DD132's integrated
separate-branch trial, including quiet invalid-candidate reporting and
the distinction from fatal faults. It must coordinate with the jump
support work; the accepted native recovery mechanism is not a
trial-only choice.

The research and resolution are in `notes/tasks/q-runtime-syscall-layer.md`;
journals 572, 612, 613 and 614 record the layer, output targets, regex
recovery and compiler-host/cross-compilation matrix, respectively;
journal 619 records the syscall-layer implementation.
