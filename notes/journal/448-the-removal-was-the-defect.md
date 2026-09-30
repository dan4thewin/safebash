title: 448 — the removal was the defect

DG put a thesis on the table:

> requiring attest to immediately and unconditionally follow the operation that
> taints a variable eliminates all order-dependent taint-bypass. Absent an
> attest, the tainted variable can only be read with `:!` modifier.

The instinct was to reason about it. Measuring it was better, and it changed
the answer twice.

## Half of it already shipped

The second clause needed nothing. Every read form of a tainted scalar is
already rejected — copy, `[[ ]]` comparison, command argument (SB5020),
arithmetic (SB4030) — and `${t:!}` compiles. DD#91's 2026-09-09 addendum had
already made `:!` a general-position modifier. SB5020's own message has been
prescribing the discipline the whole time: *"`attest t [[ … ]]` at the source,
then read it."*

So the DD is about one clause, not two.

## What the corpus said

Patching `spec/common.sh` to dump every program a sweep compiles gave 3261
unique programs; 221 compile and use attest. Of 136 taint-source sites, 126
were already adjacent. Three were not, and all three were trivially
reorderable. The rule DG proposed was one the code had almost entirely
followed by hand.

That is the shape of a good rule: it costs nothing to adopt because everyone
was already obeying it, and it buys a property nobody could state before.

## The property

Before this, `attest` was *a statement that removed a name from a mutable set*,
at any point in the program. That one property caused all three P1s in the
backlog:

- it breaks joins — `validate.rs` walked every sibling body with one `&mut`
  set, so a removal inside a branch outlived the branch;
- it breaks the call-graph fixpoint — which is the only reason SB5067 exists;
- it creates "at which program point do I read the set?", which is how a
  trailing attest retroactively cleaned a slot read inside an earlier call.

Adjacency folds the attest into the definition. A removal can now only happen
one statement below the insert, in the same block. Nothing observes the taint
in between and no removal crosses a merge, so the lattice is grow-only and
taint is ordinary reaching-definitions. That is what AGENTS.md has claimed all
along — *"definition-based, not name-based"* — and what the implementation did
not honour.

## Two corrections, both from measuring

**The first**: my own count of "caught" sites was wrong. Twelve became five
once I stopped counting the `||` *inside* `[[ -z $x || -n $x ]]` as a caught
gate. A depth-aware splitter was the difference between a rule that looked
expensive and one that was nearly free.

**The second, and the one that mattered**: DG asked what the collision was
between the new rule and DD#24's sanctioned `attest … && CMD`. The literal
reading of the rule I had recommended — "both paths clean" — would have
rejected DD#24's own amendment *and* the flagship `if attest v; then use $v;
fi`. Measuring also showed that the extra rejection power over the simple join
was about nil: the SSA backstop already caught the scalar cases. What the rule
actually buys is a **located** diagnostic, which is a different and smaller
claim than the one I had made.

Then DG caught a real inconsistency I had shipped in the same breath: I offered
`else :` as an escape hatch while the rule I was proposing rejects it. It
falls through and resolves nothing. The genuine hatches are `else exit 1` and
`else v=0`.

## What implementation added

Three things the DD did not anticipate, all found by running specs.

**The assignment kill.** R2 has to be *satisfiable*. `attest v [[ … ]] ||
v=8080` only resolves `v` if the assignment itself cleans it — and the old
walk never cleaned anything, because `attest` removed the name unconditionally
and nothing put it back. So a whole-variable assignment from a clean RHS now
kills the taint, scalar and container literal alike. `+=` and `a[i]=v` do not:
one appends to what is there, the other replaces a single cell.

**Adjacency has four structural positions.** 111 specs failed on the first
green build, and 101 of them were one of four shapes where the tainting
operation is the syntactic *head* of the block below it: a `while` condition
(`while readline v { attest v … }`), an `if` capture-condition (DD#88), a
`for` in-source (`for x in $(argv 1)`), and a DD#100 slot arriving at a
function body (Part E, the attest deferred into the callee). None of these is
"the previous statement" in a flat reading, and all four are adjacency in
every sense that matters to a reader.

**Measure against empty, not against the incoming set.** The first cut computed
"what did this statement newly taint" as a set difference. A second `argv a`
adds nothing to a set that already holds `a`, so its adjacent attest was
rejected. The effect has to be measured against an empty set — which means
probing each statement twice, and a `probing` flag on `SlotCx` to keep that
from doubling at every level of nesting.

## Cost

Twenty spec rewrites, every one of them a single added `else`. Ten new specs
(`bfb.7`–`bfb.16`) pin R1, R2, R3, the two branch leaks and the `:!` bypass.
Sweep 3614/3396 → 3624/3406, green, totals matching the prediction exactly.

Two P1s closed: `bug-attest-in-branch-leaks-past-merge` and
`bug-slot-inward-taint-reads-final-set`. Both verified rejected, not assumed.

`bug-caught-attest-all-fail-path-undefined-handle` is untouched — a codegen
defect, reachable via `--no-taint` regardless. SB5067 is now unnecessary and
deliberately **left in place**: it has no DD, and retiring it is DG's call.

## The lesson worth keeping

The bug was never in a branch of `taint_if`. It was in the *shape* of the
representation: a mutable set with removals cannot model a join, and every
attempt to make it do so adds another special case. Three separate P1 filings
were three symptoms of one modelling error, and the fix was to change what
`attest` *is* rather than where it is checked.
