---
title: "`$(match S P arr)` bare reports SB4045 (\"expects 2 arguments\") when the 3-arg statement form is meant"
status: open
priority: P4
kind: diagnostic
depends: []
gates:
  - "the diagnostic for a bare `$(match S P arr)` names the real problem"
---

# Correction — the original filing was wrong

Filed 2026-09-29 as "the 3-argument statement form is legal everywhere but
inside `$( )`". **It is not.** Measured the same day:

| position | `match S P m` |
|---|---|
| top level, status unconsumed | SB4018 |
| top level, `\|\| :` | accepted |
| function body, `\|\| :` | accepted |
| `if match S P m; then …` | accepted |
| **`$(match S P m \|\| :)`** | **accepted** |
| **`$(if match S P m; then :; fi)`** | **accepted** |
| `$(match S P m)` bare | SB4045 |

So the statement form works inside `$( )`, exactly as DD#86 says it should.
There is no legality gap. `matchall` behaves the same way and was never
affected at all — it reports SB4018 in every position including `$( )`.

I asserted the gap from one probe of the bare form and generalised it without
checking the consumed forms. That is the failure mode journal 269 recorded
against nine of nineteen task filings, repeated.

# What is actually left

Only the *wording* of the bare case. `check_cmdsub_arity` (validate.rs:560)
looks up `value_intrinsic_arity("match")`, gets the single number 2, and says:

```
`match` expects 2 arguments but got 3 — a value intrinsic must be
called with its exact arity
```

`match` has two forms, and that sentence denies one of them exists. Everywhere
else the same statement earns SB4018 ("result is unused — consume it with
if/`&&`/`||`"), which is both true and actionable; here the author is told
their third argument is illegal when the real problem is the unconsumed
status.

Arguably the bare form *should* stay rejected — `$(match S P)` captures the
matched text and `$(match S P m)` would capture empty, so a one-token
difference silently changing the result is a footgun worth a diagnostic. The
question is only which diagnostic.

# Where

- `sash/src/validate.rs:560` — `check_cmdsub_arity`, keyed on
  `ExprKind::CommandSubstitution` + a single-number arity
- `sash/src/ssa.rs:1426` — `value_intrinsic_arity`, which has no way to say
  "this name also has a statement form"
