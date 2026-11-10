title: "Track both Cargo lockfiles and enforce locked normal builds"
status: open
priority: P2
kind: work
depends: []
gates:
  - "Cargo.lock and sash-rt/Cargo.lock are tracked build inputs for their respective workspaces"
  - "Normal build/test dependency-resolving Cargo invocations use --locked and leave both lockfiles unchanged"
  - "An isolated tracked checkout builds and passes the applicable ordinary gates using its committed resolutions"
  - "Isolated missing/stale-lock controls fail without regenerating the lock or continuing later successful phases; a valid-lock control passes"
  - "Focused before/reversion controls distinguish the new enforcement from unrelated compilation failures"
  - "The private-rustix repair retains its separately owned fresh-resolution acceptance control; an old locked resolution is not its remedy"
  - "AGENTS.md, GUIDE, journal and status distinguish delivered tracking/enforcement from dependency compatibility and toolchain reproducibility"

## Accepted policy

DG, 2026-11-10:

> track both

The proposal was to track root `Cargo.lock` and `sash-rt/Cargo.lock`,
use `--locked` for normal builds/tests, and make dependency updates
deliberate reviewed changes. The alternative was to retain untracked
locks and independent fresh resolution within the manifests' ranges.

AGENTS.md's Cargo dependency-resolution section is the contributor-policy
authority; journal 615 records the choice. This task owns implementation,
which has not started. It does not select dependency versions or an
automatic update service.

## Baseline

At `fae82b08`, neither lockfile is tracked; `.gitignore:2:Cargo.lock`
ignores both. Root `Cargo.toml` lists `sash`, `sash-convert` and
`sash-expand` and excludes the separate `sash-rt` workspace.
One root lockfile does not control that runtime workspace.

The ignore rule originated in bootstrap commit `ed264ee6`; its commit
message gives no lockfile-policy rationale. The private-rustix task
records the October failure in which a local 1.1.4 resolution masked
fresh-resolution failure at 1.1.5. That evidence was not rerun here.

## Implementation and proof

Adjust ignore rules for the two selected lockfiles, establish resolutions
that pass the applicable gates, and commit them as build inputs.
Do not opportunistically add other workspaces' generated files or bundle
unrelated dependency upgrades. Manifest changes that require a lock update
must include it; ordinary builds must refuse a missing/stale lock rather
than silently repairing it.

Apply `--locked` to the normal dependency-resolving Cargo build/test
entrypoints, including the phases reached through `mk build`, `mk test`
and their quality checks. Inventory the actual invocation paths rather
than changing only one command. Preserve failure propagation and the
separate runtime workspace. Formatting or other subcommands that neither
resolve dependencies nor accept the flag need no invented wrapper.

Use an isolated tracked checkout for missing/stale-lock and valid-lock
controls. Exercise each workspace independently and assert the actual
lock/refusal outcome, not merely a nonzero exit that an unrelated failure
could satisfy. Verify unchanged lock contents and that failed prerequisites
cannot be hidden by later successful phases. Demonstrate the controls fail
before enforcement and under its final-tree reversion.

Dependency refreshes are explicit operations followed by review and
validation. `--locked` allows downloading the selected dependencies; it
does not imply `--offline`, vendoring, a compiler/toolchain pin, or
bit-identical binaries.

## Independent DD129 repair

`bug-fresh-clone-rustix-runtime-private` remains the P1 owner of removing
unsupported rustix APIs. Recording or tracking a working old resolution
does not close it, and freezing 1.1.4 is not its remedy.

Its acceptance control deliberately removes the runtime lock only in an
isolated source copy, resolves fresh dependencies, then performs a locked
build. A tracked archive will now include the lock, so extracting one
alone is insufficient. Neither task may rewrite the working checkout's
locks merely to run that control.

Coordinate the two units without inventing a delivery-order dependency.
The existing hook snapshot-consistency policy also applies to lockfiles
consumed by a check: being generated does not exempt a reviewed build
input from staged/worktree agreement.
