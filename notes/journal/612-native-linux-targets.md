title: Native Linux targets are x86-64 and AArch64
date: 2026-11-10

## The remaining scope choice

DD129 chose public rustix plus a project-owned Linux Rust syscall shim,
but explicitly left the native Linux architecture set open. Its earlier
recommendation assumed x86-64 and AArch64; an assumption was not an
end-to-end support commitment.

The AI recommended those two native targets, including Raspberry Pi
systems running 64-bit Linux, with x86-64 to AArch64 as the proposed first
cross-compilation direction. The narrower alternative was x86-64-only
initially, postponing the native ARM goal.

DG answered:

> yes to x86-64 and AArch64; briefly summarize how you know about the C++ path

## What that settles

The native Linux output targets are **x86_64 and AArch64**. The runtime
boundary must account for both ABIs and preserve the same language,
failure, ownership and handler contracts. 32-bit ARM and further
architectures are outside this native commitment.

This selects output targets, not every compiler-host/cross combination,
flag spelling, runtime/linker-selection mechanism or delivery order.
The native ARM cross-compilation goal remains. Actual AArch64 support
needs target-specific execution evidence before it can be reported done.

## How the C++ path is known

The basis was DG's own 2026-11-05 direction, quoted in DD129: macOS would
use a separately developed C++ transpiler, which could also cover exotic
targets. The reply stated that basis and the limit: the AI had not
inspected that project's current implementation in this discussion or
verified its platform support.

The C++ route is therefore a planned option, not evidence that 32-bit ARM
or any other platform already works. The existing `--emit-c` differential
backend remains a different component and is not retired. No experiment
implementation or records were changed by this native-target ruling.

## Current source, not a porting result

Narrow fresh-index inspection at `d9332af1` confirmed:

- `sash-rt/src/lib.rs:1650-1669` has an AArch64 `_start`.
- The same file's `sash_fcntl_dupfd_cloexec` at 213-229 still embeds x86
  register names and the `syscall` instruction without an architecture guard.
- `sash-rt/build.rs:24` unconditionally compiles the x86-only
  `csrc/setjmp.S`.
- `sash/src/codegen/entry.rs:112` selects the host ISA.

These are concrete remaining seams, not a complete porting inventory or
an AArch64 build/run measurement.

## Recording and ownership

DD129's target section now states the accepted set rather than leaving
it as an assumption. GUIDE and the resolved syscall question agree.
`cross-compile-and-setjmp-on-mac` retains end-to-end target delivery and
the remaining host/cross-compilation, regex-recovery, migration and
C-link-policy work. The P1 fresh-clone task keeps the immediate unsupported
rustix API repair, with both chosen ABIs part of the shared-layer contract.

Both implementation tasks remain open. No compiler/runtime/spec/build
source changed, no cross-target build or execution was run, and no full
sweep or fresh coverage accompanies this recording.
