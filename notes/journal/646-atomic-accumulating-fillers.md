title: Atomic accumulating array fillers
date: 2026-11-17

# Atomic accumulating array fillers

DG's integrated DD131/DD132 trial requires ordinary array fillers to append
complete batches and preserve their destination on error. The AI implemented
that prerequisite after scratch construction (`67e99309`). The work remains
on `glob-scratch-trial`; main remains `6c7b899f`, pending DG's evaluation.
Selectors and DD132's regex/proof/result work are not complete.

## One publication mechanism

`argv`, array `read`, `keys`, `match` and `matchall` now fill private tables
for ordinary destinations as well as scratch. `CollectionPublish` carries
an optional previous destination: absent means replacement, present means
append a completed batch. Scalar `readline`, assignment and map conversion
retain their replacement behavior. A reader's EOF or regex no-match verdict
comes from that invocation, not the retained array population.

Both backends call `sash_array_append_batch`. It clones the destination's
index and entry metadata, appends the batch into the private copy, then
replaces the original table header and frees the old metadata. All fallible
allocation and index work precedes publication. The table-header address
stays stable, so collection slots and live iteration retain the same
collection identity. Old key/value backing remains valid; the batch is not
consumed by this ABI.

This is logical publication without a fallible intermediate state, not a
hardware-atomic header write. Handler dispatch occurs at the compiler's
checkpoints. It does not roll back consumed input, argument effects or other
earlier work. An empty batch returns before computing an append index.

The existing sparse `+=` path used population as its next index: deleting
key 0 from a three-element array made the next append overwrite key 2.
The shared `sash_array_next_index` now chooses one past the greatest live
nonnegative key, or zero when none exists. Deleted and negative-only keys
do not inflate that value; overflow is checked. The final representable
index can be filled without an unnecessary subsequent increment.
This is the AI's reversible trial implementation choice to preserve sparse
contents, not an explicit new DG indexing decision.

## Coupled types and dataflow

The checker previously compared a whole array literal's String type against
an integer collection's element type. Numeric literal append/replacement
now uses the existing per-element check. Lowering shares declaration's
integer-cell shaping for append and plain assignment, using `CheckedTypes`
rather than inferring again. Without that shaping, an integer such as 4
became a string cell of length 1 and read back as 1. Nonnumeric cells still
receive SB4030; map conversion does not become a merge.

Accumulation also changes what must survive a fill:

- Source reader origins are unioned, except scalar `readline` replacement.
  Clean split/keys/regex data cannot resolve an earlier failed attest.
- Named summaries retain Incoming attest requirements. A helper appending
  its own argv cannot then attest the caller's retained tainted entries;
  that remains received taint and reports SB5088.
- SSA publication depends on both old destination and batch, retaining
  predecessor taint. Failed-path match no longer emits a clean retag.
- Terminal aggregate facts combine the populations and byte properties.
  ERR's separate NUL prover follows both sources too.

The named-summary repair reuses the handler's inherited-attest requirement
mechanism with body-specific wording and duplicate-location filtering.
It is not a second caller-context taint analysis.

## Genuine late-failure evidence

The new 31-case `spec/fillers.sh` covers ordinary and forwarded fills,
duplicates, independent snapshots, live iteration, EOF/no-match truth,
embedded NULs, sparse/negative/high indices, integer cell encoding, taint,
ownership and ERR-visible publication.

All 31 pass in native and C. The frozen pre-unit compiler and runtime fail
the same 30 assertions per mode; case 7 preserves auto-declaration.
These are failed assertions, not a claim that every original program
compiled successfully.

Freezing only the compiler would have been insufficient: native linking
names the checkout runtime archive, and `common.sh` sets C's runtime path.
A session-only argv-vector linker shim substituted the frozen archive;
native and C open traces confirmed the archive actually used. No repository
linker or harness change was needed.

Case 23 overflows on the second batch index, after a first private insertion.
The positive leaves ERR `old::1`; an in-place mutation exposes `old:first:2`.

The first case-24 OOM fixture was weaker than claimed: a 400-element literal
preallocated capacity 1024, so the 40960-byte failure occurred during the
initial clone. Removing the clone also changed the allocation budget and
could make the fault disappear. Neither result proved late rollback.

The corrected fixture incrementally constructs 400 entries, leaving capacity
512, then appends a 200-record batch under a 96K heap. The private insertions
reach entries 400-511 before growth requests 40960 bytes. The stronger
reversion retains an unused clone to preserve the allocation footprint but
mutates the visible destination in place. Both modes then print
`ready` followed by `old:new:512`; the positive prints `ready` followed by
`old::400`, with the same allocation failure. That is actual late-failure
publication evidence, not just an error-message match.

## Independent reversions

Every campaign was built and run serially through the real CLI in both
modes, then restored. Numbers below are `fillacc` IDs unless stated otherwise.

| Disabled mechanism | Actual red assertions |
|---|---|
| Transaction isolation, preserving the allocation footprint | 23, 24 |
| Sparse next-index calculation, replaced by population | 12, 13, 14, 15, 23, 25, 26, 29 |
| Native append publication | 1-6, 8-12, 14, 15, 20, 23-26; native only |
| C append publication | The same 18; C only |
| Private producer destination | 1-8, 10-12, 14, 15, 19, 20, 22-26 |
| Reader origin union | 21, 27 |
| Named-body Incoming attest requirements | 27 and `fillmig.8`, `.9`, `.11` |
| SSA prior-destination taint retention | 22, 31 |
| No clean retag after failed-path match | 31 |
| No failed-attest replacement resolution from clean fillers | 16, 17, 18 |
| Integer cell shaping at append and whole assignment | 13, 30 |
| Prior-destination terminal aggregate facts | `fillmig.14`; all 31 filler cases remain green |
| Prior-destination ERR NUL dataflow | 19 |

