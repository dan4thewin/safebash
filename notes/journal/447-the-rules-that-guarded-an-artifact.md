# 447 — The rules that guarded an artifact

Journal 446 ended with `:?` taking a literal message and a note that the
archaeology had turned up something larger: the eagerness that `:-`'s rules
were built to contain was never a safety decision. It was pre-existing
branchless-select codegen. DG read that and went one step further than the
filing had dared to propose:

> the `:-` default, let's change it to match the `:?` so eager/side-effects are
> a non-issue.

So all three disposition defaults — `${x:-D}`, `@{a:-D}`, `$(arg N D)` — now
take a **literal** operand, and a `$` in one is SB4070 unless the whole operand
is single-quoted.

## What a literal operand takes off the table

The point is not that expansions in a default were dangerous. It is that a
*conditionally taken* operand raises two questions at once — **when is this
evaluated** and **what may it do** — and a literal raises neither. Every rule
deleted today was an answer to one of those two questions:

| deleted | was answering |
|---|---|
| SB5035 (`$(random)` ban) + 4 helpers | what may it do, given eager eval |
| DD#78's read discipline on `:-` | what may it do, w.r.t. taint |
| `lift_default_value`'s hybrid lowering + `contains_capture` | when is it evaluated |
| specs `lift.4`-`lift.10` | pinning that hybrid |

The taint hole DD#78 Stage 1 closed was real. It is still closed — by the
grammar instead of by a checker. That is the trade worth naming: a checker has
to be total over the IR to be sound (three bugs in three sessions say so), and
a grammar restriction does not.

## Two implementations of one rule, on purpose

The `${ }` and brace-less forms are rejected in the **parser**, textually, on
the operand's raw bytes — `text.contains('$')`. That is DG's spec read
literally: "reject any `$` unless wrapped in single quotes." It means `a$%b` is
rejected even though `$%` expands to nothing, which is the right side to err
on.

`$(arg N DEFAULT)` could not be done there. Nothing knows a word belongs to
`arg` until validate, so that form is checked **structurally** instead, by
walking the parsed expression for any expansion (`first_expansion`, total over
`ExprKind`, marked for the totality gate). The single-quote escape falls out
for free: a `SingleQuoted` node carries no expansion to find. Same code,
SB4070, because it is the same rule — the seam is in how it is detected, not
in what is legal.

## The specs mostly moved rather than died

65 ids failed on the first sweep, exactly the predicted cohort. Most were not
testing the default *mechanism* — they used `${x:-$t}` as the cheapest way to
build a tainted value in an awkward position, and needed a new vehicle:

- `bkstp.5`/`7`/`11`/`12` (taint_switch.sh) used `@{a:-$t}` to get a tainted
  element into an array inside a `$( )` body so only the SSA backstop could see
  it. SB4070 is a *parse* error, so it fires even under `--no-taint-cst` and
  the vehicle was unusable. `array b=( $t @{a:-z} )` splices the tainted
  element directly and keeps the splat (with a literal default) purely to keep
  `a` read — otherwise SB5050 fires first and masks the backstop. All four
  distinctions preserved, including bkstp.7's point that `!` scopes to its own
  subject and not to the word beside it.
- `cov.223`/`224`/`225` exist to reach the interpolating-word scanner's
  identifier arms (multi-char, `_`-led, single-char special). Those arms are
  still live — for `:s/RE/SUB/`, which is a **pattern**, evaluated
  unconditionally, so neither question above arises and it stays an
  interpolating word by design. Moving the vehicle to `:s/` reaches the same
  arms; `$#` there even yields the same SB5024 the old spec asserted.

Only `lift.4`-`lift.10` were genuinely about the removed machinery, and they
went with it. The sweep announced exactly those 14 retirements (7 × 2 modes)
and landed on 7023, matching the source-derived prediction to the unit.

## The loss, recorded

A literal `}` in a default is now unrepresentable. `$'a\x7db'` contains a `$`
so it is SB4070, and DD#65's brace scanner is pure depth-matching so quoting
does not protect a `}` either (`${y:-'a}b'}` terminates at the first one). Use
a conditional. This was already true of `:?`; it now extends to `:-`. `vm.dv2`
pins it so it cannot be rediscovered as a bug.

## What I take from the arc

The filing I wrote yesterday (`q-sb5035-random-ban-vestigial`) asked whether
SB5035 "should either acquire a reason that is true today or go." That was the
right question one level too shallow. The ban's stated reason had expired, yes
— but the thing it described, eager materialization, was itself an
implementation artifact that nobody had chosen for safety. Two rules and a
hybrid lowering had accumulated around it, each defensible on its own terms,
none of them load-bearing once the artifact's reach was removed.

When a rule exists to contain an implementation artifact, check whether the
artifact needs its reach before deciding whether the rule earns its place.
