# 490 — the operand that stopped being a string

Continued the coverage queue from 489. Two cold clusters, four new specs, and
one hypothesis that measurement refuted — which is the part worth writing down.

## `$(wait)` in string position — a real gap, closed

`try_native_intrinsic` (`codegen/value_handlers.rs:637`) had ten cold lines in
a single block: the `"wait"` arm, `emit_wait_value` followed by
`emit_int_to_str`. That is DD#126's `wait` rendered as a *string*.

Every DD#126 spec written so far spells it `int p=$(wait)`, which takes a
different codegen path entirely — the int value never becomes text. So the
whole string route had never run. Three specs close it:

- `126.37` — `echo $(wait)` with no children prints `-1`. That is the only
  deterministic value available, and it pins sign handling for free.
- `126.38` — `str r="$(wait)"` equals the `$(getcpid)` of a real child.
- `126.39` — the targeted `$(wait $c)` form, also in string position.

Note `str p=$(wait)` is **SB4030** — an int in string position needs the
quotes. `str p="$(wait)"` is the spelling that works.

Measured after: lines 687–696 went from 0 to 3. Uncovered total 423 → 413,
which is the ten lines and nothing else.

## `expr_to_string` — the hypothesis was wrong

`transform/expressions.rs::expr_to_string` is entered **21225 times** across
the suite and takes the `Literal` arm every single time. Eight other arms are
cold, and both helpers it delegates to — `joined_form_to_string` and
`opaque_form_to_string` — show one entry each, which is their own ICE unit
test, not production.

The attractive theory: `pe_operand_string` renders a `${var:-…}` operand with
`expr_to_string`, so a double-quoted operand ought to land in
`joined_form_to_string`'s `DoubleQuoted` arm. Nothing in the suite spelled it
that way; `vm.dv1`–`vm.dv11` cover every `:-` form *except* double quotes.

Before writing it I checked `notes/decisions/`, because the first measurement
looked like a bug: `${x:-"abc"}` yields `"abc"` **with the quotes** while
`${x:-'abc'}` yields `abc`. DD#65 settles it — the operand is exactly a
double-quoted-string *body*, in which `'` and `"` are ordinary literal bytes
that neither strip nor group, and the 2026-09-30 follow-on gave the stripping
hatch to whole-operand single quotes alone. Both measurements are correct and
deliberate. Not a bug. The rule earned its keep.

So `vm.dv12` went in as a plain behaviour spec, and it passes.

**It did not warm the line.** Line 513 is still cold. The same follow-on that
settles the semantics is why: the operand stopped being an
`ExprKind::DoubleQuoted` and became a `Literal` carrying the raw body text.
That also explains the 21225 Literal-only calls — it is not that the other
arms are rare, it is that nothing upstream produces them here.

The spec is worth keeping regardless; it was the one `${var:-…}` spelling with
no coverage, and it pins a semantic DG had to rule on twice. But the comment I
first wrote on it claimed it reached `joined_form_to_string`, and that claim
was false. A wrong comment in a spec is drift of exactly the kind this project
spends its time digging out, so it was corrected in place rather than left to
be believed by whoever reads it next.

The real question — can *any* of the 24 call sites pass a non-`Literal`? — is
filed as `notes/tasks/expr-to-string-only-sees-literal.md` with all 24
enumerated. Most look like name extraction where `validate` guarantees a
literal, which would make this a function that has outgrown its signature
rather than a gap. Journal 439 is the standing warning against concluding that
from coverage alone: check the producer, not the transform you think removes
the form.

## The gate, and a zero that was not a finding

Sweep **8093**, predicted 8093 from the sum of `plan=` before running it.
No regressions. Machinery-capable 248.

New ids came back **empty** where 8 were predicted. Per the standing rule a
count you cannot account for is a finding, so: each of the four ids has
exactly two rows, both `passing`, both at `c6182e4`. The earlier of each pair
is from `mk cov-union` — which runs the full spec suite, and therefore *is* a
sweep that records into `spec.db`. So the ids were already registered and none
were new to this one. Two sweeps at one commit hash is precisely the hazard
AGENTS.md warns about for hash-keyed baselines; the row-order query is immune
by construction, which is why it is the one we use.

