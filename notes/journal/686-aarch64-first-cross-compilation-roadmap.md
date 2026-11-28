title: "AArch64-first Linux cross-compilation roadmap"
status: complete

# AArch64-first Linux cross-compilation roadmap

2026-11-28. DG requested prioritizing ARM cross-compilation, organizing the
backlog toward it, and using QEMU once ARM executables exist. The direction
was confirmed as x86_64 Linux -> AArch64 Linux, then clarified explicitly:
"specifically linux on AArch64".

This is backlog organization, not implementation or adoption of the
unattended-session branch onto main. DD129's accepted four-way Linux matrix,
public-rustix/shared-syscall boundary, no-libc runtime and regex-local jump
choice remain intact. DG chose the priority; the AI mapped the existing
seams and split the delivery owners.

## Bounded source and harness findings

At `4a3286cb`, `compile_ssa` still selects the host ISA and `link_object`
still selects host `ld` plus the single sibling runtime archive. The runtime
build unconditionally includes x86 `setjmp.S`; its C compatibility header
includes the toolchain's `<setjmp.h>`, making declaration/layout/compiler
agreement part of the ABI port, not merely an assembly-selection change.
The compiler's separate regex-validator library must follow the compiler
executable's architecture, not its selected output.

The public syscall migration and shared startup repair are already complete.
Do not repeat the old eight-site repair or reopen the startup source bug.
Their component/x86 evidence does not establish full ARM execution, and the
old startup task's unmet AArch64 trace gate is explicitly carried forward.
Quiet invalid-regex reporting is also already delivered (journal 648);
the coordinator no longer calls it unfinished trial work.

The spec harness directly executes outputs, hardcodes `gcc`/one runtime for
C and records backend-only IDs in root `spec.db`. A target gate therefore
needs compilation, execution and result identity together. A fresh target
history also exposes the existing gate's history-dependent liveness check;
bootstrap must be honest rather than borrowing mislabeled x86 results.

PATH inventory found `qemu-system-aarch64`, `clang` and `readelf`, plus an
installed AArch64 Rust target. The user-mode QEMU and GNU cross-tool names
checked were absent from PATH. This was not an exhaustive toolchain search,
a target build or a booted guest. No package was installed.
[QEMU's user-mode reference](https://www.qemu.org/docs/master/user/main.html)
explicitly excludes guest ptrace support: its partial syscall trace is not
a replacement for the suite's guest fault-injection evidence.

## Delivery owners

The retained `cross-compile-and-setjmp-on-mac` slug is now the **P1
AArch64-first coordinator**, not a native macOS task.

1. **Native cross-compilation interface (P1 decision):** settle target/default,
   archive/linker selection and provision with DG; no architecture re-vote.
2. **AArch64 toolchain/QEMU (P1 work):** an independent ready root, proving
   reproducible target tools and an actual execution environment.
3. **AArch64 runtime/jumps (P1 work):** depends on that environment and the
   completed syscall repair. Execute ABI/regex component controls under QEMU.
4. **Native cross codegen/linking (P1 work):** joins interface and runtime;
   real x86_64 CLI output must be AArch64 ELF and execute under QEMU immediately.
5. **AArch64 QEMU spec gate (P1 work):** target native/C execution, inherited
   startup/trace proof, real process/fault paths and distinct result accounting.
6. **AArch64 compiler-host matrix (P2 work):** reuse that gate to prove
   AArch64-host same-target and reverse-cross output; only then close the matrix.

The pre-existing C-oracle libc/pthread link-policy question has a separate
P3 decision owner. It is not a new ARM prerequisite. Lockfile enforcement,
coverage and the separate C++ project likewise remain outside this path.
The known temporary-object/link failure owner is linked for coordinated
repairs when those functions are changed, not falsely marked complete.

## Recording boundary

DD129, current focus and the two older implementation handoffs now agree.
The six delivery owners and the separated C-policy owner remain open.
No public target spelling/default, jump provider, distribution mechanism or
concrete QEMU provisioning choice was selected. No compiler/runtime/spec/
build source, lockfile or generated status block was changed, and no target
execution or new spec sweep is claimed by this documentation-only work.
