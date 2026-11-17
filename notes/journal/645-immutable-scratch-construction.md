title: Immutable scratch construction on the glob trial branch
date: 2026-11-17

# Immutable scratch construction

DG authorized the integrated DD131/DD132 trial and selected its source
rules: leading underscore alone requests scratch construction, an initializer
cannot read its own target, and only bare literal list words activate glob
sugar. The AI implemented the construction/lifetime prerequisite here.
The branch remains `glob-scratch-trial`; main remains at `6c7b899f`.
This is not adoption of the provisional package or completion of the trial.

The preceding const-validation unit is `e053b852`. This unit adds immutable
scratch construction and failure-safe scratch collection publication.
Ordinary accumulating fillers, glob/rglob, regex proofs/results and
caught-scratch read permissions remain subsequent work.

## Source identity and writes

The new `scratch.rs` uses central lexical identities and the source-name
table. Testing the internal name's underscore would incorrectly freeze
qualified ordinary locals. Bare `_` remains discard, and compiler temporaries
retain their normal allocation rules.

One source construction site belongs to each scratch binding. Plain and
typed initializers, reader destinations, conversion targets and actual
for-item binding create instances. The same site can execute repeatedly;
a different site cannot reuse the binding. SB5103 reports duplicate
construction and mutation, including append, element operations, unset
and mapped helper/slot writes. Read-only helpers remain legal.

SB5104 rejects initializer self-reads, including trusted reads, transitive
helper globals, used collection arguments, redirect targets and pipeline
producers. The analysis shares existing read and call discovery. Passing an
identity to an unused slot is not itself a value read; the broader DD100
identity-access footprint is not the right replacement for this analysis.

Source-origin IDs authorize construction writes in the lowered program.
Exact private-initialization versions are separately recorded, so fresh
function/capture lifetimes remain legal without exempting arbitrary writes
at a shared span. The SSA write check operates after collection arguments
have been mapped to actual identities.

This work exposed a coupled pre-existing traversal omission:
`collect_value_fn_operands` inspected direct commands and capture expressions
but missed nested statement commands. `keys source _keys` inside a for body
therefore falsely reported SB5050 for `source`; a conditional helper's
`keys _m` could also be missed by initializer-read analysis. Both now use
the canonical `walk_stmt_tree`, with no new private walker.

## Lifetime and publication

`LifetimeStart` resets the visible scalar or collection before initializer
evaluation, attached setup or fallible conversion. Scalar reset uses
meaningful zero. A collection reset clears its existing valid table;
it must not allocate a new empty table before discarding the old contents,
because that allocation could fail while the old instance remained visible.

Scratch split/read, match/matchall and map conversion fill private tables.
`CollectionPublish` installs the completed table pointer; publication itself
does not allocate. Existing argv/keys snapshots and collection-literal
construction already produce complete independent tables. Integer-element
arrays/maps remain pointer-backed in both backends, not integer scalar slots.
The runtime and its ABI are unchanged.

Both instructions participate in canonical write, taint, terminal and NUL
analysis. Fresh construction does not preserve the expired instance's
taint. This does not yet implement DD132's separate caught-attest regions.
Validation retags remain distinct from writes, using the preceding const
unit's shared path.

Skipped sites retain their instance, zero-trip loops do not bind, and loop
exit does not destroy storage. The actual for-item binding starts its
instance. Function/capture entry and self-tail entry preserve their existing
fresh-private rules. Failed external captures retain this attempt's partial
stdout; failed attestation retains its candidate. Neither is collection
publication or reconstruction.

## Failure evidence and fixture corrections

The final `spec/scratch.sh` has 51 cases in each backend. It covers the
source and alias guards, ordinary-local controls, fresh and skipped sites,
typed collections, snapshots, readers, captures, conversion and ERR state.
The frozen `e053b852` compiler fails the same 33 checks in each mode:

```text
1-8, 11-15, 17-22, 24, 32, 36, 39-41, 43-47, 49-51
```

These are failed assertions, not a claim that every old program compiled:
scalar unset already reported SB4030 rather than SB5103; scratch const in
a loop hit the ordinary const restriction; nested keys falsely reported
SB5050. The remaining 18 cases preserve existing behavior.

