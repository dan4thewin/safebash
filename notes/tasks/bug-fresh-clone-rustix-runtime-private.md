title: "BUG: a fresh clone cannot build sash-rt: rustix 1.1.5 made `rustix::runtime` private"
status: done
priority: P1
kind: bug
depends: [q-runtime-syscall-layer]
gates:
  - "implement DD129: public rustix plus the shared Linux Rust syscall shim, with no private runtime-module dependency"
  - "an isolated tracked-source copy of sash-rt, with only that copy's Cargo.lock removed, resolves fresh dependencies and then builds with cargo build --locked --release; the fresh-resolution control never modifies the working checkout's locks"
  - "`mk test` and the full spec sweep pass on the result, in both backends"
  - "runtime comments and GUIDE describe the actual layer; remove the compiler's unused rustix dependency after confirming it remains unused"

DG settled the architecture on 2026-11-05 in DD129. The AI implemented it
on 2026-11-11; journal 619 records the completed repair, not just the ruling.

## Completion (2026-11-11)

The isolated before control freshly resolved rustix **1.1.6** and failed
with five E0603 private-module references. The repaired tracked-source
snapshot built both its runtime and entire compiler workspace with fresh
locks and `--locked --release`; `mk -p 1 build` also succeeded.

All eight former hand-written syscall sites are now accounted for under
public rustix or the shared x86_64/AArch64 Linux shim. The private feature
and confirmed-unused compiler dependency are gone. The mk archive rule
also tracks the new module, other runtime Rust files, manifest and build
script; an old/new dry-run control proves a module-only edit now rebuilds.

Six existing spec files passed against the fresh compiler/runtime in both
backends: **2204 checks**. The ordinary predicted gate then passed
**11345 checks: 11332 passing, 13 existing skips**, with no new IDs, flips
or retirements; rowids **6879442-6890786**. Unit counts remain 238/27/20.
Both shim variants generated objects of the intended architecture.
No AArch64 runtime execution or full cross-compilation is claimed.

Fresh resolution stayed inside the copies. Normal manifest-driven lock
graph reconciliation in the working checkout retained every remaining
dependency version/checksum; the compiler's six now-unused packages were
removed from its local lock. Tracking locks and enforcing locked normal
commands remain with `track-cargo-lockfiles`, not this repair.

## Recorded failure (2026-10-25/26)

The following versions and source locations are the original measurements,
not a fresh resolution for the 2026-11-10 lockfile-policy recording.

sash-rt called three functions that rustix provided only in its `runtime`
module: `exit_group` (sash-rt/src/lib.rs:436), `kernel_fork` (653–658) and
`execve` (693). Both crates asked for `rustix = "1.1"`
(sash-rt/Cargo.toml:15, sash/Cargo.toml:14), and no Cargo.lock was tracked
(`.gitignore` line 2). The local locks pinned 1.1.4
(sash-rt/Cargo.lock:51, Cargo.lock:488), masking the failure. A fresh clone
resolved 1.1.5, where the module was private. rustix 1.1.5's
src/lib.rs:308–323 says why:

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
vendor nolibc, or introduce musl as the remedy. DG separately accepted
tracking both workspace lockfiles and using locked normal builds/tests
on 2026-11-10 (AGENTS.md, journal 615). `track-cargo-lockfiles.md` owns
that implementation; a locked build does not repair the unsupported API
dependency or close this task.

Preserve the fresh-resolution gate deliberately: after making an isolated
tracked-source copy, remove only that copy's runtime lockfile, generate a
fresh one and build with `--locked`. Once lockfiles are tracked, merely
extracting `git archive HEAD sash-rt` no longer creates a lock-free copy.
Do not delete or regenerate the working checkout's locks for this
compatibility control. It is an explicit isolated dependency refresh,
not an exception allowing ordinary builds to resolve dependencies silently.

The architecture applies to the runtime's other hand-written syscall sites
too. Each was accounted for under public rustix or the shared shim in
this repair. DG confirmed **x86_64 and AArch64 Linux**
as the native output targets on 2026-11-10 (DD129, journal 612), so the
shared syscall boundary must account for both ABIs rather than silently
make x86-specific assumptions.

`cross-compile-and-setjmp-on-mac.md` owns end-to-end delivery of that
target set and the selected x86_64/AArch64 regex jump support (DD129,
journal 613). DG also selected both Linux architectures as compiler
hosts, each producing either target (journal 614). The same owner retains
that matrix's delivery and the remaining cross-compilation interface/
artifact/tool selection and C-link-policy planning. The jump port and
host matrix are not new syscall-layer choices.

A fresh x86_64 build alone does not prove AArch64 runtime/compiler-host
support or either cross direction. macOS targeting remains assigned to
the separate C++ transpiler, not a native `sash-rt` port; that assignment
is not evidence of its current platform readiness.

`q-runtime-syscall-layer.md` preserves the research and resolution. A
successful local build using the old lockfile is still not this task's
acceptance gate: the fresh-lockfile build and full ordinary gates above
must pass before it can be marked fixed.

Found during the batch-531 port (step D1). Filed under DG's rule for the
port that anything not blocking the rest of the plan goes to the backlog.