## Second pass — `atok_to_text`, `$(waitstatus)`, `from_arith`

Continued down the re-ranked list. Four entries, three outcomes, and two of
them needed no work at all.

### `atok_to_text` (14 cold) — unexercised, not unreachable

The arith diagnostic renderer, cc 47 / cog 1, and the largest single entry in
the ranking. Fourteen cold arms, among them `Num`, `Var`, `Plus` and `LParen`
— the most ordinary tokens in the language, which made it look like a gap.

It has exactly two callers, and enumerating them settles it:

- `expect` renders the token it *wanted*, and is called with `Colon` or
  `RParen`. Those two are warm. Nothing else is reachable from there.
- `parse_prefix_atom`'s fallthrough renders the token it *found*, but only
  reaches it for tokens no earlier arm took. `parse_prefix` consumes the six
  prefix operators, `skip_terminators` eats every `Sep` on the line before the
  match, `parse_expansion_atom` takes `Subscript`/`BareSubscript`/
  `ParamExpansion`, and the match's own earlier arms take `Num`, `Var`,
  `DollarVar`, `LParen`, `BadNum` and `BadLit`.

That accounts for all fourteen, exactly. `Sep` was the one that looked live
until `skip_terminators` turned up three lines above the match.

But this is a property of *these two callers*, not an invariant any pass
enforces — which is the line the repo's rule draws. A third caller is an
ordinary future change, and it would need every one of these arms; making them
ICE would convert a correct renderer into a landmine. So: merely unexercised,
keep the arms, pin them with a unit test. The test asserts each source
spelling and, orthogonally, that no rendering equals `format!("{tok:?}")` —
which is the bug the function was written to fix, SB4020 telling people
`found Amp` for a `&` they had typed.

### `$(waitstatus)` in string position (4 cold) — a real gap

Same shape as `$(wait)`, one level over. `try_nullary_intrinsic` has five
sibling intrinsics and `waitstatus` was the only cold one — `getpid`,
`getppid`, `random` and `epochseconds` are all exercised. Every DD#126 §2
spec reads the latch as `int s=$(waitstatus)`, which lowers through
`int_intrinsic_i64` instead.

`126.40` (`echo $(waitstatus)`) and `126.41` (`str r="$(waitstatus)"`) close
it. Both still obey the one-statement latch rule, so the read sits directly
under the `- wait` that produced it.

### `from_arith` (5 cold) — unreachable, and the ICE is in the caller

`CheckedArithOp::from_arith`'s five `None` arms are cold because both callers
invoke it *inside* an `Add | Sub | Mul` match arm and then
`.expect("checked op by arm guard")`. Unreachable by construction.

Converting them would be wrong: `None` is the correct, documented answer —
the doc comment explains per-op why each cannot overflow — and the function is
a total classifier, not a walker making an unverified claim. What is worth
having is the contract the `.expect()` leans on, asserted from the other side.
The unit test pins `Some` for exactly the three overflowing ops and `None` for
the other thirteen. It is orthogonal to codegen: today that `.expect()` is only
tested by compiling a program that reaches the arm.

Proved both tests capable of failing before trusting them — `Pow => Some(Mul)`
makes the second one fail with the message naming the contract.

### Already classified, no work

`build_function_pipeline_stage` and `lower_one_pipeline_stage` (5 each) are
the compound-pipeline stage and the pipeline recursion diagnostic — SB4022 and
SB5037, both named in AGENTS.md as validate-shadowed. No spec can warm them.

### Gate

Sweep **8097**, predicted 8097 beforehand. No regressions, machinery 248,
4 new ids — and this time the prediction of *that* was wrong: I expected 2,
reasoning that the native pair already had rows from a standalone run.
`spec/common.sh` contains no `sqlite3` at all; recording is entirely
`run.sh`'s, so a standalone run never registers anything. Which also
retro-confirms the earlier two-rows-per-id accounting.
