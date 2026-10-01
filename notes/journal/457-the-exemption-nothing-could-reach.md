# 457 — The exemption nothing could reach

Coverage ranking again, and again the cold line was not the finding — it was
the door. Three leads out of `coverage.db`; one turned out to be dead code,
one turned out to be a P1, and the P1 is only two-thirds fixed because the
last third is deliberate.

## The lead that was deliberate divergence

`type_check.rs::default_is_arith_literal` had a cold `DoubleQuoted` arm.
Probing `$(( ${s:-"7"} ))` and `$(( ${s:-'7'} ))` produced the opposite of
what I expected on *both*: the double-quoted form is rejected (SB4030), the
single-quoted form yields 7. Against bash, sash is inverted twice over.

Checked `notes/decisions/` before calling it a bug, as the rules require,
and that was the right instinct. DD#65 settles it from two directions: the
2026-07-04 section makes `'` and `"` ordinary literal bytes and records the
bash divergence as **accepted**, and the 2026-09-30 follow-on makes a fully
single-quoted operand strip, as the hatch for a literal `$`. Both behaviours
are exactly as designed.

But the follow-on also *retired* the interpolating operand, and nothing went
back to the type checker. `parse_disposition_literal` returns an
`ExprKind::Literal` unconditionally; every branch of
`parse_disposition_operator` builds a `Literal` directly. The `DoubleQuoted`
arm had been dead by construction since that day, and its doc comment still
cited DD#65 for behaviour DD#65 no longer describes. The coverage measurement
and the construction argument agree, which is the only time a cold line is
worth acting on.

While there, two statements in DD#65 itself were stale: the `${x:-'a'}` →
`'a'` example the follow-on contradicts, and a lowering note naming both
`ExprKind::DoubleQuoted` and the janet backend DD#104 removed. Marked as
superseded rather than rewritten — a DD is a record.

## The lead that was a P1

The other two leads were `attest` list resolution. Building out DD#121 R2's
matrix produced this:

| form | DD#121 R2 | compiler |
|---|---|---|
| `attest x … \|\| x=d` | legal | ✓ |
| `if attest x …; then …; else x=d; fi` | legal | ✓ |
| `attest x … \|\| exit 1` | **legal** | SB5020 |
| `if attest x …; then :; else exit 1; fi` | **legal** | backstop reject |

R2 exempts a path that does not fall through. The second rejected row is
*verbatim the example in DD#121's own table*, and the SB5089 text explicitly
recommends `exit`. The compiler rejected the form its own diagnostic
recommends, and no spec covered it — the `else exit 1` specs in `coverage.sh`
all read the subject inside the then-branch, never at the merge.

Three layers, found one at a time, each hiding the next.

**One.** `taint_list` ended with `tainted.extend(right)`, unconditional. The
`if` path had filtered on `block_falls_through` via `merge_paths` all along;
the list path merged the out-state of a branch that exits. Worse,
`check_list_caught_resolution` had *already computed* `block_falls_through`
for its own diagnostic a few lines earlier, honoured the exemption, and then
discarded the fact — so one function granted the exemption and the next one
took it back.

**Two.** With that gated, both forms failed identically, which localised the
rest. The SSA backstop's phi rule was `any(source tainted)` with no CFG
awareness: the SSA does not model `exit` as a terminator, so an exiting block
is still a phi predecessor and its tainted version is still unioned in. This
one is independently a bug, and I proved it both directions with a temporary
toggle before believing it —

```sash
if [[ $c == y ]]; then x=safe; else exit 1; fi
echo "$x"
```

is rejected as `tainted variable x_3 … (SSA taint backstop)` without the
filter and compiles with it, with no `attest` anywhere. `ssa_taint.rs` says
of itself that a backstop false positive is a user-facing ICE. It was right.

**Three.** `transform/control_flow.rs::lower_if_attest` restores the
pre-attest version at the merge and emits no phi at all. The merge reads
`x_1` while the then-block produced `x_2`. And the comment there says, in
terms:

> That is deliberate, not a simplification — … DD#121 requires the subject
> to be resolved on every falling-through path anyway (SB5089), so there is
> no clean version to merge to.

Which is true of R2's *resolve* arm and false of R2's *exemption* arm. When
the other path diverts, the attested version is the only one reaching the
merge, so there is a clean version to merge to. The premise was written
while looking at one half of the rule.

That is where I stopped. The rules say ask before changing something that
might be deliberate, and this is not "might" — the comment says deliberate,
and records that an earlier one-sided definition in this exact spot was the
SIGSEGV in `bug-caught-attest-all-fail-path-undefined-handle`. Journal 449
then deliberately made codegen *alias* the one-path attest version rather
than declare it. Adding a merge phi means putting a phi on a name whose
versions are aliased to one slot, in the area that already produced a
nondeterministic miscompile. Filed as
`bug-attest-merge-drops-diverted-path.md` with three options and the ground-
truth SSA dump.

## What landed anyway

The collection form. `attest v in a [[ … ]] || exit 1` was SB5020 before and
compiles and runs now, because a collection subject reaches the merge as one
name and never needs the phi. I only discovered that by asking whether the
front-end fix was observable at all without layer three — I had been about to
record it as a necessary-but-invisible half-fix. It is pinned by `div.1` and
`div.2`, and I verified the attribution by reverting `validate.rs`,
rebuilding, and watching SB5020 come back.

Nine specs, `div.1`-`div.9`: both fixes, the resolve arm that must keep
working, and four guard rails that must keep rejecting (`|| echo no`, `&&`,
`if/else :`, and an unattested read behind `|| exit 1`). Two of them, `div.8`
and `div.9`, are the backstop fault and its converse with no `attest`
involved — the proof that layer two stands on its own.

Two of the nine failed first time for reasons that had nothing to do with the
bug: I asserted a *compile* outcome against `rc`, which is the compiled
binary's *runtime* status, so a program whose `attest` aborted at runtime
looked like a rejected program; and I forgot that `argv a` includes argv[0].
Both are documented, both caught me anyway.

A fourth surface fell out: `|| { exit 1; }` is a **false SB5089**, because
`block_falls_through` only inspects a trailing literal `Command` and a
`Group` is not one. That closes off `|| { echo "bad input" >&2; exit 1; }`
entirely, which is the shape people actually write. Conservative rather than
unsound, so P3, filed separately.

## Numbers

Predicted 7229 before running — 7207 plus nine dual-mode taint specs and two
dual-mode arithmetic specs. The sweep returned 7229 exactly: 3717 native,
3499 c, 13 skipped, zero failing. `mk test` green. `mk cov-reset` and a clean
`mk build` first, so the sweep did not run against an instrumented binary.

Coverage before this work: 97.50% lines, 98.82% functions.
