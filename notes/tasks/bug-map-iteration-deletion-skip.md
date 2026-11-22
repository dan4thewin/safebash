title: Live map iteration skips surviving keys after unset
status: open
priority: P2
kind: bug
depends: []
gates:
  - "Deleting the current or an already visited key does not skip an unremoved, not-yet-visited key"
  - "Preserve DD75's live walk versus explicit key snapshot distinction; test future-key deletion and whole-map clearing"
  - "Native and C agree through the real CLI; ordinary nonmutating iteration and snapshot behavior are preserved"
  - "Original-binary and independent reversion controls prove the cursor repair, followed by the predicted ordinary gate"

## Reproducer and observed result

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
on the separately queued [runtime-key extension](runtime-indexed-unset.md).

## Cause and boundary

`transform/pipelines.rs::lower_for_array` increments an integer cursor
and compares it with the current collection length on each iteration.
`sash_table_key` interprets that integer as an ordinal among currently
live entries, skipping tombstones. `sash_table_delete` marks the entry
deleted and reduces `live_count`, which `sash_table_len` returns.

Deleting `alpha` moves `beta` to live ordinal 0. The loop advances to
ordinal 1 and selects `gamma`. This is shared lowering/runtime behavior,
not a native/C divergence.

[DD75](../decisions/DD75-canonical-keyword-forms-uniform-collections.md)
explicitly distinguishes the live `$(keys m)` walk from the independent
snapshot supplied by `keys m out`. Do not silently turn the live form
into a snapshot as a cursor workaround. Its full insertion, reinsertion
and replacement contract is not settled by this finding; raise any
consequential unresolved mutation choices before selecting a repair.
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
compiler/runtime/input hashes. No compiler source or runtime was changed.
