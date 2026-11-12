title: "Eight hand-written `ArithTest` compensations beside `direct_exprs` calls"
status: next
priority: P3
kind: hardening
depends: []
gates:
  - "`StmtKind::ArithTest` either yields its `Expansion` payloads from `direct_exprs` or the eight compensations are justified in writing"
  - "no caller double-reports — `check_positionals_arith` and `dollar_at_arith` are read individually"
  - "the specs that assert SB codes in a given order still pass unchanged"

# The compensation that eight callers each wrote for themselves

Split out of `getters-still-ending-in-a-wildcard` (journal 456), where
`direct_exprs` was made total. `StmtKind::ArithTest` now has an explicit arm
that yields **nothing**, with a documented reason. This task is whether that
reason should stand.

## The shape

`ArithTest(Box<ArithExpr>)` is the one `StmtKind` variant that owns `Expr`s
without owning `Statement`s — they hang off `ArithExpr::Expansion(Box<Expr>)`.
`direct_exprs` does not surface them. Instead, **eight of its thirteen callers
carry their own `if let StmtKind::ArithTest` block right beside the call**:

`validate.rs` 3959, 4233, 4376, 7621, 8618, 9144, 9706, and `ir::walk_own_exprs`
(276). The other four are covered by separate arith passes.

This is the same duplication journal 436 removed for `SsaValue::child_values()`,
and it is not a live gap — taint (SB5020), `:~` elide (SB5046), positionals
(SB5024) and int defaults were each probed through an `ArithTest` and each
caught, precisely because of these compensations.

## Why it is not a one-line change

Two of the eight are **not** interchangeable with a yielded `Expr`:

- `check_positionals_arith` and `dollar_at_arith` also inspect
  `ArithExpr::DollarVar`, which is not an `Expr` at all. They must keep
  walking the arith tree regardless.
- Both already handle `Expansion(inner)` themselves. If `direct_exprs` started
  yielding it, they would **double-report** until trimmed.

So the work is: yield from `direct_exprs`, then read all eight, delete the ones
that become redundant, and trim the `Expansion` arm out of the two that must
survive — auditing each for double-reporting. Eight validators, and diagnostic
counts are asserted by specs.

## Why it may not be worth it

The eight compensations are visible, and the seam is now stated in the
`direct_exprs` doc comment rather than being a silence. The totality gate holds
the getter either way. Trading eight readable duplications for one seam plus two
trimmed callers is a judgement call, not an obvious win — **DG's call**.

The argument for doing it: a ninth caller written next year will not know to
write the compensation, and nothing will tell them. That is the failure mode
this whole cohort keeps producing.
