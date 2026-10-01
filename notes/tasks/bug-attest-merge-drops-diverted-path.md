---
status: open
priority: P1
depends:
gates:
---

# A caught scalar attest whose failure path diverts is still rejected

DD#121 R2 exempts a path that does not fall through. Its own table names

```sash
if attest v [[ … ]]; then … ; else exit 1; fi
```

as legal, and the SB5089 diagnostic recommends `exit`. Neither scalar form
compiles:

```sash
str x=$(getenv HOME)
attest x [[ $x =~ ^/ ]] || exit 1
echo "$x"                       # rejected: SSA taint backstop
```

```sash
str x=$(getenv HOME)
if attest x [[ $x =~ ^/ ]]; then :; else exit 1; fi
echo "$x"                       # rejected: SSA taint backstop
```

Two of the three layers are fixed and pinned by `spec/taint.sh` div.1-div.9:

- `validate.rs::taint_list` merged the `||` right operand's out-state
  unconditionally, undoing the exemption `check_list_caught_resolution` had
  just honoured. Now gated on `block_falls_through`, mirroring `merge_paths`.
- `ssa_taint.rs::classify_block_defs` unioned every phi source including one
  from a block that exits. Now filters diverging predecessors.

With both, the **collection** form works end-to-end (div.1, div.2) because
the collection subject reaches the merge as one name. The scalar form does
not, and the remaining layer is deliberate code.

## The remaining layer

`transform/control_flow.rs::lower_if_attest` restores the pre-attest version
at the merge and emits no phi. Ground truth for `attest x … || exit 1`:

```
BLK 0 entry            Assign x_1 = CmdSubst(getenv) ; Test ; Branch(attest, 1, 3)
BLK 1 if_attest.then   Attest { target: x_2, source: x_1 }  -> Goto(2)
BLK 2 if_attest.merge  Command echo args:[Var(x_1)]    <-- the PRE-attest version
BLK 3 if_attest.else   Command exit 1                  -> Goto(2)
```

The comment there says this is "deliberate, not a simplification", on the
premise that

> DD#121 requires the subject to be resolved on every falling-through path
> anyway (SB5089), so there is no clean version to merge to.

That premise holds for R2's *resolve* arm and fails for R2's *exemption*
arm — when the other path diverts, the attested version is the only one that
reaches the merge, so there is a clean version to merge to and the merge
reads the wrong one. Runtime is unaffected (both backends collapse the
versions to one slot), so this is purely an analysis gap.

## Why this is not a drive-by fix

The same comment records that an earlier one-sided definition in this spot
was the SIGSEGV in `bug-caught-attest-all-fail-path-undefined-handle`, and
journal 449 deliberately made `codegen/dispatch.rs::emit_attest_retag` *alias*
the one-path attest version to the source slot rather than declare it. Adding
a merge phi puts a phi on a name whose versions are aliased to a single slot,
in exactly the area that produced a nondeterministic miscompile. It needs
DG's call.

## Options

- **(a) Emit the phi.** `lower_if_attest` inserts
  `PHI x_3 <- [then: x_2, else: x_1]` when the else path diverts, and the
  merge reads `x_3`. With the layer-2 filter already in place the backstop
  then sees it clean. Correct SSA; interacts with journal 449's aliasing and
  with `codegen/setup.rs::map_inst_versioned` (an unmapped version returns a
  null pointer *silently*).
- **(b) Dominator-aware backstop.** Replace the backstop's single global
  name -> taint map with per-block dataflow, so a read dominated by a
  surviving attest is clean without any SSA change. Larger, and the backstop
  is a backstop.
- **(c) Sanction the blind spot.** Document it as the front end already does
  for `StrLen` and `taint_off`. But here the front end *allows* the program
  and the backstop rejects it, which `ssa_taint.rs` itself calls "a
  user-facing ICE" — so (c) means leaving a documented-legal form
  uncompilable.

## Reproducers

Both scalar forms above, each with the subject read after the merge. They
are deliberately **not** in the spec suite: they fail today, and pinning a
bug as expected is not what this suite is for. Add them beside div.1-div.9
when this lands.
