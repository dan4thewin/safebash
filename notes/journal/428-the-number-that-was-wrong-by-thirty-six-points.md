# 428 — The number that was wrong by thirty-six points

DG asked what the current coverage is. Answering it turned up a bug in the
thing that answers it.

`mk cov-db` prints three summary lines. Two were right. The third said:

```
functions    5135 recorded,  2076 uncovered
```

59.6%, against 98.1% lines and 91.5% branches. That gap should have been
read as a finding years — well, days — ago. A codebase does not execute 98%
of its lines while leaving 40% of its functions untouched. The two numbers
are measuring the same thing from different angles and they cannot disagree
by thirty-eight points.

The first row I sampled settled it: `emit_arith_op` in
`codegen/arithmetic.rs`, reported `count=0`. Every arithmetic spec in the
suite runs through that function. Its own lines were 21/23 covered. The
function was not cold; the record was wrong.

## What was wrong

`coverage.db` is loaded from `target/coverage/union.lcov`, and the *union*
is the point: the 100% target is spec-reachability ∪ unit-test-reachability,
so the lcov splices two separate builds. `contrib/covdb.py` accumulates each
record type by key, summing duplicates, exactly so that a merge like this
lands correctly.

`cov_lines` is keyed `(file, line)` and `cov_branches` on
`(file, line, block, branch)`. Both merge across the two builds, because
those keys mean the same thing in both.

`cov_fns` was keyed `(file, mangled)` — which *looks* like the most precise
key of the three, and is the only one that is build-specific. A Rust v0
mangled name embeds the **crate disambiguator hash**:

```
_RNvNtNtCs15UJJ1bSSN7_4sash7codegen10arithmetic13emit_arith_op   554
_RNvNtNtCs3mxHViwJLz2_4sash7codegen10arithmetic13emit_arith_op     0
```

Same function. Two keys. The spec build reached it 554 times; the unit-test
build never did, and contributed a second row reading zero that nothing
could ever merge away. Across `sash/src` the two cohorts were 2852 symbols
at 96.7% hit and 2283 at 13.2%, summed into one denominator.

Truth: **2928 functions, 113 uncovered, 96.14%.**

## The fix, and the fix I didn't make

Strip every build-specific hash before keying — `Cs[0-9A-Za-z]+_` for v0 and
the trailing `17h<16 hex>E` for legacy mangling.

The obvious alternative was to key `cov_fns` on `(file, line)` like
`cov_lines`, which is simpler and needs no regex. I tried it: it gives
**2278 recorded, 29 uncovered, 98.7%** — a *better-looking* number, and a
wrong one. Distinct symbols legitimately share a start line; a closure and
the function containing it are recorded at the same line more often than
not, and 2852 distinct symbols in one cohort collapsing to 2278 (file,line)
pairs is ~574 of them merging. Keying by line hides an uncovered closure
inside a covered parent. That is the same class of error as the bug being
fixed — a key that is not the identity of the thing — and it would have been
invisible, because it moves the headline in the flattering direction.

Validated the normalization doesn't over-merge in the other direction: no
normalized key absorbs more than two distinct crate hashes, i.e. exactly the
two builds and never a third symbol.

## Why it survived

Nothing reads `cov_fns`. The documented triage query in AGENTS.md and in
covdb.py's own docstring joins `cov_lines` against the indexnudge `symbols`
table to rank *uncovered lines by function* — which is the better ranking
anyway, and which was always correct. `cov_fns` existed only to print one
summary line, so the wrong number had no downstream consumer to break.

That is the whole reason it lasted: **a number with no consumer is a number
with no test.** The docstring records that `cov_lines` had a sibling bug once
(INSERT OR REPLACE, last-write-wins, a trailing zero erasing a real count).
That one was caught because a line reading dead-code-cold is something
somebody acts on. Nobody ever acted on the function summary, so it was free
to be wrong.

## Headline, for the record

Union coverage of `sash/src`, as of the Sep-25 lcov:

| metric | recorded | uncovered | covered |
|---|---|---|---|
| lines | 27538 | 519 | 98.12% |
| branches | 3912 | 333 | 91.49% |
| functions | 2928 | 113 | 96.14% |

These are two days stale — sixteen measured files have changed since, and
`coverage.db` records no commit, so the per-line detail is invalidated even
where the aggregate is roughly right. The aggregates are reported as the
last honest measurement, not as today's.
