# 456 — The copy that never got the fix

Two things landed, and the second was not on the list.

## The diagnostic that denied a form existed

`bug-match-3arg-in-cmdsub-rejected` was a P4 wording task. A bare
`$(match S P ARR)` reported:

> `match` expects 2 arguments but got 3 — a value intrinsic must be called
> with its exact arity

`match` has two forms. That sentence denies one of them exists.

The filing proposed reusing SB4018's advice — consume the status with `||`.
**Measured, that would have been actively wrong.** `r=$(match S P)` captures
the match with `ARR` left empty; `r=$(match S P ARR || :)` captures **empty**
and fills `ARR`. Inverted results, one token apart — and the `|| :` form
compiles *silently*, because `single_cmdsub_command` sees a list rather than a
command and the arity check never runs on it. The proposed repair routes the
author straight into the empty capture with no diagnostic at all.

So the special case had to be `match`-specific, which meant establishing that
`match` really is the only dual-form name rather than assuming it. Probing
every name in `value_intrinsic_arity` and `BuiltinCmd::from_name`:
`sub`/`gsub`/`len`/`strlen`/`keys` are value-only (SB5073 as a bare
statement), and `matchall` is not in `value_intrinsic_arity` at all. One
name, so the case is exhaustive.

Validator guards are keyed on syntax, so the guard's reach mattered too: all
seven positions a `$( )` is legal in report SB4045 identically. No positional
gap. (Two of those probes "failed" for reasons that were my own bugs — `=` for
`==`, and a redeclared array. Worth saying, because a probe that fails for a
reason you did not author is the only kind that is evidence.)

`cmdsub_arity_diag` now names both forms and offers two repairs, **both
verified to compile and run**. `via.21`–`via.26`. Sweep 7195, green.

## The getters, and what working them turned up

`Statement::direct_exprs()` ended in `_ => {}` — six of sixteen `StmtKind`
variants listed. Checking the other ten field by field: nine own only
`Statement`s. `ArithTest(Box<ArithExpr>)` is the exception, because
`ArithExpr::Expansion(Box<Expr>)` means it can own an `Expr`. Confirmed
reachable through `--emit-ir`.

Then the obvious question: is the hole live? Taint, `:~` elide, positionals,
int defaults — all caught. The reason is that **eight of the thirteen callers
carry a hand-written `if let StmtKind::ArithTest` compensation right beside
their `direct_exprs` call**, the same shape journal 436 removed for
`child_values()`. (The task file claims "exactly one consumer". There are
thirteen. It also names a missing lifetime as the blocker; the lifetime is
already there. A filing is a snapshot, and nothing ages it.)

And while reading those eight compensations — **two different functions named
`walk_arith_expansions`**. One in `ir.rs`, canonical. One private in
`type_check.rs`, ending in `_ => {}`.

## The bug

The private copy dropped `ArithExpr::Subscript` and `SubscriptAssign` — the
exact shape fixed in `walk_arith_intrinsic_calls` earlier this session. Its
only consumer is `check_arith_default_literals`, the DD#77 D4 rule that keeps
a non-literal `:-` default out of arithmetic because it would silently `atoi`
to 0.

```sh
int n=1
int array a=(1 2 3)
y=$(( a[${n:-abc}] + 1 ))
```

- native: `panicked at codegen/values.rs:1426: non-integer literal 'abc'
  reached int lowering; type_check (SB4030) should have rejected it`
- `--emit-c`: **exit 0**, emitting `sash_atoi("abc", 3)`

The assertion names the pass that failed. And the C backend does not crash —
it answers. Two backends disagreeing about a program neither should accept is
the thing the differential oracle exists to find, and no spec covered it.

The first probe was confounded: a plain `array` is a *string* array, so
`(( a[i] ))` is rejected as a bare string cell before the default is ever
looked at. `int array` is required to reach it.

`ir.rs::walk_arith_expansion_roots` is already total, and its comment names
this hazard by name — "skipping them let every expr-level check inside `a[…]`
go unrun, e.g. `(( y = a[${x:~}] ))`". Somebody fixed the shared walker for
precisely this. The copy never got it, and nothing connected the two.

The fix is to delete the copy. `_roots`, not `walk_arith_expansions`: the
deleted copy had root-only semantics and the callback does not recurse, so
`_roots` is behaviour-preserving apart from the bug. `d4.8`–`d4.13`, all
asserting `$err != *panicked*` — a panic is nonzero too, and a spec that
checks only the status would let the ICE straight back in.

**Fifth non-total match over an IR enum to be the bug. Second
second-implementation failure**, after the argv builders. The pattern is
sharper than "add totality markers": the marker was *on the original*. What
failed is that a private fork existed at all, so the fix had somewhere to not
reach.

## What `direct_exprs` became

Total, with the nine `Statement`-only variants spelled out and `ArithTest`
given its own arm and a reason. `ArithTest` still yields nothing, and that is
now a stated seam rather than a silence: its children hang off an `ArithExpr`,
and the passes that want them also want `ArithExpr::DollarVar`, which is not
an `Expr` at all — so they must walk the arith tree themselves, and surfacing
the `Expansion` payloads here would double-report for them.

The marker opts it into `contrib/totality-check.py`. I put a wildcard back to
confirm the gate actually fires on this function, and it does:

```
sash/src/ir.rs:246: wildcard arm `_ => {}` in a function that declares
itself total (marker at sash/src/ir.rs:213)
```

A gate that cannot fail is not a gate.

**Left for DG**: whether to go further and have `ArithTest` yield its
`Expansion` payloads, which would let eight compensations be audited and
trimmed but risks double-reporting in the two that also read `DollarVar`.
That is a real refactor across eight validators, not a getter change.
