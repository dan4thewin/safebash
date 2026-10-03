---
title: "Re-decide SB5067 now that the tainted set is grow-only"
status: next
priority: P2
kind: decision
depends: []
gates:
  - "DG picks one of the four options below"
---

# Re-decide SB5067 now that the tainted set is grow-only

**Blocked on DG.** Four choices were presented on 2026-09-30 and never
written down — DG looked for `notes/tasks/taint-sb5067.md` on 2026-10-03 and
found nothing, because it only ever existed as a session todo. Filing it so
the question survives.

## The question

`validate::check_fn_taints_global` (SB5067) rejects a function that taints a
global. It was written when the tainted set could *shrink*, so a function
body that tainted a name and then cleaned it still had to be rejected —
the set at the call site could not be reconstructed.

The set is **grow-only** now (removals were deleted; see the removal-was-the-
defect work in journal 448). That changes what the rule has to be
conservative about, so the rule should be re-derived rather than inherited.

## The four options

1. **Status quo.** Leave SB5067 as-is. Costs nothing, keeps a rule whose
   stated justification no longer holds — which is the kind of thing that
   later gets cargo-culted.
2. **A no-net-insert body rule.** Accept a function whose body leaves the
   tainted set unchanged at exit, reject one that grows it. Cheap to
   compute; still rejects some safe programs.
3. **Full call-graph propagation.** Propagate taint through calls properly
   and drop SB5067 entirely. Most precise, most work, and it has to survive
   the inlining the transform already does.
4. **Narrow SB5067 to collections.** Keep the rule only where the
   element-wise taint story is genuinely harder, and let scalars through.

## Why it matters

Whichever is chosen, the *comment* on `check_fn_taints_global` needs to
state the live justification. The current one argues from a property the
compiler no longer has.
