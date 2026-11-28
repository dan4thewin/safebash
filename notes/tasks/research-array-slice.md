title: Implement the array-slice filler
status: done
priority: P2
kind: work
depends: []
gates:
  - Native and C implement DD137 through shared typed snapshot and filler publication
  - Preserve sparse ordinal order, exact values, types, readonly sources and independent snapshots
  - Preserve taint, scratch, slot and function-arity contracts
  - Focused before and independent reversion controls, then the predicted ordinary gate
  - Publish the accepted implementation and tour examples on main

# Array-slice filler

DG accepted the implemented array-slice filler for main on 2026-11-28.
[DD137](../decisions/DD137-array-slice.md) is the contract:
`slice SOURCE START [COUNT] DEST`, an independent, typed ordinal snapshot
published under the existing accumulating/scratch filler rules.

Implementation began on `slice-trial` from main `727fe371`. The earlier
shift research below supplies the motivation, not a competing current spec.

## Delivered trial, 2026-11-26

The compiler/runtime implementation is complete in native and C, including
forwarded source-element types, function-args arity, retained bounds,
checked source reads and resultless-use refusals. It uses the shared
private snapshot/publication path rather than a new view or copy-on-write
representation.

All 62 slice specs pass in each mode and fail on the matched pre-trial
compiler/runtime. Thirteen independent reversions match their predicted
failing sets in both modes; the restored source and artifacts match the
positive build exactly. The full gate passes with 124 new IDs, no flips or
retirements, and 13,364 checks. See
[journal 677](../journal/677-provisional-array-slices.md) for the evidence.

## Accepted on main, 2026-11-28

DG requested slice examples in the tour, then directed:

> drop the provisional language, we'll take this to main

[Journal 679](../journal/679-slice-tour-and-acceptance.md) records the
acceptance, executable tour examples and main landing. This owner is complete;
converter migration remains separate.

## Why

`shift` was dropped (DD#75 §5 amendment, 2026-08-21) — it is a *degenerate
slice* (`args = args[n:]`, drop-from-front only). DG: "shift feels like a toy
version of a slice." A general slice provides sub-ranges uniformly over
indexed arrays, not just implicit `args`, without making `args` writable.

The common `shift` use — iterate/consume args — is already served by
`for x in @args`. Slice covers the narrower "skip N, keep the rest" and general
sub-ranges.

## Superseded research

The pre-trial record considered views and copy-on-write, array-valued
`$(slice ...)` syntax, from-end bounds, and aborting past the end.
DD137 instead chooses an explicit filler and independent snapshot,
nonnegative ordinal bounds, and empty/truncated batches. The old
element-wise-taint sketch predates the whole-binding checked-read model;
slice does not copy unresolved taint into another binding.

The original research remains available in this file at `727fe371`.
No view/COW machinery or converter rewrite is introduced by this implementation.

## Notes

- The converter's Bash `shift` migration remains separate. Slice writes a
  named destination; it does not mutate Bash's implicit positional vector.
- `shift` removal (2026-08-21): removed `BuiltinCmd::Shift` + `from_name`, the
  validate builtin entry, `handle_shift_stmt` + dispatch, `handle_shift`
  (codegen), the `emit_janet` arm, and spec 48.2. A bare `shift` now resolves as
  an (unknown) external → SB5011; a dedicated "shift is removed — use
  `for x in @args` / `slice args ...`" diagnostic remains separate
  nice-to-have work.
