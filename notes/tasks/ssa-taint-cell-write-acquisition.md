title: "SSA backstop misses taint acquired directly in a map cell write"
status: open
priority: P3
kind: hardening
depends: []
gates:
  - "The real native and C CLI reject the reproducer with --no-taint-cst using the SSA backstop; normal compilation continues to report SB5021."
  - "Preserve clean cell writes, forced reads, read-before-write ordering, process isolation, and legal adjacent resolution."
  - "Pin the actual mutation/acquisition mechanism with a reversion control, not an earlier unrelated diagnostic."

## Evidence, 2026-11-22

```sh
map m
m[key]=$(- getenv MAPASSIGN_INPUT)
echo ${m[key]}
```

At `4aeec21e5d70bbcca7fc7989908346201cad2403`, native compilation and C
emission both reject this with the normal frontend enabled: SB5021 at the
cell RHS. Both accept with `--no-taint-cst`. The existing-map assignment
implementation tested alongside it gives the identical four outcomes.
No environment contents were executed or disclosed by these compiler controls.

`ssa_taint::definition_taint` returns no definition for `SubscriptAssign`;
the use checker walks constituent reads but does not reject this fresh
intrinsic value as a whole. Consequently this acquisition never establishes
the map taint that a later read would need to retain. Investigate mutation
versioning and fresh-value use checks together; assigning taint to an older
version must not retroactively taint earlier reads.

This is not a conversion-publication defect. The original probe added
`m=@clean:~` before the final read, wrongly attributing acceptance to that
replacement. Removing conversion reproduces the same omission on the
pre-change compiler. A control that first constructs a map from an
SSA-tainted array correctly retains taint through subsequent replacement.

The normal frontend covers this demonstrated case, so it remains separate
defense-in-depth work under DG's recorded conditional deferral in
[ssa-taint-full-backstop](ssa-taint-full-backstop.md). That task and
[capture-form builtin propagation](ssa-taint-capture-form-builtins.md)
remain completed for their delivered scopes. Other cell-acquisition forms
have not been characterized here.

Private evidence:
`files/sb5025-global-map-20261122.cQT5jX/ssa-cell-write-controls.json`,
`ssa-cell-write-control.sb`, and `taint-retention-probes.json`.
