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
