title: "`$(match S P arr)` bare reports SB4045 (\"expects 2 arguments\") when the 3-arg statement form is meant"
status: done
priority: P4
kind: diagnostic
depends: []
gates:
  - "the diagnostic for a bare `$(match S P arr)` names the real problem"

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

# Fixed 2026-09-30

`sash/src/validate.rs` — `check_cmdsub_arity`'s `diags.push` moved into a new
`cmdsub_arity_diag`, which special-cases `match` at 3 arguments. Specs
`via.21`-`via.26` in `spec/errors.sh` (`plan=318`→`324`).

## What the measuring changed about the fix

The filing proposed SB4018 ("result is unused — consume it with if/`&&`/`||`")
on the grounds that every other position reports it. **That advice would have
been actively wrong here**, and only running it showed why:

| spelling | captures | fills ARR |
|---|---|---|
| `r=$(match S P)` | `b` | — |
| `r=$(match S P ARR \|\| :)` | **empty** | `b` |

The results are *inverted*, one token apart. Telling the author to consume the
status routes them straight into the empty capture — and it compiles silently,
because `single_cmdsub_command` sees a list rather than a command, so
`check_cmdsub_arity` never runs on it. So the new text names both forms and
offers the two repairs that actually work (`via.23`, `via.24` compile and run
them, so the advice cannot rot).

## Scope, measured rather than assumed

`match` is the **only** name that is both in `value_intrinsic_arity` and legal
as a statement. Probed every candidate in both tables: `sub`, `gsub`, `len`,
`strlen` and `keys` all draw SB5073 as a bare statement (value-only), and
`matchall` is not in `value_intrinsic_arity` at all, so the check returns early
for it. The special case is therefore exhaustive, not a patch on one symptom.

Also checked the guard's reach, since validate.rs guards are keyed on syntax:
all seven positions a `$( )` is legal in (assignment RHS, command arg, `[[ ]]`
operand, array-literal element, subscript index, inside `"…"`, user-fn
argument) report SB4045 identically. No positional gap.

## Left open — not this task

`$(match S P ARR || :)` compiles and captures the empty string. That is
consistent with the semantics (the statement form prints nothing), so it is a
footgun rather than a bug, and making it an error is a behaviour change.
`via.26` pins the current result so a future change to it has to be deliberate.
