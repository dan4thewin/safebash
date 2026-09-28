# 439 — ICE where the route cannot open itself

Journal 438 ended with the two entirely-cold taint functions explained and
pinned by unit tests asserting their answers. DG asked the question that
undid that: **"How is ICE a worse alternative to a silent unintended change
going live?"**

It isn't. The argument I had made against an ICE was that it would "turn a
correct-but-novel program into a compiler crash". That sentence has a hole in
it, and DG's question is exactly the shape of the hole.

## The cost I invoked does not exist

Every route to those arms requires **a change to the compiler's own source** —
`SsaInst::Assign` beginning to report reads, the redirect spill being removed,
`[[ ]]` becoming an expression. None of them can be opened by a user writing
sash. So the ICE can only ever fire in the tree of whoever opened the route,
in their own spec sweep, while they are already thinking about the change.

Meanwhile the benefit is the thing I had told DG two turns earlier that these
arms *do not provide*: **detection**. An `unreachable!()` announces "a new
route reached me". A recursion arm's alternative is that the route opens and
nothing says so.

And "pre-paid correct answer" smuggled in its own conclusion. The answer is
unverified by construction — nothing exercises it. That is the same shape as
the `_ => {}` that let `SsaInst::Test` and `RedirectPush` escape this pass for
its entire life: a plausible answer nobody checked. For a taint walker,
silent-and-maybe-wrong is the worst of the three options, not the best.

## The line I should have drawn

Not "is the shape nonsense" but:

> **Unreachable by construction → ICE. Merely unexercised → correct arm.**

An arm cold because a compiler invariant forbids it can only be reached by
breaking that invariant, and that deserves a crash. An arm cold because no
spec happens to hit it would crash a real user.

**Coverage cannot tell the two apart.** That is what 438's work of *explaining*
every cold arm by construction was actually for — and applying the test here
is what stopped this commit being a bug.

## The test rejected three of the eight candidates

Eight arms looked convertible from the coverage ranking. Three failed the
proof and stay as correct arms:

| arm | why it is *not* provably unreachable |
|---|---|
| `collect_expr_var_name`'s `${…}` / `$a[i]` | `spill_scalar_redirect_targets` has two call sites — but `cmd_handlers.rs:296` builds an `SsaInst::RedirectPush` that goes through **neither**. An inlined function call's redirect is not spilled. |
| `check_value_use`'s `CmdSubstBody` | `lift_int_func_calls` removes it from value position, but its callers are the assignment/return/test paths only — not command arguments. |
| `cmd_subst_taint`'s user-function head | a `user_functions` set lookup, not a structural invariant. |

The first is the one that mattered. It was on my list, the by-construction
story was written down in 438, and it is **wrong** — the spill does not cover
every redirect. Converting it would have shipped a crash on a valid program.
Checking `RedirectPush`'s construction sites rather than the spill's is what
caught it.

## The five that converted

| arm | proof |
|---|---|
| `check_value_use` — `ArithExpr`, `ArgvIndex`, `TableValue`, `TableKey` | all 8 construction sites are in `transform/pipelines.rs`, every one of them an `SsaInst::Assign { target, value }`. `Assign` reports no `taint_reads`. |
| `check_value_use` — `AssocArray` | `ExprKind::AssocArray` is built only by `parse::parse_array_literal`, called only from `parse_one_decl_assignment` and `parse_assignment`. Assignment RHS only. |
| `expr_uses_tainted` — `Test` | `ExprKind::Test` is built only by `parse_test_command` and `parse_attest`, both of which yield a *statement*. Probed: `${h:-[[ -n x ]]}` is literal text, `"v[[ -n $x ]]"` prints `v[[ -n abc ]]`. |

Two functions disappeared with them: `check_assoc_use` and
`test_uses_tainted`, both now dead.

`test_uses_tainted`'s removal also dissolves a defect I had been about to ask
DG to rule on. The `TestExpr` recursion was hand-written **three** times —
`ir::walk_test_exprs`, `check_test_taint` (550 executions) and
`test_uses_tainted` (0) — where the arithmetic equivalent is written once and
delegated to twice. The copy nothing ran is now gone, so there is no
unverifiable duplicate left to justify. `check_test_taint` stays exactly as it
is: it is the hot diagnostic twin, reached from `check_expr_taint` on the
`[[ … ]]` statement itself, and it is what actually enforces that a test
operand is a read position.

