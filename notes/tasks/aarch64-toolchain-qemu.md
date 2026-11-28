title: "Establish an AArch64 Linux cross toolchain and QEMU environment"
status: open
priority: P1
kind: work
depends: []
gates:
  - "A reproducible target Rust/C/assembler/linker environment produces an architecture-verified Linux executable."
  - "QEMU executes that target executable with asserted output and status, including a failing control."
  - "The target userspace and guest execution plan covers external exec and guest syscall/fault tests without host substitution."

# AArch64 Linux toolchain and QEMU readiness

Ready first alongside the [interface decision](native-cross-compilation-interface.md)
under the [AArch64-first roadmap](cross-compile-and-setjmp-on-mac.md).
Establish repeatable build/execution commands in task-owned output locations;
do not overwrite the working x86_64 compiler or runtime.

PATH inventory on 2026-11-28: this host is x86_64;
`qemu-system-aarch64`, `clang` and `readelf` are present.
`qemu-aarch64`, `qemu-aarch64-static`, `aarch64-linux-gnu-gcc`,
`aarch64-linux-gnu-ld` and `ld.lld` were not found on PATH. Rust lists
`aarch64-unknown-linux-gnu` and `x86_64-unknown-linux-gnu` as installed.
This is an inventory, not a complete toolchain search, a booted Linux guest
or proof that any installed tool works for this task.

Select the concrete toolchain, target headers and QEMU provisioning when
undertaking the work. Compare user-mode smoke execution with the existing
system emulator's Linux guest route. Obtain DG's decision for consequential
environment/resource choices; do not silently alter shared `binfmt_misc`,
mounts or privileged host configuration.

Prove readiness with a small independently built AArch64 Linux executable:
inspect its machine type and execute it under QEMU, pinning stdout and both
successful/nonzero statuses. Record exact tools, commands and guest inputs.
That proves the runner/toolchain, not sash. Execute sash runtime probes and
generated programs as soon as subsequent owners produce them.

The full target gate needs target external commands and reliable nested
fork/exec, paths, signals and limits. An interpreter-prefix setting is not
by itself a complete target execution environment. In particular,
[QEMU user-mode documentation](https://www.qemu.org/docs/master/user/main.html)
states that guest `ptrace` is not implemented; `QEMU_STRACE` is a partial
trace facility, not strace fault injection. Plan a Linux guest or another
demonstrably valid QEMU arrangement for those gates. A fast smoke route may
have a smaller scope, but must not masquerade as complete target conformance.

Keep all runs bounded and serialized. Document provisioning and teardown.
Close this owner only after the readiness gates actually execute.
