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

DG then confirmed **"x86-64 and AArch64 and each to the other"**
(DD129, journal 614). Both Linux architectures are compiler hosts, and
each must emit both Linux targets: same-architecture compilation on each
host plus cross-compilation in both directions. This is no longer an
unanswered host/matrix choice, nor a claim of working support today.
Other host operating systems and architectures are outside this commitment.

The cross-compilation interface and artifact/tool selection and the
existing C backend's link policy still require planning.
No new CLI spelling, target default, toolchain-distribution mechanism or
delivery order was selected.

The 2026-11-11 repair of `bug-fresh-clone-rustix-runtime-private`
implemented the shared Linux syscall layer and migrated all existing
raw syscall sites (journal 619). Its syscall module assembled for both
architectures, and fresh x86_64 runtime/workspace builds and native/C
gates passed. That is not an AArch64 runtime or compiler-host execution
result. The unported regex jump assembly and target-selection plumbing
below still belong to this task; no cross C toolchain or emulator was added
by the build repair.

The inventory below was recorded on 2026-10-26, following DG's original
2026-09-16 macOS build report. Its line numbers and counts are historical;
re-measure the selected sites before implementing. Narrow source inspection
at `d9332af1`, before that repair, confirmed the architecture seams:
AArch64 `_start` at `sash-rt/src/lib.rs:1650-1669`, ungated x86 registers in
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
complete cross-compilation interface. The initial x86_64-to-AArch64
proposal does not limit the now-selected matrix: DD129 requires both
Linux compiler hosts and both cross directions. The existence of both
`_start`s alone does not establish support. The Mach-O-specific work in
the table is historical research, not required by DD129's target split.

## Required cross-target plumbing

Source inspection at `762687bb` confirmed the remaining selection seams:

- `sash/src/codegen/entry.rs:110-119` selects the host ISA through
  `cranelift_native::builder`; the locked/offline Cargo feature tree
  shows `cranelift-codegen` using its default `host-arch` feature.
  Enable both required backends and select the requested output target
  and compatible ABI/settings rather than copying the compiler host.
- `sash/src/lib.rs:1030-1056` hardcodes `ld` and the sibling
  `sash-rt/target/release/libsash_rt.a`. Select a target-appropriate
  linker and runtime archive. That archive contains Rust plus the C
  regex engine/jump assembly and must be built for the output target.
- `sash/build.rs:19-27` builds the compiler's regex validator separately.
  Keep that library built for the compiler's own architecture, distinct
  from the generated program's target runtime.
- `sash/src/resolve.rs:431-458` already honors `--sysroot` for external
  command resolution. Preserve target-filesystem resolution; this flag
  is not currently a machine-code target selector.

User-provided C compilers/linkers were the premise of the discussion,
not a selection of distribution policy. They do not replace Cranelift
target selection or the target runtime build. A target runtime may be
built separately or supplied prebuilt; no packaging choice was made.
The matrix calls for shared target-selection machinery, not a separate
implementation for each direction. No cross build or execution
accompanied this inspection.

## Remaining planning choices

1. **Cross-compilation interface and delivery.** The Linux host/target
   matrix is settled. Plan target/runtime/linker selection, artifact
   provision and the delivery sequence without narrowing that matrix.
   No particular flag, default target or packaging mechanism is selected.
2. **Existing C backend's link policy.** Should its link use `-nostdlib` so
   an accidental libc reference fails like native? Is `-lpthread` needed?
   This concerns the existing `--emit-c` differential backend, not the
   separately developed C++ transpiler.

The layer, host/target-matrix and regex-recovery choices, and the completed
build repair, do not settle those remaining mechanisms or authorize this
cross-compilation/regex implementation.

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
Linux. End-to-end matrix completion requires the real compiler CLI
executing on each compiler architecture and producing programs for both
targets from each host. Verify the emitted architecture and execute the
generated programs in the appropriate target environment, recording
compiler/runtime identities and the execution environment. Merely
cross-building the compiler, assembling an object or retaining an
`_start` arm does not prove the full matrix.

Preserve native/C behavior on both targets and the language/runtime
contracts in all four combinations. Distinguish implemented coverage
from remaining delivery work in every handoff. Journal 614 records the
matrix ruling, not completion of these gates.

`sash-rt` is a **separate cargo workspace** — build it with
`(cd sash-rt && cargo build --release)`, not `cargo build -p sash-rt`.
