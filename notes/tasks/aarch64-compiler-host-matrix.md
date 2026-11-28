title: "Complete the AArch64 Linux compiler-host and reverse-cross matrix"
status: open
priority: P2
kind: work
depends: [aarch64-qemu-spec-gate]
gates:
  - "An AArch64 Linux sash executable actually runs as the compiler and produces both AArch64 and x86_64 Linux outputs."
  - "Both outputs have the expected machine type and execute in their target environments with native/C contract evidence."
  - "All four DD129 host/target combinations have separately identified execution results before the coordinator closes."

# Complete the Linux compiler-host matrix

Follow the prioritized [x86_64 -> AArch64 delivery](cross-compile-and-setjmp-on-mac.md).
This task reuses the target gate's execution/accounting machinery; it is
not a prerequisite to the first-direction milestone.

Build and run sash on AArch64 Linux, under the established QEMU Linux
environment or actual AArch64 Linux hardware, and produce both supported
Linux targets. Cross-building the compiler on x86_64 is preparation, not
proof that its AArch64 executable works as a compiler.

The compiler's C regex validator follows the architecture of the sash
executable being built, not the architecture of the build-script process
or the selected program output. Keep target runtime archives/toolchains
distinct, and preserve both Cranelift output backends on both hosts.

Use the real AArch64 CLI, inspect each emitted machine type, and execute
AArch64 outputs in the target environment and reverse-cross x86_64 outputs
on an x86_64 Linux execution environment. Reuse the native/C target suite
and record build-host, compiler-host and output-target identities separately.
Include default/explicit target selection and wrong-artifact controls.

Close the coordinator only with evidence for x86_64 -> x86_64,
x86_64 -> AArch64, AArch64 -> AArch64 and AArch64 -> x86_64. First-direction
success must not become a permanent one-way restriction or a claim that
the full matrix has already passed.