The SSA campaign removed both the explicit old-input contribution and the
publication's preserving-predecessor flag. It does not prove that removing
the explicit OR alone loses taint; version predecessors can preserve it too.
The integer campaign removed both new shaping call sites while retaining
the corrected checker and declaration shaping, independently pinning runtime
encoding rather than merely the old type rejection. The terminal and NUL
campaigns exercise separate analyses and have separate witnesses.

An insufficiently contextual restoration patch briefly changed the wrong
`let mut reads` initializer. The frozen source hash check caught it before
building; both sites were corrected with unique function context. No result
from that intermediate source was accepted. All mutations were restored to
the accepted positive source, compiler, runtime archive and local lockfile
snapshots before the gate. The later hook-driven refactor below is recorded
in separate final snapshots.

## Migrated contracts and acceptance

Twenty existing contracts move to fresh IDs; their old history remains:

| New ID | Retired ID | Accumulation consequence |
|---|---|---|
| `fillmig.1` | `av.rej6` | argv retains its existing prefix |
| `fillmig.2` | `63.cap1` | prefix precedes the complete capture batch |
| `fillmig.3` | `63.cap5` | no-match retains three entries |
| `fillmig.4` | `63.ma4` | matchall no-match retains three entries |
| `fillmig.5` | `63.mi4` | array population grows from three to five |
| `fillmig.6` | `39.61` | anchor miss retains the existing entry |
| `fillmig.7` | `39.73` | repeated NUL-bounded matches accumulate |
| `fillmig.8` | `sflow.1` | received-taint helper attest rejects |
| `fillmig.9` | `sflow.3` | forwarded received-taint attest rejects |
| `fillmig.10` | `sflow.19` | clean first element survives argv |
| `fillmig.11` | `sflow.15` | fifth-slot inherited attest rejects |
| `fillmig.12` | `adjresolve.14` | failed-path append is not replacement |
| `fillmig.13` | `adjresolve.15` | helper append cannot clean received taint |
| `fillmig.14` | `migratea.33` | ERR no-match preserves a nonempty safe array |
| `fillmig.15` | `p518.25` | reader retains old and appends new |
| `fillmig.16` | `p518.26` | empty reader retains population one |
| `fillmig.17` | `p518.27` | split yields `4:a,b,c,new` |
| `fillmig.18` | `75.137` | keys retains `x,y,z` before `only` |
| `fillmig.19` | `86.64` | retained prefix moves argv's `p` to index two |
| `fillmig.20` | `errnul.37` | ERR regex output retains `old` |

The first full gate recorded 11825 checks at rows 7063716-7075540.
Its four failures were the unmigrated `86.64` and `errnul.37` contracts in
both modes. Actual xtraces showed the required accumulation behavior.
Those contracts moved to fresh IDs rather than rewriting their history.
Case 31 separately pins SSA retention after failed-path match.

The first accepted gate is:

```text
NEW=6 RETIRED=4 FLIPS=0 TOTAL=11827
rows 7075541-7087367
native 6025 + C 5802
11814 passing + 13 existing skips
```

The sweep took 274 seconds. No failures, unexpected flips, ghosts or plan
mismatches remain; liveness is 249. Unit suites pass 239/27/20, and the
static gates pass. Against the last accepted scratch baseline, all 11725
retained statuses are unchanged, with exactly 102 added and 40 retired
mode IDs: net 62 additional checks.

The normal commit hook then rejected three touched dispatchers for nested
publication handling: `Facts::instruction`, `written_value` and
`collect_inst_var_types`. Flattening the two optional-source matches and
extracting C's publication-variable collection preserves the same operations
and leaves all three as flat dispatchers. No bypass or threshold change.
After that refactor the ordinary gate passes again:

```text
NEW=0 RETIRED=0 FLIPS=0 TOTAL=11827
rows 7087368-7099194
11814 passing + 13 existing skips
```

The final sweep took 273 seconds, with unchanged mode counts and statuses.
Final source/compiler identities are recorded separately from the pre-hook
positive; runtime and lockfiles remain unchanged. Publication uses ordinary
format, Clippy, totality and touched-function complexity hooks. No dependency
or DD change, new coverage measurement, or main-branch adoption belongs to
this unit.

Evidence remains in the session's `files/glob-trial-20261117/fillers/`:
frozen compiler/runtime pairs, actual-link controls, positive identities,
before/after cases, all thirteen retained campaigns and the full gate logs.
Earlier weak OOM controls remain distinguishable from the final late-fault
evidence rather than being presented as equivalent.

Private batches and cloned metadata establish correctness at a memory cost;
repeated-use allocation/time measurements still belong to the integrated
trial. No consuming-batch ABI or favorable performance claim is implied.
Next are the bounded directory selectors and source-provenance-aware list
sugar, followed by DD132 proof/results and useful-program evaluation.
The integrated owner remains open until that work and DG's review are done.
