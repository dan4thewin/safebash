title: Fresh-clone builds and the public Linux syscall boundary
date: 2026-11-11

## Taking up the repair

DG asked:

> let's address the issue preventing fresh clone builds

DD129 already settled the architecture: public rustix APIs where they fit,
plus one project-owned Linux Rust syscall shim. The AI implemented that
choice, without reopening the no-libc model or taking on the separate
native host/cross-compilation and regex-jump delivery.

The isolated original-source control resolved rustix 1.1.6,
linux-raw-sys 0.12.1 and cc 1.11.1. `cargo build --locked --release`
exited 101 with five E0603 errors: exit_group, kernel_fork, its two Fork
variants and execve all named the private runtime module. The older
1.1.5 finding was still live under fresh resolution.

## One boundary, not three replacement calls

`sash-rt/src/linux_syscalls.rs` now owns the Linux ABI details:

| Operations | Binding |
|---|---|
| fork, execve, exit_group | Shared Rust syscall shim |
| fcntl duplication, dup2, close | Shim; preserve possibly closed raw-fd inputs and kernel errors |
| getcwd | Shim; preserve the caller-owned fixed buffer |
| signal action and signal return | Shim; architecture-specific entry/return instructions and kernel layout |
| Directory open and enumeration | Public rustix open and allocation-free RawDir |

This accounts for all eight earlier hand-written syscall sites as well
as the private APIs. Only native startup assembly stays in `lib.rs`.
Linux constants come from the already-resolved linux-raw-sys family;
negative results become public rustix Errno values at the boundary.
The x86_64 and AArch64 implementations use their own registers and syscall
numbers. Fork preserves the original clone flags and architecture-specific
child-tid argument position. AArch64 dup2 uses dup3, with a same-fd validity
check rather than dup3's rejection of that case.

The raw-fd choice is deliberate: public dup2 requires an owned destination,
and public close does not provide the required possibly-closed/error
contract. Inventing Rust descriptor ownership would misrepresent the
runtime's inputs. Directory opening/iteration, by contrast, fit the public
API and its fixed-buffer allocation discipline.

Caller initialization, reader cleanup, errno storage, exec diagnostics and
DD128 terminal routing stay intact. Directory matching and its existing
failure behavior are unchanged; the selector/failure work keeps its owner.
The runtime's inaccurate pure-Rust/all-rustix header is corrected, and the
compiler's confirmed-unused rustix dependency is removed.

Adding a module also exposed a coupled build-graph problem: the archive
rule named only lib.rs and C inputs. It now includes runtime Rust modules,
Cargo.toml and build.rs. With only the syscall module newer than the
archive, the old mkfile dry run skipped rebuilding; the new one scheduled
the runtime build. This control needed no source stub.

## Acceptance evidence

The repaired source snapshot was archived from the Git index, excluding
the existing README/licensing drafts. Its five implementation/build paths
were byte-compared with the final worktree. Fresh locks were generated
inside that copy, not copied from the working checkout.

- Runtime: rustix 1.1.6, linux-raw-sys 0.12.1, cc 1.11.1;
  `cargo build --manifest-path sash-rt/Cargo.toml --locked --release` passed.
- Root: fresh resolution included Cranelift 0.130.2;
  `cargo build --locked --workspace --release` and `mk -p 1 build` passed.
- The fresh compiler and runtime passed glob, errors, exec, signals,
  pipe_functions and termination in both modes: **2204 passing checks**,
  with exact plans and unique mode IDs. All runs were serialized and used
  the copy's compiler, runtime archive and prepended helper PATH.
- A no_std component probe included the actual syscall module and forced
  code generation for every API. Both x86_64 and AArch64 objects assembled,
  and readelf verified their machines. Both variants passed Clippy with
  warnings denied; only missing safety docs on the private probe's public
  wrappers were exempted. The missing AArch64 Rust target was installed
  after its E0463 failure; no cross C toolchain or emulator was installed.
- The ordinary gate passed with `NEW=0 FLIPS=0 RETIRED=0 TOTAL=11345
  timeout 900 mk -p 1 gate`. Units: **238/27/20**. Sweep:
  **11332 passing, 13 existing skips**, native 5784 / C 5561, rowids
  **6879442-6890786**, 253 seconds. Counts, plans, ghosts, history and
  liveness passed; liveness found 251 changed first-ever statuses.
  Both workspaces' formatting checks passed.

The toolchain was rustc/cargo 1.98.1. Runtime builds report five warnings
about the unchanged byte-pointer signatures of the C-ABI strlen/memory
stubs; these pointer types are ABI-compatible, not new syscall warnings.
No permanent behavioral specs were added: the regression is the fresh
build failure, with the original-source failure as its negative control.

Normal manifest-driven lock reconciliation removed six unused compiler
packages and changed the runtime's dependency edges. Every retained package
version and checksum stayed fixed. The fresh-resolution control did not
refresh either working lock, and the ordinary gate left both unchanged.
This is not the separately queued lockfile-tracking/locked-command rollout.

Private evidence is archived in
`files/fresh-clone-runtime-20261111-probes.tar.gz`: 71 byte-verified files,
including source snapshots, locks, build/ABI logs and objects, all twelve
fresh-run xtraces and the accepted gate's rows and artifact identities.
Archive SHA256:
`c1a505439f171ccf3f95a29af08df484540c3baa1bb6deae21cf972ec9080e86`.

## Boundaries

The P1 fresh-clone task is complete. DD129, GUIDE and the architecture
question/cross-compilation records now distinguish delivered syscall work
from the remaining full matrix. An assembled AArch64 shim is **not**
an executed AArch64 runtime, compiler host or cross-compiler. The x86-only
regex jump assembly and target-aware codegen/runtime/linker selection remain.

The short README's obsolete build warning was removed within its existing
uncommitted draft. README/licensing changes remain outside this repair's
commit. No new coverage measurement or unrelated backlog implementation
was undertaken.
