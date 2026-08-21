---
title: "research: a general array-slice operation (post-beta) — subsumes the dropped `shift`"
status: research
priority: post-beta
kind: research
depends: []
gates:
  - "decide value vs. view semantics (recommend value/copy; ponder copy-on-write)"
  - "decide syntax (lean: a `slice` intrinsic, not bash `${a[@]:s:l}`)"
  - "decide negative/from-end + out-of-range disposition (consistency with DD#60)"
  - "confirm taint propagation (element-wise, DD#61) and `args[1:]` as the shift replacement"
---

# research: a general array-slice operation (post-beta)

## Why

`shift` was dropped (DD#75 §5 amendment, 2026-08-21) — it is a *degenerate
slice* (`args = args[n:]`, drop-from-front only). DG: "shift feels like a toy
version of a slice." A general slice subsumes it and gives arbitrary sub-ranges
from both ends, uniformly over any array (not just the implicit `args`). This is
a **post-beta** feature: the args model ships read-only for beta; slice is a
deliberate design task, not carried by inertia.

The common `shift` use — iterate/consume args — is already served by
`for x in @args`. Slice covers the narrower "skip N, keep the rest" and general
sub-ranges.

## Axes to decide (do NOT pre-decide — this is the research)

1. **Value vs. view semantics — the crux.** Go slices are *views* over shared
   backing storage: `b := a[1:]; b[0] = x` mutates `a`. That aliasing is a
   notorious review hazard. For sash's "easier to review than bash" bar, lean
   **value semantics (copy)**: `b = a[1:]` yields an independent array —
   deliberately *unlike* Go.
   - **Ponder copy-on-write (DG, 2026-08-21).** Value semantics is the
     reviewable model but naive copying is O(n) per slice. COW gives value
     semantics with view-like efficiency: share the backing table until a write,
     then copy. Worth pondering — it keeps the simple mental model (independent
     values) while avoiding the copy cost, at the price of a write barrier /
     refcount on the DD#33 table. Evaluate whether the runtime table can carry a
     refcount + copy-on-mutate cheaply, and whether the auditability story stays
     clean (no observable aliasing).

2. **Syntax.** sash does array ops as intrinsics + `@` splat (`len`/`keys`/
   `join`, `@a`), not bash's cryptic `${a[@]:s:l}`. Lean: a `slice` intrinsic —
   `$(slice arr start [len])` → a new array, consumed via `@(slice …)` or an
   assignment. Keeps the no-magic-subscript posture.

3. **Negative / from-end + bounds.** DD#60 already rejects negative argv indices
   (`$(arg -1)`); consistency says slice rejects negatives in v1 (no from-end)
   unless deliberately wanted. Out-of-range → sash's instinct is reject/abort,
   not Go's silent clamp — decide.

4. **Taint & `args`.** A slice inherits element taint (DD#61: clean→clean,
   tainted→tainted). `args[1:]` (the shift replacement) is just the general op
   applied to the implicit `args` array — no special case.

## Notes

- Pre-beta, so dropping `shift` has no compat cost. The converter emits a MANUAL
  note for bash `shift` until slice exists (converter work is last, T6).
- `shift` removal (2026-08-21): removed `BuiltinCmd::Shift` + `from_name`, the
  validate builtin entry, `handle_shift_stmt` + dispatch, `handle_shift`
  (codegen), the `emit_janet` arm, and spec 48.2. A bare `shift` now resolves as
  an (unknown) external → SB5011; a dedicated "shift is removed — use
  `for x in @args` / a future slice" diagnostic would be friendlier (nice-to-have).
