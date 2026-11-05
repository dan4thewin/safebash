---
title: "Native Linux cross-compilation and regex error recovery"
status: open
priority: P2
---

# Native Linux cross-compilation and regex error recovery

## Scope settled by DD129 (2026-11-05)

DG assigned macOS to the separately developed C++ transpiler, which can
also cover exotic targets. A native macOS runtime/Mach-O port is no longer
the goal of this task. The filename is retained for existing links.

DG selected public rustix plus a shared Linux Rust syscall shim. That
settles the layer, not the complete native Linux architecture matrix,
cross-compilation seam, migration staging or regex error-recovery strategy.
`bug-fresh-clone-rustix-runtime-private` owns the immediate build defect.

The inventory below was recorded on 2026-10-26, following DG's original
2026-09-16 macOS build report. Its line numbers and counts are historical;
re-measure the selected sites before implementing.

## 1. `sash-rt/csrc/setjmp.S` is unguarded x86_64 ELF assembly

43 lines of AT&T x86_64, fed unconditionally to `cc::Build` by
`sash-rt/build.rs:24`. It contains **no `#if`, no `__aarch64__`, no
`__APPLE__`** — zero guards (verified).

The original Mac failure had two independent causes:

- **Apple Silicon is aarch64.** The file is `mov %rbx, (%rdi)` and
  friends; there is no aarch64 variant to select.
- **Mach-O mangles C symbols with a leading underscore.** The file
  exports `_setjmp`, `setjmp`, `longjmp`, `__longjmp_chk`. On Mach-O the
  C function `setjmp` is the symbol `_setjmp`, so the current
  `_setjmp:`/`setjmp:` pair aliases the wrong thing and `longjmp`
  resolves to nothing.

**This is an inconsistency inside `sash-rt`, not just a missing port.**
`sash-rt/src/lib.rs:1591` carries a complete
`#[cfg(target_arch = "aarch64")]` `_start`, written deliberately — the
comment at 1553 records a real bug where the x86_64 and aarch64 init
paths drifted. So `_start` treats aarch64 as a supported target, while
`setjmp.S` assumes x86_64. So do eight x86_64 syscall blocks in the
Rust half, which no `cfg` gates (found 2026-10-26; listed in
q-runtime-syscall-layer.md). Whichever way that is resolved, the runtime
should agree with itself.

Note the aarch64 `_start` uses `:lo12:` relocation syntax, which is
**ELF**. Mach-O spells the same thing `@PAGEOFF`. So aarch64 support
today means aarch64 *Linux*, not macOS.

Who needs it: `setjmp`/`longjmp` is the Plan 9 `regcomp` error-recovery
path (DD#63), reached from `regcomp.c`. q-runtime-syscall-layer.md
lists four ways to drop or replace it; (A), plain error returns, removes
`setjmp` entirely.

## 2. Native cross-compilation needs an explicit target seam

`setjmp.S` is the first thing that breaks, but fixing it alone would not
produce a working cross compile:

| site | what it hardcodes |
|---|---|
| `codegen/entry.rs:130` | `cranelift_native::builder()` — the **host** ISA. No target triple is ever consulted. |
| `sash/src/` (41 sites) | `CallConv::SystemV`. Apple aarch64 wants `CallConv::AppleAarch64`. |
| `sash/src/lib.rs:1005` | `Command::new("ld")` with `-static` — GNU ld, static ELF. Mach-O has no static-binary equivalent. |
| `codegen/entry.rs` | `embed_sash_source` writes a `.sash_source` **ELF section**. |

That inventory describes host-ISA emission and static ELF linking, not a
complete cross-compilation interface. Native x86_64-Linux to AArch64-Linux
compilation remains a possible target; the existence of both `_start`s
alone does not establish support. The Mach-O-specific work in the table
is historical research, not required by DD129's target split.

## Questions for DG before any work starts

1. **Native Linux targets and host support.** Which architectures require a
   native backend, and which may use the C++ transpiler? Is cross-compiling
   from x86_64 Linux to AArch64 Linux the first native cross target?
   Compiler-host support is separate from the generated program's target.
2. **Regex error recovery.** Replace `setjmp` with ordinary error returns,
   provide the required architecture-specific jump implementation, or make
   the runtime's regex error path terminate? The last choice interacts with
   `q-dynamic-regcomp-levers` and must not settle it by side effect.
   The earlier recommendation was ordinary returns; DG has not selected it.
3. **Migration staging.** Land the three private-rustix replacements first,
   or migrate the remaining raw syscall sites in the same unit? DD129's
   public-API/shim policy applies to all of them either way; staging is not
   permission to leave an unaccounted third route.
4. **Existing C backend's link policy.** Should its link use `-nostdlib` so
   an accidental libc reference fails like native? Is `-lpthread` needed?
   This concerns the existing `--emit-c` differential backend, not the
   separately developed C++ transpiler.

No choice above was made when DD129 was recorded, and no implementation
has started under this task.

## Gate

Any change here must keep `mk test` and the spec sweep green on x86_64
Linux, and `sash-rt` is a **separate cargo workspace** — build it with
`(cd sash-rt && cargo build --release)`, not `cargo build -p sash-rt`.
