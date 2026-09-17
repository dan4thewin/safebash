# 356 — Two arms a decision orphaned

Four cold branches in `join_intrinsic` sent me looking for a program that
reaches them. Two were ICE guards already doing their job. The other two were
dead — and the same decision had killed a matching pair in `try_len_intrinsic`.

## What was cold

`join_intrinsic` (`transform/expr_handlers.rs:307`) had cold branches at 317,
321, and 330 twice. The first two resolved quickly:

- **317** is the `let … else` on the separator argument, whose else-arm is an
  `unreachable!` guarding SB5074. Cold because validate rejects first.
- **321**'s false outcome reaches the 0-arg `unreachable!` guarding SB5040.
  Same story. `join a ,` is exercised by 36 specs, so the live path is well
  covered; only the impossible one is cold.

**330** was different. The arm is

```rust
ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
    pe.name.clone()
}
```

— the `@arr` splat form of the array operand. No program reaches it, because
**DD#100 Part D removed the splat operand**. `join @a ,` is SB5062 today, and
specs `75.69` and `75.70` assert exactly that. The arm was orphaned by a
decision, and the comment sitting above it —

```rust
// The array arg should be @arr → ParameterExpansion(arrvals)
```

— had survived the decision that made it false. It read as documentation of
the intended shape while describing a form the compiler now rejects.

## Checking the family, not the site

Journal 352 and 353 both cost time by fixing one member of a family and
finding the siblings later. So before touching anything I asked which other
intrinsics resolve a collection name the same way.

`try_len_intrinsic` has **the same dead arm at line 284**, with two cold
branches of its own, orphaned by the same clause of the same DD. `keys` turned
out not to have a separate intrinsic at all — it is handled inline at
`control_flow.rs:590` — so the family is exactly two arms, and both are now
accounted for.

Two other `arrvals` arms are **live** and were deliberately left alone:
`control_flow.rs:574` (the for-in splat) and `expressions.rs:103` (splat as a
command argument). DD#100 Part D removed the splat as an *operand of a
name-taking builtin*, not the splat itself.

## Why guards and not deletion

DG's standing instruction is to purge dead code, with one exception: an ICE
that prevents a regression. This is that exception, and the reason is visible
in what each arm shadows.

Delete the arm in `join_intrinsic` and an `arrvals` expression falls to:

```rust
_ => expr_to_string(arr_expr),
```

which renders it as the source text `"@a"` — and the compiler then joins a
*variable named `@a`* rather than the array. `try_len_intrinsic`'s fallback is
the same shape: strip a leading `$`, look the rest up. Neither crashes.
Neither reports anything. A regression in SB5062 would not fail a test; it
would produce a program that silently measures or joins the wrong thing.

That is precisely the failure mode `join_intrinsic`'s own existing guards were
written against — the two `unreachable!`s at 318 and 326 exist because the
alternative was a silent wrong answer about the separator. Making the operand
arm match them is consistency, not new policy.

So both arms became:

```rust
// INVARIANT: DD#100 Part D removed the splat operand — `len @a` is
// SB5062 before transform, so this arm is unreachable from any
// accepted program. It is kept as a loud guard because the fallback
// below would render `@a` as the literal text "@a" and look *that* up:
// a wrong answer, not a crash.
ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
    unreachable!("len splat operand guaranteed rejected by validate (SB5062)");
}
```

and the stale comment in `join_intrinsic` was rewritten to say what is true
now: the argument is a bare name, it *used to* be able to be `@arr`, and DD#100
Part D is why it no longer can.

## The tests, and the one that lied

DG's ICE policy is that a guard earns a unit test simulating the regression.
Both intrinsics now have one: build the IR the parser can still produce but
validate now rejects, call the intrinsic under `catch_unwind`, and assert the
panic message names SB5062. Asserting on the *text* matters — a test that only
checks "it panicked" would pass if the panic came from somewhere else
entirely, which is the failure the next paragraph describes.

The first version of both tests failed, and the output was worth reading
carefully:

```
panicked at expr_handlers.rs:290:13:     <- the guard, correct
panicked at expr_handlers.rs:837:14:     <- the test itself, wrong
```

Two panics per test. The guard fired exactly as designed; then my assertion
panicked on `.downcast::<String>()`. `unreachable!` with a **literal** message
carries a `&'static str` payload; only the formatting form allocates a
`String`. A test that had merely asserted `is_err()` would have gone green on
the strength of its own bug.

## Result

Sweep **5440, 0 failing** — predicted unchanged, and unchanged, which is the
whole claim being made: converting unreachable code to a guard is
behaviour-neutral by construction, and the sweep is how that gets checked
rather than asserted. **Units 89 → 91.** `cargo fmt`, `clippy -D warnings`,
and the complexity gate all clean.

Four cold branches enter, four leave — two are now *documented* as
unreachable rather than accidentally so, and two carry tests that fail if the
decision that made them unreachable is ever undone.
