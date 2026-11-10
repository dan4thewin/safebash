title: "BUG: a fresh clone cannot build sash-rt: rustix 1.1.5 made `rustix::runtime` private"
status: open
priority: P1
kind: bug
depends: [q-runtime-syscall-layer]
gates:
  - "implement DD129: public rustix plus the shared Linux Rust syscall shim, with no private runtime-module dependency"
  - "a copy of sash-rt with no Cargo.lock builds: `git archive HEAD sash-rt`, then `cargo generate-lockfile` and `cargo build --release` exit 0"
  - "`mk test` and the full spec sweep pass on the result, in both backends"
  - "runtime comments and GUIDE describe the actual layer; remove the compiler's unused rustix dependency after confirming it remains unused"

DG settled the architecture on 2026-11-05 in DD129. This task remains
**open for implementation**; recording that choice did not repair the build.

## What

sash-rt calls three functions that rustix provides only in its `runtime`
module: `exit_group` (sash-rt/src/lib.rs:436), `kernel_fork` (653–658) and
`execve` (693). Both crates ask for `rustix = "1.1"` (sash-rt/Cargo.toml:15,
sash/Cargo.toml:14), and no Cargo.lock is tracked (`.gitignore` line 2). The
locks on this machine pin 1.1.4 (sash-rt/Cargo.lock:51, Cargo.lock:488), so
the build works here. A fresh clone resolves 1.1.5, and there the module is
private. rustix 1.1.5's src/lib.rs:308–323 says why:

> "runtime" is also a public API module, but it's only for libc-like users.
> People have been observed using it in the wild, so as a counter-measure,
> it now has a name mangled with a random string that will rotate
> periodically.

The module is now `pub mod runtime_448b8ad740e2a26f`, re-exported only as
`pub(crate) use … as runtime`.

Repro (measured 2026-10-25): `git archive HEAD sash-rt` into an empty
directory, then `cargo generate-lockfile --offline` (it picks 1.1.5) and
`cargo check --offline --release`. That gives E0603 "module `runtime` is
private" at lib.rs:436, 653 and 654, and cargo exits 101. So `mk build` on a
fresh clone cannot build the runtime, and nothing links.

The compiler crate asks for the `runtime` feature too but never uses the
module (no `rustix::runtime` under sash/src), so this error cannot arise
there. Measured 2026-10-26: with a fresh lockfile the root workspace
builds. sash/Cargo.toml:14 is the crate's only mention of rustix, so that
dependency is dead.

## Required fix (DD129)

Keep the working public rustix APIs. Move the unsupported fork, execve and
exit_group operations behind the project-owned Linux Rust syscall shim,
then drop the private `runtime` feature dependency. Preserve the existing
child initialization, descriptor ownership, errno and terminal-handler
contracts; this is a binding change, not a process-semantics redesign.

Do not use the deliberately mangled module name, freeze rustix at 1.1.4,
vendor nolibc, or introduce musl as the remedy. Tracking lockfiles remains a
separate reproducibility policy and would not repair the unsupported API
dependency.

The architecture applies to the runtime's other hand-written syscall sites
too. Account for each under public rustix or the shared shim; migration
staging remains planning work. DG confirmed **x86_64 and AArch64 Linux**
as the native output targets on 2026-11-10 (DD129, journal 612), so the
shared syscall boundary must account for both ABIs rather than silently
make x86-specific assumptions.

`cross-compile-and-setjmp-on-mac.md` owns end-to-end delivery of that
target set, compiler-host/cross-compilation planning, and the independent
regex setjmp and C-link-policy choices. A fresh x86_64 build alone does
not prove AArch64 runtime or cross-compilation support. macOS targeting
remains assigned to the separate C++ transpiler, not a native `sash-rt`
port; that assignment is not evidence of its current platform readiness.

`q-runtime-syscall-layer.md` preserves the research and resolution. A
successful local build using the old lockfile is still not this task's
acceptance gate: the fresh-lockfile build and full ordinary gates above
must pass before it can be marked fixed.

Found during the batch-531 port (step D1). Filed under DG's rule for the
port that anything not blocking the rest of the plan goes to the backlog.
