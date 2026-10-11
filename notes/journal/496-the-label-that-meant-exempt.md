# 496 — the label that meant "exempt"

DG, on the "already settled, do not re-open" label I had put on five
functions in the coverage queue:

> no spec can warm them means they must be warmed by a unit test - the 100%
> goal as the union of specs and unit tests is achievable, is it not?

It is. The label was wrong, and wrong in a way that had been quietly
shrinking the queue for several sessions.

## What the rule actually says

Journal 466 records DG's policy:

> For any given function, I want every line warm by spec coverage except
> ICEs first. Then I want a unit test to cover the final ICE lines.

That is an **order of operations**, not a filter. 467 applied the second half
to 28 ICE lines. I had internalised it as a four-category triage where only
category 1 — a genuine spec gap — counted as work, and categories 2 and 3
(validate-shadowed, totality) were *finished* because no program reaches them.

"No spec can warm it" is a statement about the spec suite. It is not a licence
to leave a line unexecuted. The correct reading is that categories 2 and 3 are
in exactly the same position as category 4: reachable only by direct call,
therefore unit-test work.

That mistake is what let five functions carry a do-not-re-open stamp. Checking
one of them had already found a real spec gap (journal 495's postscript); the
stamp itself was the larger error.

## Twenty-three lines, thirteen tests, nothing clever

| function | lines | why no spec reaches it |
|---|---|---|
| `resolve_command` | 9 | SB5011 is raised in an earlier pass that exits first |
| `attach_redirects_to_last_inst` | 5 | two SB4046 ICEs + a parse-shadowed totality arm |
| `walk_arith_subscripts` | 3 | both callers sit behind rules that reject the spelling |
| `strip_ssa_version` | 3 | every key codegen builds carries a version suffix |
| `parse_var_base_version` | 3 | same |

Not one needed a harness, a fixture file, or a new abstraction. They needed a
`#[cfg(test)] mod` in the same file and a direct call. The barrier was never
technical; it was that I had stopped counting them.

## What the tests have to assert

A test that merely *runs* a cold line is not worth writing. Two of these do
real work rather than panic, and for those the assertion is the point:

- `walk_arith_subscripts`' `SubscriptAssign` arm differs from the `Subscript`
  arm in exactly one respect — it descends into `value` as well as `index`.
  That third line is what a reader would plausibly drop, so that is what the
  test pins. Mutation-checked: remove the descent and it fails
  `["outer","inner"]` against `["outer","inner","other"]`.
- `attach_redirects_to_last_inst`' `Pipeline` arm decides **which stage**
  receives the redirects. The test asserts `[0, 1]` across two stages.

The reason to bother is on the record: an unexecuted *correct-looking* arm is
the shape that let `SsaInst::Test` escape the SSA taint pass for its whole
life. Silent-and-maybe-wrong is the failure mode, and only an assertion
catches it.

## Wrong twice, caught by probe both times

Neither error was caught by reasoning.

`(( m[k] = 5 ))` raises SB4030 from `check_arith_bare_map_subscript`, which is
one of `walk_arith_subscripts`' two callers. I read that as evidence the
`SubscriptAssign` arm was spec-reachable — a category-1 gap wanting a spec,
not a unit test. A `panic!` planted in the arm never fired, through that
program and through a full native sweep: the diagnostic comes from a different
walker.

The session's other probe had already overturned the claim that `rdr.47`-`49`
warmed the `Pipeline` arm. Two for two. The technique is cheap enough that
there is no excuse for asserting reachability without it — plant the panic,
`cargo build --release`, **grep the binary for the probe string** (a silent
build failure leaves the old binary and makes the test vacuous), run the file.

## A divergence worth recording

The two base-extraction helpers do not agree. `codegen/values.rs::strip_ssa_version`
has no `is_empty` guard, so `"".chars().all(is_ascii_digit)` is vacuously true
and `"x_"` strips to `"x"`; `codegen/redirects.rs::parse_var_base_version`
guards, and returns `("x_", 0)`.

No producible key ends in a bare `_` — keys are built `name_version` — so
nothing can tell them apart today, and this is not a bug. It is recorded in
both tests anyway. A duplicated walker where a fix lands on one copy and
silently not the other is a shape this codebase has been bitten by twice, and
the cheapest moment to leave a note is while both are open in front of you.

## The measurement was lying by ten lines

Predicting the delta caught it. 23 lines targeted, 11 in the second batch, so
347 → 336 expected; the db said 339. The three-line gap was not rounding: one
target line was still cold, and **two new cold lines were in my own test code**
— a `&'static str` downcast arm that no `panic!`-with-arguments payload takes,
and the `else` of a `let … else`.

That is the exact complaint in `coverage-ranking-excludes-cfg-test`, filed
earlier and still open: `#[cfg(test)]` modules inside production files are
ranked as production. So the fix went in — option (1), filter at load, because
it cannot be forgotten. `covdb.py` scans for `#[cfg(test)]` + `mod NAME {`,
closing at the next column-0 `}`. Brace counting would be wrong; these modules
are full of `panic!("… {inst:?}")`, and a brace in a string is never alone at
column 0.

**1240 lines, 24 branches, 183 functions excluded.** `sash/src` cold lines
339 → 329, so test code had been contributing ten. `ice_text`, the example in
the task, now reports zero. `mk cov-db` prints the counts, so a scan that
matched nothing would be visible rather than assumed.

The missed production line was the `}` of the `Pipeline` arm's `if let` — its
region is the *empty-stages* path, a second layer of unreachability inside an
already-unreachable arm. It swallows the redirects silently, which is right
precisely because the arm is dead: an ICE there would assert something about
pipeline construction this function cannot see. Pinned, so the silence stays a
choice.

## Where this leaves the queue

`sash/src` is at **329 cold lines**, and the five functions are off the head
of it. The honest asterisk on "100%" is the one 467 already named: arms
unreachable because the *same function* covers the case — `arith_to_ssa`'s
trailing arm, where the leaf helper returns `Some` for the other four variants.
No call reaches those. They are eliminated by types, not tested, which is the
standing `eliminate-ice-arms-via-types` task.

Everything else is reachable by a spec or by a direct call, so the union goal
is achievable. It was never the arithmetic that was wrong — it was that I had
been subtracting a category that belonged in the total.
