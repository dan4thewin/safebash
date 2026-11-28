title: "Settle the native cross-compilation interface and artifact contract"
status: open
priority: P1
kind: decision
depends: []
gates:
  - "DG-approved target/default and runtime/linker selection contracts are recorded without narrowing DD129."
  - "Compiler-host libraries, output-target runtime artifacts and command-resolution sysroots remain distinct."
  - "Unsupported targets and missing or incompatible target tools/artifacts have explicit failure behavior."

# Native cross-compilation interface and artifact contract

Ready first under the [AArch64-first roadmap](cross-compile-and-setjmp-on-mac.md).
DD129 already chooses both Linux hosts/targets and all four combinations;
DG now prioritizes x86_64 Linux -> AArch64 Linux. Do not reopen that scope.

Before implementing the public interface, present the consequential choices
together with tradeoffs and a recommendation:

- Target selector spelling, accepted target descriptions and the omitted-target
  default; preserve current x86_64 invocation behavior unless DG changes it.
- Runtime archive and linker selection, override precedence and compatibility
  checks. Distinguish a working build-checkout arrangement from installation or
  prebuilt distribution; user-provided tools were discussed, not yet selected
  as the distribution contract.
- The relationship to `--sysroot`, which already resolves external commands
  against a target filesystem but does not choose an ISA, runtime or emulator.
  Also define interaction with inspection/C output modes without inventing
  target execution for an artifact-only command.

Keep the library used for the compiler's own regex validation separate from
the target runtime. Selecting ARM output must not rebuild that host library
for ARM or silently reuse the x86 runtime/linker. Missing prerequisites must
not produce host output under a successful cross-compilation report.

This owner records the interface decision, not its implementation. The
[codegen/link owner](native-cross-codegen-linking.md) implements it. The
[toolchain/QEMU owner](aarch64-toolchain-qemu.md) can establish readiness
independently; it does not need a final sash flag spelling.