Two draft fault fixtures initially tested the wrong thing. A reader inside
`if` caught its own missing-file failure, so it could not establish ERR
state. The corrected ordinary checked reader prints the first value, then
ERR sees empty after the next open fails. A 128K map-conversion heap failed
while building the source. A bounded heap matrix found 160K reached `ready`,
then failed during map population: the old implementation exposed a partial
map, the repaired implementation exposed empty. At 192K and above both
completed with 512 entries. Literal ERR messages avoid allocating in the
observer.

The final failures also exercise partial NUL splitting and matchall
population, odd map conversion after a complete pair, scalar arithmetic
failure, missing slurp/reader input, and second pipeline setup failure.
The pipe fault checks the actual injected `pipe2`/EMFILE trace; it is not a
restart-sensitive clone count. Integer array/map initializers fail on their
second visit and ERR sees length zero rather than the previous element.

## Independent reversions

Each stub was built and run through the real CLI in both modes, then
restored before the next. The observed red sets are:

| Disabled mechanism | Failing scratch cases |
|---|---|
| Duplicate construction registration | 1, 2, 32 |
| Source non-construction mutation guard | 5, 51 |
| SSA mapped-write guard | 6, 7, 8, 17 |
| Helper-global read closure | 13, 14, 18, 21 |
| Slot-mediated initializer reads | 15 |
| Redirect/pipeline initializer-prefix checks | 19-22 |
| Canonical collection-operand traversal | 18, 36 |
| Native lifetime reset emission | 39, 40, 41, 43, 44, 49, 50, native only |
| C lifetime reset emission | The same seven, C only |
| Private collection fill staging | 40, 45, 46, 47 |

Two initial predictions omitted co-fired diagnostics. Without duplicate-site
rejection, the desugared arithmetic update in case 2 still rejects as an
SB5104 self-read, but loses the required SB5103. Without the source mutation
guard, scalar unset in case 5 falls through to its existing SB4030. Case 51
separately establishes refusal of first-use append without a construction.
These diagnostic changes are not acceptance claims.

Without private staging, case 40 visibly produces `1` then `err:1`: the
odd conversion has already inserted its complete pair before finding the
dangling cell. The three allocation-failure cases expose partial batches
too. This independently proves publication, not just reset timing.

The final restored Rust source, compiler, runtime archive and both local
lockfiles match the accepted positive identities byte-for-byte. The
earlier 50-case snapshot is preserved separately from the final 51-case
snapshot; the extra spec changes no product source.

## Acceptance and remaining trial

The actual commit hook initially found six touched complexity violations.
Small extractions separated input processing, bound-diagnostic reporting
and fill-target selection; no bypass was used. A misplaced dispatcher
extraction was caught and repaired before acceptance, as was its missing
diagnostic type qualification. The repaired implementation passed the normal
format, Clippy, totality and complexity gates.

The first predicted gate added 100 mode IDs:

```text
NEW=100 FLIPS=0 RETIRED=0 TOTAL=11763
rows 7040188-7051950
11750 passing + 13 existing skips
```

The final gate adds only case 51 in both modes:

```text
NEW=2 FLIPS=0 RETIRED=0 TOTAL=11765
rows 7051951-7063715
native 5994 + C 5771
11752 passing + 13 existing skips
```

Unit/static gates pass, with no failures, flips, retirements, ghosts or
plan mismatches. Liveness is 251; each sweep took 274 seconds.
The complete unit adds 102 mode IDs over the const baseline.
No fresh coverage measurement, runtime change or dependency change occurred.

Evidence is retained in the session's `files/glob-trial-20261117/scratch/`:
frozen compilers and identities, before/after logs, heap controls, ten
reversion pairs, actual hook reports, both full gates and source snapshots.
The trial owner and GUIDE record the architecture and remaining boundary.

Private batch construction has a memory cost; this unit establishes
publication correctness, not a favorable repeated-use allocation profile.
Measure that with ordinary accumulating fillers and the useful-program
trial. Keep the integrated owner open, retain every DD's provisional
status and bring the combined results to DG before changing main.
