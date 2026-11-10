title: Cargo resolutions become reviewed build inputs
date: 2026-11-10

## The choice

The private-rustix repair already had a selected architecture in DD129,
but its task deliberately left lockfile tracking as a separate policy.
The October incident demonstrated why the distinction matters: local
locks selected a working old dependency while a fresh resolution exposed
an unsupported API. Those measurements were not rerun in this discussion.

The AI recommended tracking both workspace lockfiles and using `--locked`
for normal builds/tests. Dependency updates would become explicit reviewed
changes rather than an incidental effect of a fresh environment. The
alternative was to retain independent resolution and its compatibility
exposure, without maintaining reviewed locks.

DG chose:

> track both

This covers root `Cargo.lock` for `sash`, `sash-convert` and `sash-expand`,
and `sash-rt/Cargo.lock` for the separate runtime workspace. Normal builds
must not silently create or revise their dependency resolution. Deliberate
updates still need review and the applicable gates.

## Evidence and boundaries

At the clean `fae82b08` baseline, `git ls-files` reported neither lockfile;
`git check-ignore -v` identified `.gitignore:2:Cargo.lock` for both.
The manifests confirm two workspaces. The ignore rule dates to bootstrap
commit `ed264ee6`; its message explains the experiments, not that policy.
No new dependency resolution or build was needed to establish those facts.

The policy stabilizes Cargo dependencies, not all build inputs. It does
not select new dependency versions, automatic update tooling, offline
builds, vendoring, a Rust/C toolchain pin or byte-identical output.

DD129's removal of unsupported APIs remains mandatory and independently
owned. Tracking the old working resolution cannot stand in for that fix.
Its fresh-resolution control must discard only an isolated copy's runtime
lock, resolve dependencies there and then build locked. Once locks are
tracked, `git archive` alone no longer provides that lock-free control.

## Recording and ownership

As with the hook policies, this is contributor build policy in AGENTS.md,
not a new language DD. GUIDE and DD129 point to the same boundary.
The new P2 `track-cargo-lockfiles` owns tracking and ordinary-build
enforcement. The existing P1 keeps its API repair and fresh-resolution
proof; neither task gains an artificial delivery-order dependency.

This is policy/backlog recording only. Neither `.gitignore`, lockfile,
manifest nor build command changed; no source/spec edit, dependency refresh,
full sweep or fresh coverage accompanies it. Implementation remains queued.
