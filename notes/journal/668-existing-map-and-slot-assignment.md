title: Existing-map conversion and collection-slot assignment
date: 2026-11-22

# Existing-map conversion and collection-slot assignment

DG's global-map example reached SB5025 because array-to-map conversion
worked in a declaration but not in assignment to an existing map. DG
selected existing-map assignment rather than new global-declaration syntax,
then confirmed well-typed whole-collection assignment through collection
slots generally. He chose complete-conversion publication: failure preserves
an existing map; failed construction leaves a new map empty.

The AI implemented those choices on `err-attempt-trial`. Direct map output
from `match`/`matchall` was explicitly set aside. This work neither adopts
DD136 on main nor adds a new tour feature.

## Contract

`map m=@pairs:DISP` and `m=@pairs:DISP` use the same conversion. The source
is a string array and the destination a string-valued map. A disposition
is mandatory: `:~` drops an odd tail, `:-KEY` makes the tail the value at
literal KEY, and `:?MESSAGE` aborts on odd count. `:!~` forces the source
read without validating it.

Success replaces the destination, including replacement by an empty map.
Duplicate keys retain their last value. The source remains unchanged.
Failure publishes no partial conversion and does not roll back earlier
effects. A fresh scratch construction cannot recover its previous instance.
Allocation failure remains fatal; this work adds no allocation-fault
conversion probe.

For example, a function can update both ordinary globals and caller maps:

```sh
fn replace { A=@B:~ }
map m=([old]=stale)
array pairs=(key value)
replace =m =pairs
echo ${m[key]}
```

Slots retain collection identity. Well-typed array/map literal assignment
replaces the actual's contents, array append remains legal, and map append
remains forbidden. Source and destination slots resolve through forwarding
and permutation. The existing one-signature, non-aliasing, readonly,
scratch and taint rules continue to apply. Slot declarations, scalar slot
replacement and top-level slot use still reject.

DD99, DD100, DD124, DD131, GUIDE and the specification outline now agree.
The outline distinguishes reading the source pairs from consuming the array.

## Shared checking and lowering

Ordinary-name assignment-shape checks retain their early diagnostic stage.
Only A-E shape checks wait for the function's inferred signature. Both use
the same shape diagnostic helper; type checking now returns ordinary
diagnostics so contextual shape errors retain their own codes rather than
being relabeled SB4030. The generic statement-pass traversal preserves its
existing scoped environment.

Declaration and assignment share collection-element and conversion checks.
String collection elements require explicit conversion of whole integer
values. Pair conversion does not silently reinterpret integer arrays,
integer maps or map sources. An uncalled slot template does not acquire an
invented string-element signature.

The lowering resolves the actual destination before writing it, but retains
the original source spelling for construction/lifetime decisions. Otherwise
`A=...` could mistakenly restart the caller's `_m` scratch lifetime and
permit a forbidden mutation.

Both declaration and assignment call `lower_map_from_array`. It resolves
the source slot, builds into the existing typed private-collection target,
then emits replacement-form `CollectionPublish` only after `MapFromArray`
finishes. There is no new runtime operation or backend ABI.

Publication uses `define_replacement`: ordinary writes retain unresolved
taint, while a legal direct replacement on the failed-attest path retains
its existing resolution permission. `define_initial` would erase old taint;
ordinary `define` would lose that permitted resolution.

## Before and mechanism controls

There are 37 new dual-mode assertions: `mapassign.1`-`.19` and
`slotassign.1`-`.18`. The final focused files pass 506 type cases and
178 collection-parameter cases in each backend.

The frozen original compiler fails exactly 14 new map assertions and
15 new slot assertions per backend, with no pre-existing failures in
either file. The other eight additions are preservation controls.
In particular, `mapassign.18` already proved fresh-scratch emptiness before
this change; it is not a newly repaired behavior.

Eight independent compiler reversions produced these exact failures in
both native and C:

| Reverted mechanism | Failed checks |
| --- | --- |
| Fill the public map directly rather than publish privately | `mapassign.4`, `.5`, `.18` |
| Publish with `define_initial` | `mapassign.15` |
| Publish with ordinary `define` | `mapassign.19` |
| Do not resolve the assignment target slot | `slotassign.1`-`.4`, `.9`, `.10`, `.18` |
| Do not resolve the conversion source slot | `slotassign.1`, `.2`, `.16` |
| Restart scratch using the resolved caller name | `slotassign.9` |
| Omit contextual slot-shape checking | `slotassign.7`, `.8` |
| Invent string element types for unknown slot templates | `slotassign.17` |

