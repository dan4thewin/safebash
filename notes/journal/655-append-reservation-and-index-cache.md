title: Removing append scans and destination cloning
date: 2026-11-17

# Removing append scans and destination cloning

DG asked why appends were inefficient after the integrated trial evaluation,
then whether allocation failure was the uncatchable heap-exhaustion case.
It is: DD130 permits terminal ERR cleanup, not resuming the append. DD131
still requires that cleanup to see the old logical destination. That
requirement does not demand a full destination copy.

DG authorized the proposed repair: "redress the inefficiencies as proposed".
The AI implemented maintained next-index metadata and complete reservation
before in-place publication. The old cloning strategy was the AI's initial
implementation choice, not a cost inherent in scratch or the language rule.
This unit starts at `6773fdf7` on `glob-scratch-trial`; main remains
`6c7b899f`. DD131/DD132 are still provisional.

## What changed

Previously every nonempty append scanned destination keys, and every
multi-cell append cloned its index and entry arrays before adding the batch.
Repeated small batches therefore repeatedly visited and copied a growing
destination. The single-cell exception avoided cloning, not the scan.

The table now maintains its next nonnegative integer index. Insertion
advances it; deleting the highest live key marks it dirty for a lazy rescan.
Deleting the last live cell or clearing the table resets it. Negative keys
do not increase it. Unsigned storage can represent one past `i64::MAX`, so
inserting that maximum key remains legal; a later nonempty append reports
overflow. Empty batches return before asking for an index.

The new field grows the shared C ABI header from 32 to 40 bytes. The runtime
size assertion and the compiler's shared native/C empty-header constant
agree. The generated C declaration also reflects that asking for the next
index may repair the cache. Old compilers and new runtime archives must not
be mixed: the old compiler reserves a smaller global header.

Append validates the **whole** final index range, then reserves all needed
index and entry capacity. Growth compacts/rehashes once when needed; entry
storage grows geometrically. The physical layout may change during
preparation, but the logical entries remain the old entries if a later
reservation fails. The header address stays stable for aliases and live
iteration.

Once preparation succeeds, publication allocates nothing and has no
per-entry overflow check. Private array batches already own eight-byte
integer-key buffers: rewrite those keys to the final indices, insert the
entries and relinquish each source key pointer. Batch destruction frees
only remaining keys and its own metadata/header. Values keep their borrowed
backing. The single-cell and multi-cell paths now share this mechanism.

This preserves logical all-or-nothing publication, not hardware-atomic
updates. Handler dispatch remains at the compiler's checkpoints. Allocation
failure remains fatal, and earlier input consumption or argument effects
are not rolled back.

The optimization does not make arbitrary mutation histories constant-time.
Repeated highest-key deletion followed by append still requires repeated
cache repair. Capacity growth still copies existing metadata occasionally;
retained values and independent snapshots still consume memory. No general
reclamation or regex cache was added.

## Focused evidence

`spec/fillers.sh` retains all 31 original cases and adds four:

| ID | Claim |
|---|---|
| `appendperf.1` | 1,000 two-cell appends fit 256 KiB and retain first/last cells and population |
| `appendperf.2` | Highest-key deletion, sparse insertion while dirty, maximum/negative keys, reset and deletion of every live cell preserve the next index |
| `appendperf.3` | A helper fill through A compacts/grows during live iteration without changing the header or an independent snapshot |
| `appendperf.4` | Failure reserving destination entries after a completed batch leaves ERR the old 400 cells |

The final 35 cases pass in both native and C. Against the matching frozen
pre-optimization compiler/runtime pair, exactly `.1` and `.4` fail in each
mode. `.1` exhausts the heap requesting 81,920 bytes instead of printing
`left:right:2000`. `.4` preserves the old population before and after, but
the old path fails on an earlier 4,096-byte allocation; the new assertion
pins the 40,960-byte destination reservation. It is not a claim that the
64 KiB program should succeed.

The final old-pair link log contains 52 substitutions: 26 direct native
links and 26 GCC/collect2 links, all selecting the frozen old archive.
An exported Bash `gcc` function was not the mechanism: `common.sh` invokes
GCC through `timeout`, so that function's intended log does not exist.
The external linker shim supplies the actual evidence for both modes.

Five independent source reversions were built and exercised in both modes.
Every run retained 35 distinct checks and matched the failing set:

| Reversion | Failing IDs per mode |
|---|---|
| Remove highest-key invalidation | `appendperf.2` |
| Remove last-live-cell cache reset | `appendperf.2` |
| Keep transferred keys owned by the source batch | `fillacc.24`, `appendperf.1`, `appendperf.4` |
| Reserve one entry at a time during publication | `fillacc.24`, `appendperf.4` |
| Check index overflow one entry at a time | `fillacc.23` |

