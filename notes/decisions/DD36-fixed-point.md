title: DD#36 — Fixed-point arithmetic via dtoi/itod
status: superseded

1. No current requirements.

---

**SUPERSEDED 2026-09-13 (DG): `dtoi`/`itod` are YAGNI and have been removed.**

~~The original decision: `dtoi(digits, string) → int` and
`itod(digits, int) → string`. Explicit scale, no silent truncation.
Not a float type. Runtime builtins alongside atoi/itoa.~~

## Why it went

The pair shipped as a *primitive* for fixed-point arithmetic, but sash
never grew the thing that would have used them — a scaled-decimal type,
or wrappers carrying a scaling factor through multiply/divide. Without
that, `dtoi`/`itod` were two conversions in search of a caller: nothing
in the codebase, the specs, or any contributed script used them for
arithmetic. DG considered writing a small sash-level fixed-point library
to justify them and rejected it as scope creep.

They were also carrying real defects. Measured 2026-09-13:

| call                | result   | why it is wrong                        |
|---------------------|----------|----------------------------------------|
| `itod 2 hello`      | `0.00`   | value operand silently coerces to 0    |
| `itod $s 42`        | `42.`    | *scale* operand silently coerces to 0  |
| `dtoi 2 $n`         | `4200`   | int operand stringified, then reparsed  |

All three are the recurring "unresolvable value became a plausible
constant" class. Removal dissolved them, and dissolved a queued task
(`dd103-itod-int-only`) to make `itod` integer-only.

## What replaced them

Nothing. `dtoi` and `itod` are now ordinary unknown tokens — SB5011 in
statement position, in `$( )` capture, and in an arithmetic sexpr alike.
There is deliberately **no** removed-builtin table and no bespoke "this
used to exist" diagnostic (DG: "error just as for any other unknown token
in cmd position"). Specs `36.1`/`36.4` pin that; `36.2`/`36.3` retired.

If fixed-point returns, it should return as a *type* with operators, not
as two bare conversions.

## Fallout

Removing them exposed a pre-existing hole: an arithmetic sexpr
`(dtoi 2 3)` had been rejected SB4030 only because `dtoi` was a *known*
string producer. Once unknown, it silently lowered to 0 — as any unknown
callee always had. See **DD#103**, which states the general rule.
