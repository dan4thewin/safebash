---
title: "BUG: a fresh clone cannot build sash-rt: rustix 1.1.5 made `rustix::runtime` private"
status: open
priority: P1
kind: bug
depends: [q-runtime-syscall-layer]
gates:
  - "DG rules on q-runtime-syscall-layer.md; the remedy and its reason are recorded in the DD that ruling writes"
  - "a copy of sash-rt with no Cargo.lock builds: `git archive HEAD sash-rt`, then `cargo generate-lockfile` and `cargo build --release` exit 0"
  - "`mk test` and the full spec sweep pass on the result, in both backends"
---

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

## Fix

Options:

- **(a) Pin `rustix = "=1.1.4"` in sash-rt/Cargo.toml.** One line. It works
  for as long as 1.1.4 is not yanked, and it holds sash-rt back from every
  later rustix fix.
- **(b) Track the Cargo.lock files.** Every dependency is then pinned, not
  only rustix. Builds become reproducible, but any `cargo update` reopens the
  break, and the policy covers the whole repo.
- **(c) sash-rt issues the three syscalls itself and drops the `runtime`
  feature.** On x86_64, fork is a `clone` with SIGCHLD. sash-rt already
  builds an assembly stub (csrc/setjmp.S, via build.rs), and Rust's
  `core::arch::asm!` would also do. This removes the dependency on an API
  that rustix says is not for us.

Using the mangled name is not an option: rustix rotates it on purpose.

**Superseded 2026-10-26.** Discussing these options with DG widened the
question to the runtime's whole syscall layer, with cross-compiling in
view. It is captured, unruled, in q-runtime-syscall-layer.md, and this bug
waits on it. In short: (a) is a dead end, since it freezes rustix and the
module stays unsupported; (b) is a separate reproducibility policy; (c)
became options (N) and (R) there. The earlier recommendation, (a) then
(c), is withdrawn.

Found during the batch-531 port (step D1). Filed under DG's rule for the
port that anything not blocking the rest of the plan goes to the backlog.
