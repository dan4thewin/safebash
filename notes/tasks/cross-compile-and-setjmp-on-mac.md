---
status: open
priority: P2
---

# Cross compilation, and `setjmp.S` will not build on mac

Two related gaps, filed together because the first is the visible
symptom of the second. Reported by DG, 2026-09-16: `sash-rt` fails to
build on macOS.

## 1. `sash-rt/csrc/setjmp.S` is unguarded x86_64 ELF assembly

43 lines of AT&T x86_64, fed unconditionally to `cc::Build` by
`sash-rt/build.rs:24`. It contains **no `#if`, no `__aarch64__`, no
`__APPLE__`** — zero guards (verified).

Two independent reasons it cannot build on a Mac:

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

## 2. There is no cross-compilation seam at all

`setjmp.S` is the first thing that breaks, but fixing it alone would not
produce a working cross compile:

| site | what it hardcodes |
|---|---|
| `codegen/entry.rs:130` | `cranelift_native::builder()` — the **host** ISA. No target triple is ever consulted. |
| `sash/src/` (41 sites) | `CallConv::SystemV`. Apple aarch64 wants `CallConv::AppleAarch64`. |
| `sash/src/lib.rs:1005` | `Command::new("ld")` with `-static` — GNU ld, static ELF. Mach-O has no static-binary equivalent. |
| `codegen/entry.rs` | `embed_sash_source` writes a `.sash_source` **ELF section**. |

So the compiler can only ever emit code for the machine it is running
on, and the output format is static ELF by construction. "sash on mac"
and "sash cross-compiling to mac" are different projects; "sash
cross-compiling from x86_64 Linux to aarch64 Linux" is the smaller and
more plausible first step, since the runtime already has both `_start`s.

## Questions for DG before any work starts

1. **What is the actual goal?** Three quite different asks hide here:
   (a) `sash-rt` merely *builds* on mac so the workspace is not broken
   for a Mac contributor; (b) `sash` *runs* on mac and produces Mach-O;
   (c) `sash` *cross-compiles* from Linux to another target.
   (a) is a day; (c) is a redesign of the codegen/link seam.
2. **Is a static Mach-O even in scope?** Apple does not support
   statically linked binaries (no crt1.o equivalent, `libSystem` must be
   dynamic). Byte-identical static-ELF output is a stated property of
   the project; Mach-O cannot have it.
3. **If only (a):** is `#[cfg]`-ing `setjmp.S` down to a build error
   with a clear message ("sash-rt supports x86_64 and aarch64 Linux")
   acceptable, or does mac need a real aarch64 implementation?

Filed rather than started: this is a design fork with a large range of
possible scopes, and picking one is DG's call.

## Gate

Any change here must keep `mk test` and the spec sweep green on x86_64
Linux, and `sash-rt` is a **separate cargo workspace** — build it with
`(cd sash-rt && cargo build --release)`, not `cargo build -p sash-rt`.
