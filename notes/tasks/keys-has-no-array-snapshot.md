---
status: open
priority: P3
depends:
gates:
---

# `$(keys m)` has no array snapshot, and DD#75 says it does

Found 2026-10-02, chasing DG's observation that **SB5073 disagrees with
SB5025**. The diagnostic contradiction is fixed (commit below). This is the
language gap that the contradiction was sitting on top of, and it is DG's call.

## What DD#75 §3 says

> `$(keys <map>)` → a **splat of the map's string keys**. Like `$(argv)`, it is
> an array splat subject to the DD#60 context rule: legal as the **sole source
> of a for-in header** (or **snapshotted into an array**), and **SB5025**
> anywhere else.

and, as the section's closing argument:

> Map access is symmetric and both forms already work: **`$(keys m)`** splats
> the string keys, **@m** splats the values.

## What is actually true

Measured against `target/release/sash` at `3448763`:

| position              | `@m` (values) | `$(keys m)` (keys) |
|-----------------------|---------------|--------------------|
| sole for-in source    | accepted      | accepted           |
| array literal `( … )` | accepted      | **SB5025**         |
| command argument      | SB5032        | SB5025             |

So they are symmetric in two positions out of three and diverge in exactly one:
**the snapshot**. `array k=( $(keys m) )` is rejected, `array k=$(keys m)` is
SB4064, and there is no out-param form — `keys m k` is SB5073. `$(keys m)` is
for-in-only, full stop. The parenthetical "(or snapshotted into an array)"
describes something that does not exist, and the symmetry claim is false for
the one position where it would matter.

`argv`, the other collection intrinsic, does have the snapshot: `argv a` /
`argv N a` fills an array and is a statement anywhere (DD#60, pinned by
`errors.sh` via.12). That is also exactly why `argv` is **deliberately absent**
from `VALUE_ONLY_INTRINSICS` while `keys` is in it — see that list's comment.

## Why it surfaced as a diagnostic bug

`keys` is the only collection-valued name among the 21 in
`VALUE_ONLY_INTRINSICS`, so it inherited the generic wording: *"Capture it
first (`x=$(keys m)`) and use `$x` here"*. That form is itself rejected — by
SB5025, which says in as many words that `$(keys m)` "cannot be joined into a
scalar". An author following SB5073 landed on a rule contradicting it.

Fixed by giving the collection case its own pair of wordings, both naming the
for-in form (`validate.rs::value_intrinsic_diag`, pinned by `errors.sh`
via.27-30). The diagnostics now agree. **But they agree on a restriction, not
on a way to do the thing**, which is what makes the gap worth DG's attention:
there is no answer to "I want the keys in an array".

## The options, for DG

- **(a) Leave it; correct DD#75.** `$(keys m)` is for-in-only by design; strike
  the parenthetical and soften the symmetry claim. Cheapest, and the
  diagnostics are already honest about it. Costs an author who genuinely wants
  a key array — today there is no spelling at all, not even a verbose one.
- **(b) `keys m k` out-param**, mirroring `argv a`. Makes DD#75's promise true,
  restores the argv/keys symmetry the shared `is_collection_intrinsic`
  predicate implies, and removes `keys` from `VALUE_ONLY_INTRINSICS` (it would
  then be a legal statement, exactly as `argv` is). Costs: a second arity, a
  type-check entry, codegen, and the taint story for a map's keys.
- **(c) Allow `array k=( $(keys m) )`** — the snapshot spelling DD#75 actually
  names. Narrower than (b), no new statement form, but it makes `$(keys m)`
  legal in a second context, which is the context rule SB5025 exists to
  enforce.

I have no recommendation I would defend; the three differ mainly in how much
language surface DG wants for a case that may not come up. Worth noting that
the workaround is not terrible — `for k in $(keys m) { … }` covers iteration,
which is what keys are usually for.

## See also

- `notes/decisions/DD75-canonical-keyword-forms-uniform-collections.md` §3
- `notes/decisions/DD60-argv-trio.md` (the out-param precedent)
- `sash/src/validate.rs::value_intrinsic_diag`, `keys_splat_diag`
- `spec/errors.sh` via.12, via.27-30
