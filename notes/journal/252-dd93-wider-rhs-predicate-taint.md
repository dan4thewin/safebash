# 252 — DD#93 Part D addendum: wider arith RHS, and predicate taint

**Branch:** `hand-parser`. Follows journal 250 (Part D) — DG found a bug in it
the same day.

## The report

DG: *"this gives SB5031: `x=$(printf 123); attest x ((y=x*2)); echo $y`"*.

SB5031 is "assigned nowhere", about `y` — a confusing message for what is
really "your attest form was not recognised". Digging in, it was worse than a
bad message: with `y` actually *used*, the same line **panicked**:

```
thread 'main' panicked at sash/src/transform/expr_handlers.rs:362:48:
internal error: entered unreachable code: arith mutation in a value context is
rejected by validate (SB4020) or hoisted before reaching the value path
```

A user-reachable ICE, introduced by me in Part D that morning.

**Cause.** The SB4020 exemption I added keyed on "an attest whose last arg is
arithmetic" — it never checked that the arg actually *decoded* as the Part D
form. So a near-miss was exempted from SB4020, failed to classify, fell through
to the generic-command path, and reached the value lowering as a mutation,
where an `unreachable!` fires. Where it did not ICE it was a silent no-op: the
attest neither cleared the source's taint nor bound the target, so the user got
SB5031/SB5020 about names that were never the problem.

My own comment in `classify_attest` claimed "validate reports the near-misses".
It did not; I never wrote that check. Writing the claim and not the code is the
part worth remembering.

## What DG decided

Rather than just fixing the diagnostic, the report opened a design question I
put to him: *should* `(( y = x*2 ))` work? It reads like something a user would
naturally write, and the DD's restriction ("bound clean to the i64 value of
numstr") was a narrower reading than the gate actually requires.

**Wider RHS, with a reference rule.** Computing `x*2` still requires converting
`x`, so the `i?` gate proves exactly what it proves for `(( y = x ))`. The RHS
may therefore be any expression that **references** the attested name; one that
never mentions it proves nothing and is SB5054.

**The taint wrinkle, and the tightening.** I flagged that a wider RHS can read
*other* variables, and that attest predicates were not taint-checked at all —
`taint_command` had an explicit `return; // Don't check args of attest`. DG
asked for the explanation before deciding, which was the right instinct,
because the two forms differ:

- a `[[ ]]` predicate yields a **boolean**, so an unvouched tainted read is
  bounded influence over the gate;
- an arith predicate yields a **value** bound to the clean target, so an
  unchecked read launders the data outright — `attest x (( y = 0*x + z ))`
  would hand you tainted `z` in a clean `y`.

DG took the stricter line for both (option C): *even one bit of influence over
a declassification should not be free.* So a predicate may read the attested
name — it is tainted by definition, that is what is being vouched for — and
every other read must be clean.

Blast radius was checked before committing to it: of every attest predicate in
the suite, all reference only the attested name, and the one that reads
something else (`taint.sh` 24.16, `attest x [[ $(echo yes) = yes ]]`) is a
negative test that was already rejected. The sweep confirmed it: 0 regressions.

## Implementation

- `AttestPredicate::ArithConvert` now carries `value: &ArithExpr`; the
  transform binds that expression rather than re-synthesising a bare var read.
- `walk_arith_var_reads` moved from `type_check` to `ir` (it is a general IR
  walk, and three passes now want it) — the reference rule, the taint rule and
  the dead-variable rule are all expressed with it.
- `check_attest_predicate_reads` in validate, plus `walk_test_operands`; the
  exemption set is the attested name or the attest-all binders.
- The SB4020/SB4030 exemption now requires the predicate to actually classify,
  which is the ICE fix.
- Dead-var analysis had to learn the RHS's other reads too, or `k` in
  `(( y = x + k ))` looks like a stray write (SB5050). Caught by spec 93.44.
- janet emits the expression via `arith_expr_to_janet`, keeping `_s` only for
  the failure message.

## Verification

3-mode sweep: **0 regressions**, +26 passing (4034 → 4060; failing unchanged at
33). specs `taint.sh` 93.40–93.48, including DG's report verbatim as 93.48, the
laundering case (93.45), the `[[ ]]` tightening (93.46) and its clean-operand
counterpart (93.47). Nine specs written before the fix; two guards (93.47,
93.48) passed immediately and pinned what must not change.

## Note to self

The near-miss existed because I wrote a restriction into `classify_attest`
("only the plain conversion") and documented the consequence as someone else's
job. Two lessons: a comment that says "validate reports X" is a promise that
needs a test, and a rejection path deserves the same spec coverage as an
acceptance path — every one of these would have been caught by a single spec
asserting a diagnostic for the near-miss.
