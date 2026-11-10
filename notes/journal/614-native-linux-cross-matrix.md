title: Both native Linux compiler hosts, both cross directions
date: 2026-11-10

## The choice

The output architectures and regex jump mechanism were already settled.
The remaining scope question was where the native compiler itself runs
and which cross-compilation directions it supports.

The AI recommended x86_64 Linux and AArch64 Linux compiler hosts, each
able to emit either Linux target. The narrower alternative was an
x86_64-only compiler host, still producing both output architectures.

DG first asked:

> if the user is responsible for providing the C compilers/linkers, what
> more is needed to cross compile?

The remaining work is chiefly shared target-selection plumbing and a
matching runtime, not four separate compiler implementations. Cranelift
emits the native machine code; a cross C compiler does not change its
selected ISA. The target runtime contains Rust as well as C/assembly.

DG then chose:

> yes to x86-64 and AArch64 and each to the other

DD129 now requires both Linux compiler hosts, same-architecture
compilation on each, and cross-compilation in both directions. Other
host operating systems and architectures are outside this commitment.
The separate C++ target route is unchanged.

## Concrete seams

Source inspection during the preceding discussion, at `762687bb`,
confirmed:

- `sash/src/codegen/entry.rs:110-119` selects the host ISA. The
  locked/offline Cargo feature tree shows the default `host-arch`
  feature, not explicit support for both required output backends.
- `sash/src/lib.rs:1030-1056` hardcodes `ld` and one runtime archive
  path. Both selections must follow the requested output target.
- `sash/build.rs:19-27` builds the compiler's regex validator for its
  own architecture, separately from the generated program's runtime.
- `sash/src/resolve.rs:431-458` already supports `--sysroot` command
  resolution. That does not select the machine-code architecture.

No cross build or target execution was performed. These observations
identify work; they do not establish that AArch64 or either cross
direction works today.

## Recording and ownership

`cross-compile-and-setjmp-on-mac` retains implementation and proof of
the full matrix, together with the selected regex jump support. A
compiler cross-build or an emitted object alone is insufficient:
completion needs the real CLI on both compiler architectures and
programs produced and executed for both output targets from each host.
The P1 private-rustix build repair remains independently open.

DD129, GUIDE, the existing repair/question records and status now agree.
No new flag spelling, target default, toolchain-distribution mechanism,
runtime packaging, migration sequence or C-link policy was selected.
User-provided toolchains were a discussion premise, not a separately
ratified distribution policy. The accepted runtime, language and failure
contracts remain unchanged.

This is recording/backlog work only. Implementation remains queued;
no product/spec/build source changed, and no new behavior probe, full
sweep or fresh coverage accompanies the recording.