The first slot-shape mutation failed to build because deleting the pass
also removed a diagnostic-vector type constraint. Its explicitly typed
retry supplies the accepted evidence; the unbuilt attempt is not a ninth
control. All mutations were restored. Source, compiler, runtime, lockfiles
and protected inputs matched the saved positive manifest afterward and
again after integration verification.

## What the first full gate found

The first sweep occupied rows 7386194-7399198, with the predicted 13005
checks and 74 additions. Counts, plans and retirements matched, but seven
assertions failed: native/C `atint.16`, `scratch.16`, `scratch.17`, and
native `convdiscard.2`.

Their saved xtraces showed a shared cause: unquoted integer expressions in
string-array initializers now reached the shared element-type check.
DD30/DD76 require explicit stringification; DD98 disposal retains ordinary
RHS typing. The repair corrects the fixtures and converter, not the rule.

`atint.16` now uses an integer array, preserving its direct arithmetic
element and expected result 6. It remains a positive control against
blanket rejection of arithmetic collection elements. The two scratch
fixtures quote the integer capture, preserving the unused-slot acceptance
and the illegal string-cell mutation. The latter's actual diagnostic is
the SSA scratch-write guard at the helper's assignment, not a masking
SB4030 refusal.

The converter now reuses its integer-expression classifier when rendering
colon disposal into string-array sinks. The same defect was reproduced in
ordinary converted array and map literals, so their string-valued elements
also receive explicit quoting. Integer collections, existing string
interpolation, splats and operand ordering stay intact. The stock Bash CST
represents a map entry as a concatenation; only its whole value is quoted.
Bracket splats are excluded from scalar-integer classification.

Three native converter assertions cover those boundaries, with annotated
and unannotated real-CLI round trips. The frozen converter fails all three
plus existing `convdiscard.2`; the repaired converter passes all 227 cases.
The corrected taint file passes 556 cases and scratch passes 109 cases in
each backend. These focused runs use private TSVs, not spec-history writes.

## Final gate

```sh
timeout 1200 env NEW=3 RETIRED=0 FLIPS=7 TOTAL=13008 mk -p 1 gate
```

The final 320-second sweep passes at rows 7399199-7412206: native 6620,
C 6388, with 12995 passing and 13 existing skips. Exactly the seven
previous failures flip back to passing. The three first-ever IDs are
`native:convdiscard.8`, `native:convctype.1` and `native:convctype.2`.
There are no retirements, ghosts or plan mismatches; liveness is 246.

An independent comparison against the last good pre-change sweep,
rows 7373263-7386193, retains all 12931 statuses unchanged and adds exactly
77 passing mode IDs. This comparison does not use the shared HEAD hash as
a sweep identity.

The gate also passes 241 compiler, 27 converter and 20 expansion unit tests,
152 C regex checks and five getopt checks. Formatting, Clippy with tests,
totality and static accounting pass. The measured 29 touched compiler
functions have no live complexity violations. No fresh coverage run was
performed, and the converter/fixture integration did not alter the compiler
source used for the eight completed reversions.

## Separate hardening

A failed exploratory taint-retention fixture exposed an independent SSA
cell-write acquisition omission. Removing map conversion reproduces it on
the original compiler: `m[key]=$(- getenv NAME)` rejects with frontend
SB5021 but compiles with `--no-taint-cst`, in both native compilation and
C emission. Those compiler controls did not execute the generated programs.

The new [P3 hardening owner](../tasks/ssa-taint-cell-write-acquisition.md)
records the exact evidence and DG's existing conditional defense-in-depth
deferral. It does not reopen completed capture/bulk-fill work or claim that
other acquisition forms were characterized.

The implementation baseline is `4aeec21e5d70bbcca7fc7989908346201cad2403`.
Private evidence is retained under
`files/sb5025-global-map-20261122.cQT5jX/`: frozen compiler/runtime/converter,
before and positive manifests, focused TSVs/xtraces, eight successful
reversion reports, the unbuilt attempt, both full-gate logs and the final
publication record. Publication remains confined to `err-attempt-trial`;
main and DD136's provisional status are unchanged.
