title: "Implement target-aware native code generation and linking"
status: open
priority: P1
kind: work
depends: [native-cross-compilation-interface, aarch64-runtime-jump-support]
gates:
  - "The real x86_64 Linux sash CLI emits and links an architecture-verified static AArch64 Linux executable."
  - "The first generated target programs run under QEMU with asserted output and status before broader suite work."
  - "Unsupported targets and missing or incompatible tools/artifacts fail explicitly without falling back to host output."
  - "Existing x86_64 native/C behavior and ordinary gates remain intact."

# Target-aware native code generation and linking

Join the interface and runtime readiness tracks from the
[AArch64-first roadmap](cross-compile-and-setjmp-on-mac.md).
Implement the approved interface, not an invented flag/default/provider.

Current seams at `4a3286cb`: `compile_ssa` selects
`cranelift_native::builder` (`codegen/entry.rs:110-117`); `link_object`
selects host `ld` and the sibling default runtime (`lib.rs:1064-1090`).
Thread one target selection through the real CLI/backend/link path.
Enable both required Cranelift backends and use target-compatible ISA,
settings, relocations and calling conventions. Do not copy host CPU
features or assume the name `SystemV` means AMD64-only.

Select matching runtime and linker artifacts with useful failures for
missing/incompatible inputs. Preserve checked SSA, the embedded source/IR/
manifest, normal x86 behavior and `--sysroot` command resolution. The
compiler's regex validator remains built for the sash executable's own
architecture, distinct from the generated program target.

The existing [native temporary-object owner](bug-native-object-temp-file.md)
covers known faults in these same write/link paths. Coordinate directly
coupled error/cleanup changes there; do not route around them or claim this
target task closes its broader artifact-safety requirements without proof.

Drive the actual CLI to produce initial builtin/arithmetic programs, inspect
ELF machine/static-link properties, and execute them under QEMU immediately.
Pin outputs, full observed exit statuses and the selected runtime/linker.
Negative controls must detect host output or a missing/wrong target artifact,
not merely see that some executable ran.

This is the first generated-ARM-executable milestone. It does not complete
the [target spec gate](aarch64-qemu-spec-gate.md) or prove the AArch64-host
half of the matrix. Use existing gate/negative-control conventions; never
replace real-CLI acceptance with direct `compile_to_object` tests.
