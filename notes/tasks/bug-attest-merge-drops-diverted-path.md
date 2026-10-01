---
status: done
priority: P1
resolved: 2026-10-02
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

## Resolved — the rule settled it

DG: *"don't we have a rule that a caught attest must leave a clean
variable/container?"* R2 says exactly that — **"clean on every path that
falls through to the merge"** — which makes the merge's required state a
matter of record, not a design fork.

`transform/control_flow.rs::lower_if_attest` now merges like every other
two-branch construct, via the shared `insert_phis_two_branch`. It was the
**only** one of the transform's six two-branch merges that did not.

Reading it against R2 turned up a second, larger fault than the one filed.
The comment there claimed to "restore to the pre-state", justified as
deliberate because SB5089 meant "there is no clean version to merge to" —
and R2 is precisely the guarantee that there *is* one. But the code also did
not do what the comment said: the restore ran only in the **no-else** case.
With an `else`, the state simply leaked out of the else branch, so

```sash
str x=$(getenv HOME)
if attest x [[ $x =~ ^/ ]]; then echo t; else x=d; fi
echo "$x"
```

lowered to a merge reading `x_3` — **the else-branch version, defined on
exactly one path**. That is the hazard journal 449/450 is about, in the code
that cites journal 449 as the reason not to touch it. It survived because
codegen collapses a scalar's versions to one slot, so the runtime value was
right and nothing observable went wrong.

Now:

| | merge reads |
|---|---|
| resolve (`\|\| v=8080`, `else v=0`) | `PHI x_4 <- [(then, x_2), (else, x_3)]` |
| divert (`else exit 1`) | `x_2`, the attested version, no phi |
| no else | phi against the untaken edge (SB5089 rejects; `--no-taint` reaches it) |

The divert arm emits no phi deliberately: the else path is not a predecessor
of the merge in any sense that matters, so the then-state *is* the merge
state. That keeps the attested version off every path where it is undefined,
which is the invariant journal 449 was protecting.

`block_falls_through` is now `pub(crate)` and shared with the transform
rather than duplicated — the predicate that decides "is this else a
predecessor" must be the same one `validate` uses to decide "is this path
exempt", or the two passes disagree. (Forking it is exactly the P1 in
journal 456.)

## Verified

Every form, both backends, on both the passing and the failing path —
including the collection attest-all failing path, which is the shape of
`bug-caught-attest-all-fail-path-undefined-handle`. `spec/taint.sh`
div.10-div.16 pin them; div.1-div.9 pin the two earlier layers and the guard
rails. Sweep 7243, zero failing.
