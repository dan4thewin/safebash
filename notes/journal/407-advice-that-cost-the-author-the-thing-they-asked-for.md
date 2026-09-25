# 407 — Advice that cost the author the thing they asked for

Back to the coverage queue. `coverage.db` predated three commits that
renumbered `parser.rs` and `validate.rs`, so it was invalid by construction —
it is keyed by `(file, line)` with no commit recorded. Rebuilt it first
(`mk cov-union`, `mk cov-db`, `mk cov-reset`): 538 uncovered lines against
`56cb1f2`.

The queue has flattened — the top entries are now 4-6 lines each, mostly cold
diagnostics. Four looked worth probing. Three turned out to be reachable and
merely untested; the fourth was a bug.

## `${x:!~}` was advised into a second error

`elide_advice` had a branch for `:!` that coverage said was never reached.
The obvious reading was that `:!` in a value position should be SB5046 and
wasn't — DD#91's body says exactly that: *"`check_elide_pexp` treats `:!` like
`:~` (SB5046 outside a list)"*.

Reading the whole DD saved me from "fixing" deliberate behaviour. A 2026-09-09
**addendum supersedes** that rule: `!` was split out of the operator into a
flag, so `${x:!}` is "bare, but trusted" and is legal wherever a bare
expansion is. *"SB5046 now keys on the elide, so it fires for `:~` and `:!~`
and not for `:!`."* The branch is stale code, not a missing check.

But checking `:!~` — the spelling that *should* fire — turned up the real
fault. `check_elide_pexp` builds its message from `pe.operator` alone, and
after the split the `!` lives in `pe.taint_off`. So:

```
$ cat p.sash
str x=$(getenv HOME)
str y=${x:!~}
SB5046: `${x:~}` (elide) is only valid in a VARIADIC list position …
        use `${x:-DEFAULT}` for an empty/default value.
```

Two faults in one message. It quotes `${x:~}` at someone who wrote `${x:!~}`
— a different expansion. And the advice drops the `!`, so an author who
follows it lands on **SB5020**: the taint-off was the whole point of the
spelling they chose, and the advice is what removed it. The form that works,
`${x:!-D}`, is never mentioned.

I verified the chain rather than asserting it — wrote all three programs and
ran them. `${x:!~}` → SB5046; `${x:-D}` → SB5020; `${x:!-D}` → accepted.

The fix renders the sigil from operator **plus** flag (`elide_sigil`) and
keys the advice on `taint_off`. The dead `Some(":!")` arm went with it; the
parser strips `:!` into the flag at `parse/expr.rs:1337`, so `pe.operator`
can never hold `":!"` and the arm could never have matched.

## The cheapest guard is the one you keep tripping

The first probe of this whole thread returned SB5050 (dead variable) instead
of the behaviour I was after, and then my own new spec `84.35` failed for the
identical reason — `str y=…` with nothing reading `y`. The note says to make
every probe read what it writes. It applies to specs too, not just probes.

## Two cold diagnostics that were only untested

Neither is a bug; both are genuine spec gaps, now closed.

**SB4014 "expected operand in test expression"** (`parse_test_operand`).
`[[ -n ]]`, `[[ ]]`, `[[ $x == ]]` all reach it. The suite had never written a
test with a missing operand. Three specs, `ow.24`-`ow.26`. Worth noting it is
a *different* SB4014 from the "expected `]]`" one DD#118 added last session,
so the specs assert the message text, not just the code.

**SB4018 on a bare `! X`** (`check_negation_must_consume`). This one took a
detour. There are two SB4018 sites with near-identical text, and the suite's
nine SB4018 specs all cover the *other* one (`check_boolean_must_consume`, for
`[[ ]]`/`match`/`matchall`). My first probe of a negated pipeline hit SB4040
instead — `!` may only negate a pipeline's *final* stage — and
`echo hi | ! cat` bare is accepted, because `append_pipeline_stage` unwraps
that into a per-segment `Negated` marker and leaves the outer pipeline's
`negated` flag false. So the reaching shape is the plain one, `! true`, and
the suite had simply never written it bare. Three specs, `e.b70.nbare/nif/ndash`.

## Left alone, deliberately

`check_tainted_array` (`ssa_taint.rs:552`) is cold because it is a backstop.
DD#91 says so directly: the SSA pass *"does not yet check these argument
positions — task `ssa-taint-full-backstop` — so the flags are
forward-compatible correctness; the CST gate enforces the bypass today."* A
defence-in-depth check that the authoritative gate shadows is supposed to be
cold. Not a finding.

## Gates

6340 specs, zero failures, exactly the predicted total (6320 + 10×2). Zero
flips. `mk test` 59/27/20, clippy and fmt clean. Complexity unchanged at the
means (cc 3.85 / cog 1.87); `check_elide_pexp` 5/4. Non-vacuity by revert: two
specs fail without the fix.
