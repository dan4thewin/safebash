title: Live map iteration skips surviving keys after unset
status: done
priority: P2
kind: bug
depends: []
gates:
  - "Deleting the current or an already visited key does not skip an unremoved, not-yet-visited key"
  - "Apply DG's 2026-11-22 snapshot ruling: direct key loops visit each captured key once despite source-map insertion, deletion, reinsertion, clearing or replacement"
  - "Native and C agree through the real CLI; ordinary nonmutating iteration and snapshot behavior are preserved"
  - "Original-binary and independent reversion controls prove snapshot-at-entry, followed by the predicted ordinary gate"

## Selected repair, 2026-11-22

DG chose "use the snapshot, update dd75" after comparing mutation-aware
iteration with other languages and asking for the pragmatic, least-churn
solution. Direct `$(keys m)` loops take the same independent key snapshot
as the existing fill form. Every captured key is visited once; later
insertions do not extend the sequence, and deletion, reinsertion, clearing
or replacement cannot shorten or reorder it. Body lookups still read the
current map. This deliberately supersedes DD75's live-loop distinction,
rather than retaining it through a new mutation-aware cursor.

Implemented through the existing snapshot runtime and both backend paths.
No new table layout, stable-entry identifier or registered iterator lifetime
was added. Mutable sources allocate a fresh private snapshot on each loop
entry; immutable scratch maps retain equivalent non-copying traversal.

`iterdel.1`-`.6` fail on the original compiler and on a live-traversal
reversion in both backends. The independent SSA backstop also checks the
snapshot's exact reaching source version. The counter-allocation fixture
moved to immutable `_m` at fresh `loopint.17`, retaining its heap/workload
and proved by a real counter-format allocation injection.

The corrected 12931-check gate passes. [Journal 667](../journal/667-snapshot-key-loops-runtime-unset.md)
records all controls and the two first-gate findings. DD75, GUIDE and the
specification outline are aligned. Delivery is on `err-attempt-trial`;
DD136 remains provisional and main is unchanged.

## Historical reproducer and observed result

Found while answering DG's map-mutation question on 2026-11-22, at
`6e0414c6053c083b154c537ff78dc8a405f77d58`:

```sh
map m
m[alpha]=A
m[beta]=B
m[gamma]=C
for k in $(keys m) {
    echo $k
    if [[ $k == alpha ]] { unset m[alpha] }
}
echo survivors
for k in $(keys m) { echo $k }
```

Native and C both exit 0 without diagnostics and print:

```text
alpha
gamma
survivors
beta
gamma
```

`beta` was neither visited nor deleted. A nonmutating control visits
`alpha`, `beta`, `gamma`. This uses a literal target and does not depend
on the [runtime-key extension](runtime-indexed-unset.md).

## Original cause and repair boundary

`transform/pipelines.rs::lower_for_array` increments an integer cursor
and compares it with the current collection length on each iteration.
`sash_table_key` interprets that integer as an ordinal among currently
live entries, skipping tombstones. `sash_table_delete` marks the entry
deleted and reduces `live_count`, which `sash_table_len` returns.

Deleting `alpha` moves `beta` to live ordinal 0. The loop advances to
ordinal 1 and selects `gamma`. This is shared lowering/runtime behavior,
not a native/C divergence.

[DD75](../decisions/DD75-canonical-keyword-forms-uniform-collections.md)
originally distinguished the live `$(keys m)` walk from the independent
snapshot supplied by `keys m out`. The finding alone did not authorize
changing that distinction; DG subsequently selected the snapshot repair
above, including insertion, reinsertion and replacement behavior.
Inspect other consumers of the shared ordinal accessors rather than
changing their meaning globally to suit the loop.

Additional controls agree in both modes: deleting future `beta` while at
`alpha` visits `alpha`, `gamma`; whole-map unset finishes the current
body and ends the loop; iterating an untouched key snapshot still visits
all three saved keys after a source-map deletion. A lookup of deleted
`beta` through that snapshot returns the current map's empty string,
not its old value.

Evidence: session `files/unset-iteration-20261122/` retains `cases.sb`,
both modes' stdout, compiler/runtime stderr and exit codes, and the
compiler/runtime/input hashes. That initial investigation changed no
compiler source or runtime; the implementation evidence is recorded above.
