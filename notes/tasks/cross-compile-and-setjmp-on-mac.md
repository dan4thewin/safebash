title: "Native Linux cross-compilation and regex error recovery"
status: open
priority: P2

# Native Linux cross-compilation and regex error recovery

## Scope settled by DD129

On 2026-11-05 DG assigned macOS to the separately developed C++ transpiler,
which can also cover exotic targets. A native macOS runtime/Mach-O port is
no longer the goal of this task. The filename is retained for existing links.

On 2026-11-10 DG confirmed **x86-64 and AArch64** as the native Linux
output targets, including Raspberry Pi systems running 64-bit Linux.
32-bit ARM and other architectures are outside that native commitment.
The separate C++ path is an option, not verified support or a new
implementation claim about that project.

Public rustix plus the shared Linux Rust syscall shim and the native
architecture set are now settled. This task owns their end-to-end target
delivery, not another vote on which native architectures to support.

DG subsequently chose **"jump support for both"** native targets
(DD129, journal 613). Retain the regex compiler's `setjmp`/`longjmp`
recovery and provide ABI-correct x86_64/AArch64 implementations, rather
than refactor the shared parser to ordinary error propagation. No
particular jump-code provider or new public ABI was selected; the
no-libc runtime and existing engine/dialect stay.

Compiler-host support, the cross-compilation seam, migration staging and
the existing C backend's link policy still require planning.
`bug-fresh-clone-rustix-runtime-private` owns the immediate build defect.

The inventory below was recorded on 2026-10-26, following DG's original
2026-09-16 macOS build report. Its line numbers and counts are historical;
re-measure the selected sites before implementing. Narrow source inspection
at `d9332af1` confirmed the remaining architecture seams: AArch64 `_start`
at `sash-rt/src/lib.rs:1650-1669`, ungated x86 registers in
`sash_fcntl_dupfd_cloexec` at 213-229, unconditional `setjmp.S` compilation
at `sash-rt/build.rs:24`, and host-ISA selection at
`sash/src/codegen/entry.rs:112`. No cross-target build or execution was run.

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
path, reached from `regcomp.c`. `q-runtime-syscall-layer.md` preserves
the historical alternatives. DD129 now selects jump support for both
native architectures; the ordinary-return rewrite was not selected.

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
complete cross-compilation interface. Native ARM cross-compilation remains
the goal; x86_64-Linux to AArch64-Linux was the proposed first direction,
not a completed capability or a decision on the full host/target matrix.
The existence of both `_start`s alone does not establish support.
The Mach-O-specific work in the table is historical research, not required
by DD129's target split.

## Remaining planning choices

1. **Compiler hosts and cross-compilation delivery.** The output targets
   are x86_64 and AArch64 Linux. Select the supported compiler hosts and
   cross combinations, target/runtime/linker selection and the delivery
   sequence. Do not equate supporting an output target with porting the
   compiler to every host, or remove the native ARM goal.
2. **Migration staging.** Land the three private-rustix replacements first,
   or migrate the remaining raw syscall sites in the same unit? DD129's
   public-API/shim policy applies to all of them either way; staging is not
   permission to leave an unaccounted third route.
3. **Existing C backend's link policy.** Should its link use `-nostdlib` so
   an accidental libc reference fails like native? Is `-lpthread` needed?
   This concerns the existing `--emit-c` differential backend, not the
   separately developed C++ transpiler.

The layer, target-set and regex-recovery choices do not settle those
remaining planning questions or authorize implementation.

## Required regex jump support

Select the appropriate implementation when building for each native
target. The jump buffer, saved registers/stack, C declarations and
exported symbols must agree with the actual ABI and compiler assumptions.
Keep recovery inside the live regex compiler invocation; this is not
general unwinding through Rust or a new EXIT/ERR transfer mechanism.

Prove ordinary and jumped returns on each target, including required
state preservation, partial-program cleanup and a later successful
compilation after a failed one. Preserve compile-time pattern diagnostics
and the shared engine's accepted language. An assembled object alone
cannot establish that recovery works; use target execution and negative
controls.

Coordinate reporting with `q-glob-semantics`, which owns the integrated
DD131/DD132 trial and its DD revision inventory. Invalid `r?` candidates
must return quiet false, while resource faults remain fatal. The runtime
currently prints in `regerror9` and aborts in its regex callers: retaining
the jump does not implement that reporting split or permit a blanket
terminal path for validation. `q-dynamic-regcomp-levers` records the
policy answer. Do not select a regex cache or change result kinds as a
side effect of the jump port.

Journal 613 records the selection and narrow current-source confirmation.
No implementation or cross-target execution accompanied the ruling.

## Gate

Any change here must keep `mk test` and the spec sweep green on x86_64
Linux. End-to-end native-target completion also needs the real CLI and
generated programs exercised for AArch64, with runtime identity and the
execution environment recorded; merely compiling an object or retaining
an `_start` arm is not evidence that it works. Preserve native/C behavior
on the supported targets, and distinguish implemented target coverage
from remaining delivery work in every handoff.

`sash-rt` is a **separate cargo workspace** — build it with
`(cd sash-rt && cargo build --release)`, not `cargo build -p sash-rt`.
