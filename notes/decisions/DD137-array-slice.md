title: DD#137 - Array slice filler
status: provisional

1. This is a separate-branch trial, not accepted on main. DG will evaluate
   the implemented operation before deciding on adoption.
2. `slice SOURCE START [COUNT] DEST` selects a batch from an indexed array.
   SOURCE and DEST are bare collection names; string and integer arrays,
   function `args` and correctly typed collection slots are supported.
   Maps are not supported.
3. Select zero-based positions in the present-value sequence used by
   `@SOURCE`, not numeric source keys. Skip holes and give the selected
   batch dense keys starting at zero. Preserve element type and exact bytes,
   including empty strings and interior NULs.
4. START and COUNT are ordinary nonnegative integer operands. Omitted COUNT
   selects the remainder. Zero COUNT, an empty source or START at/beyond
   the sequence's end produces an empty batch. An oversized COUNT truncates
   at the end; adding START and COUNT must not overflow.
5. Evaluate supplied bounds once, left to right, before the operation
   snapshots the named source. Earlier bound values survive later effects.
   The source and destination operands name identities, not copied values.
6. The result is an independent snapshot, not a mutable view. Construct
   the complete batch before publication, including when SOURCE and an
   ordinary DEST are the same array. Slicing never shortens SOURCE.
7. Apply DD131's filler rules: ordinary destinations accumulate; a new
   destination acquires the source element type; underscore destinations
   construct fresh immutable scratch. No partial batch is published on
   failure. Fresh construction cannot read its own target, including
   through a bound's helper.
8. Preserve readonly imports, immutable `args`, scratch ownership, typed
   collection slots and non-aliasing rules. A slice is an ordinary checked
   source read, not a new taint acquisition or a taint-resolution operation.
   Copying contents transfers no regex or redirect-nonemptiness proof.
9. Slicing function `args` makes that function variadic without requiring
   any skipped position. Explicit fixed-position reads retain their normal
   required/optional arity rules. DEST is an ordinary named array and does
   not inherit `args`' external-splat disposition exemption.
10. Slice is a resultless filler, not a status or array-valued expression.
    Ordinary stdout-capture bodies may contain it; captured bytes are not
    an array result.
    Invalid negative bounds reject; a runtime bound violation is a terminal
    guard, not a recoverable false result. Ordinary operand failures and
    allocation faults retain their existing rules.
11. Bulk `argv` keeps its separately settled literal-only offset. No `shift`,
    shared view, implicit mutation of positionals, new array-valued capture
    or copy-on-write mechanism is introduced.

# Array slice filler

DG requested a bulk array loader for the uses Bash expresses with `shift`
or `${@:2}`, discussed the snapshot/filler model, and on 2026-11-26 directed:

> take the current branch to main as accepted (no longer provisional),
> then start a new branch and implement slice as provisional

The preceding ERR-attempt branch is accepted separately in journal 676.
This trial starts as `slice-trial` from accepted main `727fe371`.
The existing [slice owner](../tasks/research-array-slice.md) now tracks
implementation and evaluation; its earlier first-class-value, element-taint
and out-of-range research sketches are superseded by this contract.

## Examples

```sash
array items=(a b c d)
slice items 1 2 _middle
echo "$(join _middle ,)"

fn remaining {
    slice args 1 _rest
    /bin/printf '%s\n' @{_rest:~}
}
remaining first second third
```

`_middle` contains `b`, `c`; `_rest` contains `second`, `third`.
At program scope, `argv 2 _rest` already selects that process-argument tail,
because process argv includes the executable name. Function `args` does not.

An ordinary destination appends the selected values after its current
highest key, as other fillers do. Repeatedly slicing successively shorter
tails can be quadratic; use ordinary iteration when consuming one element
at a time. Snapshot semantics do not require copying immutable string bytes
or adding a shared mutable view.

Bounds follow the ordinary command operand rule: evaluate operands, then
perform the named collection operation. This also makes an effectful bound's
mutation of SOURCE visible to the snapshot. It does not permit a later
bound to change the already evaluated START.

## Implementation and delivery boundary

The AI implemented the trial on `slice-trial` using shared operand roles,
source-derived collection types, retained integer bounds and a typed
`SliceFill` instruction. Both native and C call the same runtime snapshot
and use the existing private-batch publication path.

The 62 dual-mode specs, matched before controls, thirteen independent
reversions and predicted ordinary gate are recorded in
[journal 677](../journal/677-provisional-array-slices.md). Every temporary
mutation is restored. The language contract remains provisional until DG's
later adoption decision; converter automation for Bash `shift` is not part
of this trial.