The per-entry reservation control makes ERR print `old:new:512`, rather
than `old::400`, at the same 40,960-byte failure. The range-check control
makes ERR see `old:first:2` before overflow. These distinguish genuine
partial publication from an earlier failure that happens to leave the
destination unchanged. The ownership control also fails without aborting:
`.1` prints `:right:2000`, losing its first key.

One attempted reset control was vacuous: removing `sash_table_clear`'s
reset left all 35 native cases green. The whole-array `unset` fixture did
not pin that in-place reset. The final `.2` instead also deletes the two
remaining cells individually before reusing the array; the separate
last-live-cell reversion then fails. The unsuccessful clear control is
retained, not counted among the five proofs.

Two other draft mistakes are retained in the evidence. A helper originally
used illegal `A+=(...)`; the final helper uses legal clean scalar-NUL
`read` into A. An expected success at 96 KiB was not supported: the optimized
program still fails requesting 40,960 bytes. The original `fillacc.24` is
unchanged and the speculative extra case was dropped before any database
run. No fragmentation measurement or success at that budget is claimed.

All mutations are restored. Final source, compiler, runtime and both
lockfile identities match the saved positive; the final spec has its own
hash after the last-cell-deletion extension.

## Matched cost measurements

The existing evaluation driver ran three workloads serially against the
accepted pre-optimization trial and this optimized trial, in both backends.
These are not comparisons against original main. An output control precedes
five timing samples, with a 64 MiB configured heap. Reported medians include
process launch and `timeout`; small shared-host differences are noise.

| Workload | Iterations | Before native / C, ms | After native / C, ms |
|---|---:|---:|---:|
| One-cell append | 4,000 | 16.919 / 12.028 | 2.484 / 2.272 |
| Two-cell append | 1,000 | 24.222 / 22.388 | 1.840 / 1.718 |
| Two-cell append | 4,000 | 328.497 / 327.609 | 2.925 / 2.684 |
| Accumulating glob | 1,000 | 535.456 / 383.476 | 20.089 / 20.504 |

At the largest measured two-cell count this is about 112x native and 122x C.
The one-cell improvement comes from avoiding the scan, despite its old
no-clone exception.

Configured-heap searches use 4 KiB resolution. At 1,000 iterations, both
backends give the same fail/pass brackets:

| Workload | Before, KiB fail/pass | After, KiB fail/pass |
|---|---:|---:|
| One-cell append | 80 / 84 | 80 / 84 |
| Two-cell append | 320 / 324 | 160 / 164 |
| Accumulating glob | 9408 / 9412 | 7404 / 7408 |

These are sufficient configured budgets for these programs, not live or
peak-allocation measurements. The glob workload retains 32,000 entries at
this count. Its absolute fixture pathname differs from journal 654's run,
so only the matched pairs here support attribution to this optimization.
Journal 654's original measurements remain historical and unchanged.

## Gate and boundary

The predicted ordinary gate passed:

```text
NEW=8 RETIRED=0 FLIPS=0 TOTAL=12573
rowids 7258678-7271250
native 6398 + C 6175 = 12573
12560 passing + 13 existing C skips
0 failing, flips, retirements, ghosts or plan mismatches
liveness 245
```

All 12,565 statuses from the accepted preceding block
`7246113-7258677` remain present and unchanged. The eight first-ever IDs
are exactly `appendperf.1`-`.4` in both modes. `mk test` passes with
241 compiler, 27 converter and 20 expansion tests. Runtime and workspace
formatting and spec shell syntax pass.

An additional plain runtime Clippy invocation did **not** pass: 51
`not_unsafe_ptr_arg_deref` errors and 65 warnings. Every error is in one
of 25 whole functions byte-identical to HEAD, mechanically recorded in
`runtime-clippy-baseline-sites.json`. The normal repository hook lints
the three root-workspace crates, not this separate runtime workspace.
No lint suppression or unrelated repair is included.

Evidence is in session `files/append-atomic-20261117/`: frozen pairs,
`positive.sha256`, `final-spec.sha256`, final per-run TSVs/xtraces and link
records, the five reversion patches/results, build logs and
`evaluation/resource-01/results.json`. The original failed fixture and
control attempts are retained separately.

The GUIDE, DD131 implementation notice and integrated owner now describe
reservation and key transfer rather than cloning. The owner remains open:
unchanged sequential converted-mapfile programs still retain old entries.
DG has not selected a converter migration or adopted the trial. Neither
that work, scalar regex reclamation/caching nor fresh coverage measurement
is part of this append optimization.
