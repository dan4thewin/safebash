title: Runtime unset backlog and live map iteration
date: 2026-11-22

# Runtime unset backlog and live map iteration

After the unset ICE repair, DG asked whether runtime keys were already
backlogged and how substantial the work would be. Neither completed unset
task owned that extension. The AI traced the current target decoder and
both deletion emitters: runtime deletion already accepts key bytes, but
the compiler currently reduces unset operands to literal target strings.

DG then requested the backlog item and a discussion of what happens when
unset changes a map during `for k in $(keys m)`.

## Queued extension

[Runtime-indexed unset](../tasks/runtime-indexed-unset.md) is P3 work:
keep the collection identity static and support an ordinary runtime
string key or integer index. Reuse the existing subscript discipline,
binding/type/taint checks and ordered evaluation; do not add runtime
collection-name lookup or a status to mutation. The internal target
representation remains an implementation choice.

This is not a compiler delivery. The motivating `unset choices[$pre]`
still receives SB5063. The previous repair remains a diagnostic repair,
and the changed refusal spec needs deliberate migration when the
extension is implemented.

## Live walk and snapshot are different

DD75 already calls `$(keys m)` a live walk and `keys m out` an independent
key snapshot. Ordinary snapshot destinations now accumulate; a fresh
destination or scratch binding supplies one snapshot for this purpose.
Copying keys does not also freeze their values.

Reading the actual loop found a deletion hazard. `lower_for_array`
increments a numeric cursor, while `sash_table_key` counts only entries
that are still live. Deleting a key at or behind that cursor changes the
ordinal of surviving keys.

A real CLI probe at `6e0414c6` confirms the problem in native and C:
with `alpha`, `beta`, `gamma`, deleting `alpha` in its own iteration
visits `alpha`, `gamma`. A subsequent traversal prints `beta`, `gamma`,
proving that the skipped key remains present. Neither run emits a
diagnostic; both exit 0.

The same probe supplies independent controls:

- With no deletion, all three keys are visited.
- Deleting future `beta` while at `alpha` visits `alpha`, `gamma`.
- Whole-map unset finishes the current body, then the live loop stops.
- Iterating a saved key array still visits all three keys after deleting
  from the original map.
- A saved key whose map entry was deleted still appears in that loop,
  but a fresh lookup yields the map's current empty value.

The AI recorded [the deletion skip](../tasks/bug-map-iteration-deletion-skip.md)
as a separate P2 bug. It already occurs with literal unset and must not
be confused with the runtime-key extension. Retaining the live/snapshot
distinction while avoiding skipped survivors is the repair recommendation,
not a newly selected complete mutation policy or an approved cursor design.

The probe used `spec/common.sh`'s actual native/C compiler and linker
paths, with explicit checkout binaries and bounded execution, but no
`setup`, `chk`, sweep or spec-history writes. Sources and outputs are
retained in session `files/unset-iteration-20261122/`; generated executables
and C intermediates were cleaned by the existing harness. Compiler,
runtime, local lockfiles and `_tmp/t2.sb` hashes remained unchanged.

Only backlog/journal/status records are changed. DD136 stays provisional
on `err-attempt-trial`; no implementation or main landing is part of
this request.
