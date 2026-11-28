title: "Build and execute the AArch64 Linux runtime and regex jump support"
status: open
priority: P1
kind: work
depends: [aarch64-toolchain-qemu, bug-fresh-clone-rustix-runtime-private]
gates:
  - "The complete Rust/C runtime archive builds for AArch64 Linux without libc startup or mixed-architecture members."
  - "ABI-correct ordinary and jumped returns execute under QEMU, preserving required state and zero-to-one longjmp behavior."
  - "Regex failure cleanup and subsequent success execute under QEMU; ordinary x86_64 native/C gates remain passing."

# AArch64 Linux runtime and regex jumps

Part of the [AArch64-first roadmap](cross-compile-and-setjmp-on-mac.md).
The public-rustix/shared-syscall repair is already done (journal 619), as
is shared startup (journal 287). Reuse those implementations; assembling
both syscall variants did not prove the full AArch64 runtime.

`sash-rt/build.rs:24` still unconditionally builds the x86-only
`csrc/setjmp.S`. `csrc/lib9.h:55-56` includes the toolchain's `<setjmp.h>`.
Supply/select AArch64 jump support whose buffer size/alignment, saved
registers/stack, declarations, compiler assumptions and exported symbols
agree with that C build. Evaluate the provider when implementing; DD129
selects regex-local jumps, not a particular provider or libc startup.

Build the complete runtime in its separate Cargo workspace with matching
target C/assembler inputs and isolated target artifacts. Preserve the
working x86 archive. Inspect the linked result as well as archive members;
unresolved symbols or accidental host objects must not be hidden by a
different libc/link arrangement.

Use QEMU component executables to prove ordinary/jumped returns, required
callee state and stack restoration, `longjmp(..., 0)` returning one, invalid
regex cleanup and a later successful compilation. Preserve the implemented
DD132 quiet `r?` false result and DD130 fatal faults. Recovery stays inside
the live C regex invocation, not through live Rust frames or into EXIT/ERR.
These are ABI controls, not a replacement for real-CLI specs.

The [codegen/link owner](native-cross-codegen-linking.md) consumes this
archive; the [target spec gate](aarch64-qemu-spec-gate.md) then proves full
language behavior and the outstanding AArch64 startup/trace obligation.
Runtime work need not wait for the final public target-selector spelling.
