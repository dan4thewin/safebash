# 343 — Four copies of the same walk

Back to the coverage loop with a working triage. `hoist_mutation_node`
(`transform/statements.rs`) carried 28 cold lines across three arms, and
the filing called it "a genuine spec gap, not dead code". That was a third
right.

## Triaging the three cold arms

The function applies the store for an assignment embedded in a `(( … ))`
statement or condition — `(( (a=2) ))`, `(( 1 + (a=2) ))` — and returns a
mutation-free residual. Only the plain `Assign` arm had ever run.

Probing each arm separately split them three ways:

| arm | verdict |
|---|---|
| `CompoundAssign` | **reachable and working**, zero coverage |
| `PostIncDec` | rejected by SB4020 before transform — 12/12 spellings |
| `SubscriptAssign` | rejected by SB4020 before transform — 7/7 spellings |

`(( (a+=2) ))` gives 3. `(( 1 + (a+=2) ))` gives 3. `(( -(a+=2) ))`,
`(( (a+=2), 1 ))`, `(( (a+=2) + (b+=3) ))` all work. None of it was tested,
which is why the whole arm read as dead. That is the failure mode journal
341 named: a cold branch is an unasked question, and six of them were
answerable with `echo`.

The other two are genuinely unreachable, and unlike the `PreIncDec` case in
journal 342 the guarantee here is **structural**. `check_arith_test_placement`
walks every `ArithTest` statement and rejects the whole tree if
`arith_has_incdec`, `arith_has_subscript_assign`, or
`arith_mutation_under_cond` holds. Those are whole-tree predicates, not spot
checks at two entry points — which is exactly the property the prefix
rejection lacked and had to be given. I probed all nineteen spellings anyway,
because that is the lesson of 342.

So: six specs for the live arm, `unreachable!()` for the two dead ones citing
a new `ir::ARITH_TEST_HOISTABLE_INVARIANT`, and two more specs
(cov.262-263) pinning the rejections that make the invariant true. A guard
that depends on another pass should say which pass, and the spec suite should
fail if that pass stops.

Deleting those arms instead would have been actively wrong: they sit above a
`_ => None` catch-all, so a mutation that ever slipped through would have
been silently *not* hoisted — the store dropped, the program quietly
different. An ICE is the only safe removal here.

`hoist_mutation_node` went 10/6 to **6/1**.

## The thing I actually didn't expect

Chasing the rejection predicates, I found `validate::arith_children` — a
flat "give me the sub-operands of this `ArithExpr`" dispatcher. Which is
precisely what I added to `ir.rs` yesterday as `ArithExpr::children()`,
believing I was introducing a new idea.

There were **four** copies:

- `ir::ArithExpr::children` (mine, yesterday)
- `validate::arith_children`
- `validate::arith_operands`
- `transform::functions::arith_children`

The doc on `arith_operands` says, in as many words, "A flat dispatcher —
mirrors `transform::functions::arith_children`." The duplication was known
and documented rather than fixed, twice over, and then I made it worse
without noticing because I searched for the *concept* in the wrong place.

All four collapsed onto `ArithExpr::children()`, five call sites repointed.
Mine survives on merit rather than seniority: the three older ones end in
`_ => vec![]`, so a new `ArithExpr` variant silently gets no children and
every walker in the compiler quietly stops descending into it. The version in
`ir.rs` is exhaustive — a new variant is a compile error in exactly one
place, which is the whole reason to have one copy.

Worth noting what would have caught this: `block_dupes` in the index. Four
near-identical 10-line matches is exactly its use case, and I did not look
because I did not suspect. The lesson is cheap — **before adding a helper,
grep for the shape, not the name.** `arith_children`, `arith_operands` and
`children` are three names for one function.

## Result

Sweep **5245** (+16, predicted exactly), 0 failing, units 83. Eight specs,
two ICE arms, three walkers deleted.
