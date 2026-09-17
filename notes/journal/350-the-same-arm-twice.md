# 350 — the same arm, written twice, in two backends

## The tell was a divergence, not a zero

`codegen/values.rs::lower_value_as_int` came up as the top coverage target: 12
cold lines, and all twelve were one arm — `SsaValue::ArithExpr`.

The useful move was not "write a spec that reaches it". It was to check the
**other backend**. The c emitter has the same function with the same arm, and
the spec suite runs both modes over the same programs, so the two should agree
about what is reachable:

| | native | c |
|---|---|---|
| `lower_value_as_int`, `ArithExpr` arm | **0** | **57** |

Zero against 57 on identical input is not "untested". It says the two backends
route the same SSA differently, and one of the routes is dead.

## What it was

Native reaches `ArithExpr` through `lower_value` (51 hits), whose arm does the
whole job itself — fetch the var as an int, `iconst` the operand, `emit_arith_op`
— and then stringifies. So `lower_value_as_int`'s arm never ran: `lower_value`
always got there first.

The **third instance this stretch** of a canonical answer with a narrower
duplicate written beside it (344, 347, 348, now 350). The fix is the same shape
as 348's: make the live path delegate rather than recompute.

```rust
SsaValue::ArithExpr { .. } => {
    let result = lower_value_as_int(value, …);
    emit_int_to_str(builder, module, externs, result)
}
```

One arithmetic lowering. The formerly-dead arm now runs 51 times and
`lower_value_as_int` has **zero** uncovered lines.

## The part worth keeping

The two copies were **not identical**. One passed `exit_status_var` to
`emit_arith_op`; the other passed `None`. That difference had survived because
it makes no difference: `emit_arith_op` threads the parameter straight to
`emit_div_or_mod_with_zero_check`, whose signature names it `_exit_status_var`
and ignores it.

So the parameter was dead along its whole length, and **a dead parameter is
exactly what lets two copies of the same code disagree without consequence.**
Had it ever been wired up, the duplicate would have been quietly wrong in one
of the two places. Purged from both signatures, all four call sites, and the
`Variable` import it was the last user of.

Divide-by-zero still reports `division by zero`; array for-loops — the only
`SsaValue::ArithExpr` producer — still iterate.

## Gate

`lower_value_as_int` was a pre-existing gate violator (cc 12 / cog 7), so
touching it triggered grandfather-on-modify, exactly as designed. Extracting
the `CmdSubst` arm (two nested `if let`s — all of the cognitive load) to
`lower_cmdsubst_as_int` left the dispatcher at **cc 10 / cog 2**: under the bar
and flat, which is what a dispatcher should read as. No bypass.

## Ledger

- `sash/src/codegen/values.rs` — `lower_value` delegates; `lower_cmdsubst_as_int` extracted
- `sash/src/codegen/arithmetic.rs` — dead `exit_status_var` purged from two signatures

Sweep unchanged at **5354 passing / 13 skipped / 0 failing** — correct for a
pure refactor. Units 83. Coverage **96.85% lines / 89.82% branches**.

Branches did not move: the arm was straight-line code, so it was line-region
cold, not branch cold. Worth remembering when picking targets — uncovered
*lines* and uncovered *branches* are different queues, and this session has
been working the first while quoting the second.
