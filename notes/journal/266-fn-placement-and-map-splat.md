# 266 — three rulings: `fn` is top-level, `const` is not a loop body, a splat is values

*2026-09-11*

DG gave three verdicts in one message:

> I think all functions must be top-level - counterargument?  const shouldn't
> be within loop.  splat on map or join on map should probably just deal with
> values and not keys

All three are implemented here. The interesting part is not the code — it is
that I got the first one wrong on the first pass, and DG's follow-up question
on the third one killed a sentence I had written the day before.

## 1. `fn` must be top-level — SB5071

I went looking for evidence instead of answering from memory, which is the
only reason this got settled correctly.

```sh
if [[ 1 = 2 ]] { fn f { echo A } }
f
```

sash prints `A`. Bash says `f: command not found`. The branch never runs, and
the function is still defined. Same for `false && fn f { … }`, and same for a
`fn` used as a pipeline stage — bash runs that stage in a subshell, so the
definition dies with it. Two definitions in two branches: sash takes the
lexically last one, unconditionally.

It also turned out to be the **last remaining taint bypass**. A `fn` at top
level and a `fn` nested in another `fn` both correctly raise SB5020 on raw
`$1`; a `fn` inside an `if` was accepted and leaked argv straight through.
That is now closed by construction rather than by another taint rule.

### The counter-argument I offered, and why I withdrew it

I argued for the *narrow* form — reject `fn` in control flow, keep `fn` inside
`fn` — on the grounds that the nested spelling is a common organising habit
and specs 18.3/18.4 already pinned it. DG's instinct was the broad form.

DG was right and my argument was **cost-based, not principled**. Read DD#86
again: scope is positional, there is no block scope, and loop-variable
auto-scoping is an *explicitly rejected alternative*. Control flow therefore
creates no scope at all. A function name is global wherever it is written. So
the hoisting is DD#86-**correct**; the defect is that the placement *reads*
like a scope or a condition that the language does not have. That reasoning
does not distinguish `if` from `fn` — a `fn` inside a `fn` is misleading for
exactly the same reason. Specs 18.3 and 18.4 were pinning the wart, so they
are inverted here into rejections.

The unifying rule, which SB5071 and SB5061 now share: **reject a construct
whose appearance implies something the language does not do.**

## 2. `const` in a loop body — SB5072

Falls straight out of the same no-block-scope model. The binding is
re-executed on the second iteration, which is a `const` reassigned.

It was already rejected — but only by the spanless `VersionState::define`
backstop, which reports at `1:1` with no useful text. In `while`'s case it was
rejected only as a *side effect* of the `readonly_vars` leak across the
two-pass lowering (see 265). Both of those are accidents. SB5072 makes it a
real diagnostic with a real span; spec 86.27 asserts the location is not
`:1:1:` so the backstop can't quietly take over again.

`const` inside a **function body** stays legal. DD#86 Phase 2 already fixed
the identical leak there with `enter/exit_fn_body_scope`; spec 86.30 pins that
it stays fixed.

## 3. A splat yields values, not indices

`join m ,` already produced `v1,v2` and `for v in @m` already produced
`v1 v2`. Neither was pinned by any spec, so both were undefined behaviour that
happened to be right.

I flagged a contradiction with `join-map-reject`, filed the day before, which
opens by describing `@m` as producing pairs. DG's reply settled it in one
line:

> arrays and maps differ only in how they're indexed, splat of an array
> doesn't produce indices, why would splat of a map…?

Map keys are **indices**, not data. `@a` does not give you `0 1 2`; `@m` has
no business giving you keys. The DD#75 `len` amendment from the day before had
already blessed this — its survey table lists `for v in @x` ✅ for array *and*
map, one row above the `join` row that contradicted it.

### SB5061 survives, on different reasoning

`join-map-reject` proposed allowing `join m ,`. It is still rejected, but the
"silent mis-pairing" argument it rested on presupposed the pairs reading and
dies with it. DG supplied the replacement: **there is no round trip.**

```
array   (k1 v1 k2 v2)      4 elements
  → map                    2 entries
  → values (v1 v2)         2 elements
  → map again              1 entry, keyed v1
```

`a ≠ b`, `m ≠ m2`, silently, at every step. That is worth a diagnostic on its
own terms. DD#99 stage 8's verdict survives; only its reasoning was replaced.
Spec af.10 demonstrates the broken round trip directly.

## Three traps worth recording

**A spec can pass vacuously.** af.10's assertion was
`[[ $rc != 0 || $out = … ]]`, and it was passing on the *first* disjunct
because the program didn't compile at all — `$(keys m2)` is SB5025 (valid only
as the sole source of a `for`-in) plus SB5062. A "demonstration" spec that
never compiles demonstrates nothing. Check that the program builds before
trusting a `rc != 0` short-circuit.

**Don't re-indent a spec block when wrapping it in `in_modes`.** Doing that to
guard af.5–af.10 for janet broke three native/c tests: the multi-line
`run_sash '…'` heredocs and the `$out` comparison strings are
whitespace-sensitive. Wrap without indenting. Bash does not care.

**Baseline regression queries are vacuous when HEAD hasn't moved.** `runs`
records a `commit_hash`, so re-running `run.sh` against a dirty tree records
under the *same* commit and the `MAX(rowid)` baseline CTE returns the current
run. Use an older commit — `4d7d792` here. And that JOIN silently excludes
newly-added tests, so pair it with a `NOT EXISTS` pass to catch a new test
that lands failing.

## Result

Sweep **4947 / 38 / 501**, zero non-janet failures. 55 new tests, all passing;
6 new skips (af.5–af.10 on janet, blocked on the known `janet-map-from-array`
gap that already guards af.4). One flip against `4d7d792`: `janet:18.3`
failing → passing, which is an improvement — it is now a rejection test and is
settled in `validate` before codegen ever runs. Rust suites diff empty against
baseline: 140 pre-existing failures, same names.

Closed: `taint-fn-in-toplevel-control-flow` (by construction),
`while-two-pass-readonly-leak` (DG's ruling makes the leak irrelevant), and
`join-map-reject` — **reversed**, it was not a bug.

Deferred by DG: the `len`/`strlen` split, as its own pass. It absorbs the
DD#100 Part D splat-operand removal — roughly 35 spec sites, SB5025's message,
and `sash-convert`.