## The asymmetry is the point, and it has a task

`value_is_tainted` keeps real, hot arms for all five converted shapes —
`TableValue`/`TableKey` 340 executions, `ArgvIndex` 36, `ArithExpr` 1586,
`AssocArray` 2592. It is the **derivation** half and it is reached for `Assign`
values; `check_value_use` is the **reporting** half and is not.

That is precisely what `notes/tasks/ssa-taint-full-backstop.md` already
diagnoses: "the derivation half already knows the answer; the reporting half
does not consult it." Worth recording that I overstated the connection to DG
first — I claimed the task's fix was one line from making these arms live. It
is not. The task proposes a whole-value check *at the top of*
`check_value_use`, and entry is still gated on `taint_reads`, which it does not
touch. The arms' coldness is a **symptom** of the split the task exists to
close, not something closing it would cure.

## The tests inverted

The six unit tests from 438 that asserted these arms' answers now assert the
panic instead — which is the honest statement: *this is still unreachable, and
we mean it*. The day one stops panicking, a read route was opened and the arm
must be written deliberately, with specs.

The three unprovable arms keep their original answer-asserting pins.

## One stale comment fixed in passing

`check_value_use`'s doc comment justified writing its recursion out rather
than delegating to `SsaValue::child_values()` "because that getter still ends
in a wildcard". That getter was made total earlier in this session, so the
stated reason was gone. The conclusion survives for a different reason, now
recorded: the arms are not a uniform visit-every-child — `Trusted` and
`StrLen` stop, `ElideEmpty` descends only when the author has not vouched, and
five shapes are an ICE.

## Verification

- Sweep **6740 passing, 13 skipped, zero failing** (6753 rows, unchanged —
  no specs added). **Zero flips.** Across 6753 spec executions covering the
  whole language surface, none of the five converted arms fired.
- `mk test` **71 passed** (was 67: −2 tests replaced, +6 added — predicted
  before running).
- fmt, clippy `--release --all-targets -D warnings`, totality, complexity:
  clean.

A sweep that does not panic is *not* proof of unreachability — that is the
"merely unexercised" case by definition. It is corroboration. The proof is the
construction-site enumeration above, and it is why three candidates were
dropped despite the sweep being equally silent about them.

## Coda: the fourth copy, and the helper that already existed

Deleting `test_uses_tainted` left the `TestExpr` recursion hand-written three
times. I told DG a shared shallow walker would have to be *added* to dedup
them. That was wrong, and wrong in the most ordinary way: one already existed,
`walk_test_operands`, in the same file four hundred lines above
`check_test_taint`. It was written for `check_attest_predicate_reads` and never
looked at again. So the count was four, not three, and the dedup needed no new
code at all — only the recognition that the helper was there.

The four:

| copy | shape | status |
|---|---|---|
| `ir::walk_test_exprs` | deep — `walk_expr_tree` per operand | hot (27688) |
| `validate::walk_test_operands` | shallow — operand roots only | hot, but `Not`/`And`/`Or` arms **cold** |
| `validate::check_test_taint` | shallow, hand-rolled | hot (550) |
| `validate::test_uses_tainted` | shallow, hand-rolled | deleted earlier today |

Collapsed to one. `walk_test_operands` moved to `ir.rs` as the single shared
shape; `walk_test_exprs` became a one-liner over it, and so did
`check_test_taint`. All four had been letter-identical, so there was no latent
divergence to find — the risk here was prospective, not realised.

The pleasing part is the coverage arithmetic. `walk_test_operands`' own
`Not`/`And`/`Or` lines were cold, because an `attest` predicate is never a
compound test. `check_test_taint` exercises all three. Routing the second
through the first closed three cold lines **by deleting code** — the opposite
of the usual move, which is to write a spec that reaches them. A cold line can
mean "nothing calls this", and sometimes the right answer is to give it the
caller that was sitting next door reimplementing it.

One hazard worth the doc comment it now carries: `check_test_taint` must use
the *shallow* walker. `check_expr_taint` descends into composites itself, so
routing it through the deep `walk_test_exprs` would report every nested taint
violation twice.

### Verification

Sweep **6740 passing, 13 skipped, zero failing** (6753 rows, unchanged).
**Zero flips.** `mk test` **71** — unchanged, as a pure refactor should be.
fmt, clippy, totality, complexity clean.
