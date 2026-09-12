# 271 — dropping dtoi/itod, and the hole they were hiding

DG: *"I have changed my mind. dtoi and itod are YAGNI. how much churn to
drop them?"* — then, after I measured: *"finish in-flight, then drop them
— the library was just scope creep."*

Two commits. The removal is the small half.

## The removal

~190 lines across 16 files, all of it table rows and two runtime
functions. `sash-rt/src/inner.rs` lost `dtoi_inner`/`itod_inner` (142
lines), `lib.rs` the two `extern "C"` wrappers, `sash-rt-tests` a
100-line test block I had missed when I first measured the churn — the
earlier count said "16 files" and was taken from `sash/src` alone.

Everything else was one or two lines per file: a Cranelift signature, an
`ExternFuncs` field, an arity-table row, a taint arm, a `STRING_INTRINSICS`
entry. That is what a well-factored intrinsic costs to delete, and it is
worth noting that the cost of *keeping* them was higher: `itod 2 hello`
returned `0.00`, `itod $s 42` returned `42.` with the scale silently 0,
and `dtoi 2 $n` stringified an int and reparsed it. Three instances of the
recurring class in one pair of functions. Removal dissolved a queued task
(`dd103-itod-int-only`, to make `itod` int-only) rather than doing it.

No removed-builtin table, per DG's earlier ruling: `dtoi` is now an
ordinary unknown token and gets SB5011 in statement position, in `$( )`
capture, and in an arithmetic sexpr alike.

## The hole underneath

Verifying the removal, I checked the sexpr spelling:

```
$(( (dtoi 2 3) ))    before: SB4030      after: compiles, prints 0
```

Not a removal artefact — a pre-existing hole the *name* had been hiding.
`is_sexpr_string_producer` rejects **known** string producers; `dtoi` was
on that list, so the sexpr form got a clean error for the wrong reason.
Take the name off the list and the expression falls through to
`IntFuncCall`, finds no user body, and yields 0. `$(( (nosuchfn 1) ))`
always did.

I reported it rather than patching it, and DG reframed it as a rule
instead of a bug:

> *"Shouldn't any unrecognized symbol in arithmetic context be an error
> except an assignment?"*

That is a much better question than the one I asked. I had come with two
symptoms and a menu of diagnostics; DG replied with the invariant. So I
measured the invariant instead:

| spelling            | undefined name | code   |
|---------------------|----------------|--------|
| `$(( zz ))`         | rejected       | SB5031 |
| `$(( $zz ))`        | rejected       | SB5031 |
| `$(( ${zz} ))`      | rejected       | SB5031 |
| `$(( zz[0] ))`      | rejected       | SB5031 |
| `$(( a[zz] ))`      | rejected       | SB5031 |
| `(( zz ))`          | rejected       | SB5031 |
| **`$(( (zz) ))`**   | **`0`**        | —      |
| **`$(( (zz 1) ))`** | **`0`**        | —      |

Every spelling already obeyed DG's rule. `ArithExpr::FuncCall` was the
sole violator, and the assignment exception DG named (`(( x = 5 ))`,
`x += 1`, `x++`, the `for (( i=0; … ))` initializer) was already correct
and matches bash.

So the fix stopped being "add two checks" and became "make one node stop
abstaining". `validate.rs` had been calling `collect_only` there on the
grounds that the name *might* be an intrinsic — recording it for dead-var
but never flagging it. Meanwhile the transform resolved the very same node
positively, as a variable read or an empty int-func call. Two passes
disagreeing about what a node means, with an unresolved name falling
through the gap as a plausible constant. DD#103 writes the rule down.

## Getting it wrong first

My first implementation rejected *every* with-args sexpr whose callee was
not an intrinsic, on the strength of this probe:

```
fn g(int a) -> int { return a; }
$(( (g 5) ))     → SB4014 "expected expression"
```

I read that as "user functions with arguments do not parse in a sexpr" and
built the rule on it. Six native specs and six C specs disagreed:

```
fn add -> int { return $(( $1 + $2 )); }
int x=$(( (add 3 4) ))          → 7
```

DD#58's calling convention is positional `$1`/`$2`, not typed parameters.
My probe used a declaration form that fails for an unrelated reason, and I
generalised from it without checking the specs that already covered the
surface. `16.2`, `27.3`, `58.2`, `58.3`, `58.34`, `58.35` had been pinning
`(add 3 4)` the whole time. **The suite knew; I had not asked it.**

The correction needed the function set at the check site, which `known`
deliberately blurs (it merges functions into the variable scope so `(g)`
is not flagged). `ReadSink` became a struct carrying `funcs` alongside the
mode — two construction sites, four match arms. In `Collect` mode the set
is empty and every branch records the name, which is the same reasoning
that already lets `collect_reads` pass an empty `known`.

## A janet gap on the way out

The new control spec — `(x)` grouping on a defined variable — failed under
janet:

```
(var n (+ (x) 1))     error: 7 called with 0 arguments
```

`arith_funccall_to_janet` emitted `(name …)` verbatim for the nullary case.
`arith_var_to_janet`, three lines above it, already makes the right
decision for the bare spelling: call if `is_int_func`, else `(_num name)`.
The same concept, two spellings, a guard on only one — the shape this
whole entry is about. Reusing it was one line, so I fixed it rather than
guarding the spec to `native,c` and filing a task, which would have
entrenched the gap. The intrinsic sexprs (`(len a)`, `(random)`) are still
unknown symbols under janet; those belong to the existing parity cohort.

## Numbers

Sweep 5983 runs — 5381 passing, 41 failing (all janet, unchanged), 567
skipped, **zero non-janet failures**. Flips against the DD#102 baseline:
`36.2`/`36.3` retired in all three modes, `sx.18`–`sx.23` new and passing
in all three. No existing test changed status.

Fail-first verified: `sx.17`–`sx.20`, `sx.23`, `36.1`, `36.4` all fail on
the prior compiler; the two no-false-positive controls pass on both.

Two clippy warnings in `dtoi_inner` went with it — `sash-rt-tests` is now
warning-clean.

## What I'd carry forward

The measurement was the same as last session's: build a table of every
spelling of one concept and look for the row that disagrees. It found the
`len $s` bug, then `(strlen s)` in the sexpr rebuild, then both of these,
then the janet one — four in two sessions, all the same shape.

The other lesson is narrower and sharper: **when a probe contradicts a
whole cohort of passing specs, the probe is wrong.** I had the evidence to
catch the `(add 3 4)` mistake before writing the code and did not look for
it, because the probe felt like measurement. A probe I wrote is a
hypothesis; the suite is the record.
