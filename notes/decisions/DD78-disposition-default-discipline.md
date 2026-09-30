title: DD#78 — disposition-default discipline (clean, side-effect-free)
status: accepted

A **disposition default** is the fallback/message operand of a `:-`/`:?`
expansion or the new `$(arg N DEFAULT)` (DD#60 addendum). This DD constrains
what that operand may be, and in doing so closes a pre-existing taint hole.
Decided 2026-08-17 (DG), during the `arg N DEFAULT` design.

## The rule

A disposition default must be a **clean, non-`random` value**:

- **Clean** — it may not read a tainted value (a tainted variable, or a
  tainted source: `$(getenv …)`, `$(arg …)`, `$(argv)`, or an external
  `$(cmd …)`). The disposition is an **expansion — a read, not a store** — so
  an operand inside it is a *read position*. Reading a tainted value there is
  rejected (SB5020-class), exactly like `echo $tainted`. A tainted fallback is
  laundered the normal way: capture into a variable and `bless` it first, then
  use the clean variable as the default.

- **Non-`random`** — `$(random)` is *clean* (a compiler intrinsic) but
  **stateful**: it advances the PRNG. Because the default is evaluated
  **eagerly** (see below), a `$(random)` default would advance the PRNG even
  when the fallback is not taken — an observable side effect. It is rejected.
  (`$(getpid)`/`$(epochseconds)` are non-deterministic but *stateless* — an
  eagerly-evaluated-then-discarded timestamp has no side effect, so they are
  allowed.)

Allowed defaults: literals, clean variable/subscript reads, `$(( … ))`, and
clean **stateless** intrinsic substitutions (`$(getcwd)`, `$(getpid)`,
`$(epochseconds)`, `$(len …)`, `$(keys …)`, `$(join …)`, `$(status)`,
`$(signum …)`, user functions).

Rejected (→ capture into a variable and bless, or inline a literal):
`$(getenv X)`, `$(arg N)`, `$(argv)`, external `$(cmd)`, `$(random)`, and any
tainted variable read.

## Why — the taint hole this closes

The taint checker walked the *var* of a `:-`/`:?` disposition but **not its
default operand**. So a tainted default slipped through when the result was
used directly (never stored):

    y=$(getenv HOME); echo ${x:-$y}        # COMPILED (should reject)
    ls ${x:-$(getenv HOME)}                # COMPILED (should reject)
    echo ${x:-$(date)}                     # COMPILED — also ran date eagerly

The store path already propagated correctly (`z=${x:-$y}` taints `z`, and a
later read of `z` is caught), so the leak was direct-use only. Treating the
default as a checked read position closes it uniformly.

Note `z=$y` (direct source→target assignment) stays legal (DD#24), but
`z=${x:-$y}` does **not**: nesting `$y` inside an expansion makes it a read,
not a direct source assignment. This is the same distinction DD#24 already
draws — this DD just extends the checker to see it inside a disposition.

## Eager evaluation

sash evaluates a `:-` default **eagerly** (`emit_default_value` computes both
arms and `select`s — branchless), unlike bash's lazy `:-`. That is why a
side-effecting default is a problem: it runs whether or not the fallback is
taken, diverging from bash for a side-effecting/nondeterministic default. The
clean-and-non-`random` rule makes eager evaluation **safe**: a clean, stateless
default has no observable effect regardless of which arm is chosen, so eager ==
lazy for every legal default. (Keeping eager avoids per-backend branching; the
arith `DefaultValue` lowering, DD#77, is the same shape.)

## Scope

Applies uniformly to every disposition default:

- `${x:-DEFAULT}` — the fallback value (a read).
- `${x:?MESSAGE}` — the abort message (printed to stderr; constrained for
  uniformity and because it is eagerly materialized).
- `$(arg N DEFAULT)` — DD#60 addendum. The `arg` *result* is tainted regardless
  (a source, blessed downstream), so a clean default here is technically
  stricter than taint-correctness requires — but the uniform rule is simpler
  and the cost is negligible (a tainted fallback is captured+blessed first,
  which you would do anyway).
- The DD#77 arith `${s:-N}` default is already **stricter** (a decimal integer
  literal — D4), which trivially satisfies clean + non-`random`; no change.

## See also

- DD#24 — taint quarantine (source-assignment allowed; tainted read rejected).
- DD#60 — argv trio; the `arg N DEFAULT` addendum introduces the source-context
  disposition governed here.
- DD#65 — `${x:-WORD}` default word (the operand this constrains).
- DD#69 — splat disposition `@{a:-WORD}` / `@{a:?WORD}` (same operator family).
- DD#77 — arith string→int disposition (its `:-` default is literal-only).

## Amendment (DG, 2026-09-28) — a capture-bearing default is lowered lazily

The "Eager evaluation" section above rests on a premise that does not hold for
one of the operands it allows. Its argument is: eager is safe *because* every
legal default is side-effect-free, so "eager == lazy for every legal default".
But the allowed list ends with **user functions**, and a user function is not
side-effect-free in any enforced sense — `f() { echo RAN >&2; echo hi; }` is a
legal default whose effect is plainly observable when the fallback is *not*
taken.

The contradiction was never observable, because the compiler **ICEd** on
`${x:-$(userfunc)}`: `lift_int_func_calls` ended in `other => other` and never
descended into a `DefaultValue`, so the capture reached codegen unlifted and
panicked. The allowance was dead letter — no spec in the suite exercised it.
Fixing the ICE (journal 432) made the question live, and DG ruled for bash
parity:

> **b** — lower lazily.

So the rule is now split by whether the operand can have an effect:

- **No capture in the operand** (literals, clean variable/subscript reads,
  `$(( … ))`, stateless intrinsics) — **unchanged, still eager**. These are
  exactly the operands the original reasoning contemplated, `emit_default_value`
  still computes both arms and selects, and no backend branches.
- **A capture in the operand** (`$(userfunc)`, a multi-statement `$( … )` body)
  — lowered as real control flow in *transform*: bind the source to a temp,
  test it, re-bind from the lifted operand in the taken block only, merge
  through a phi. bash parity in both backends, and it also keeps the capture
  off the `DefaultValue` operand that codegen cannot lower.

Because the laziness is built during transform, neither backend grew a special
case — the cost the original section wanted to avoid ("keeping eager avoids
per-backend branching") is not incurred. The clean/non-`random` rule is
untouched: `$(random)` stays rejected, and its justification no longer needs to
lean on eagerness.

Pinned by `spec/functions.sh` `lift.4`-`lift.10`; `lift.6` is the one that
fails if the operand is hoisted (the untaken `f`'s stderr appears).

### Addendum (2026-09-30) — the `${x:?MESSAGE}` scope clause is withdrawn

The Scope section above names the `:?` abort message as a disposition default,
"constrained for uniformity and because it is eagerly materialized". That
clause is withdrawn. The message is a **literal**, and a `$` in it is SB4070
(DD#65, follow-on 2026-09-30); a single-quoted message is literal text. Either
way the operand is never read, so there is nothing for this DD's read
discipline to check and no route by which a tainted value escapes through an
abort message.

This is a *withdrawal*, not a gap left open. Stage 1 of this DD said so
originally — "`:?`/pattern operators carry a literal operand (DD#65), nothing
to check" — and the uniformity argument in Scope was added on top of it without
noticing the two disagreed.

The clause was acted on once, in the belief that this DD settled the parse. It
does not: this DD constrains what an operand may *contain*, DD#65 defines how
it is *spelled*, and the interpolating-word spelling is `:-`'s. Making `:?`
interpolate turned a documented-safe literal into a user-function call site on
both the abort and the **success** path. See DD#65 for the full account.

The lesson generalises past this operator: grouping two forms by purpose in a
DD does not make every rule stated about one of them apply to the other. When a
DD appears to settle a question about a form it merely *mentions*, check which
DD actually owns that question.

The rest of this DD — the clean/non-`random` read discipline on `:-`,
`arg N DEFAULT`, and the laziness amendment — is unaffected.

### Amendment (2026-09-30) — the read discipline dissolves; this DD is spent

The withdrawal above was the first half. DG took the second:

> the `:-` default, let's change it to match the `:?` so eager/side-effects are
> a non-issue.

A disposition default is now a **literal** in all three spellings — `${x:-D}`,
`@{a:-D}`, `$(arg N D)` — and a `$` in one is SB4070 (DD#65, follow-on
2026-09-30). Everything this DD constrains is therefore unreachable by
construction, and the machinery is deleted:

- **The read discipline is gone.** It required an operand to be a clean read.
  An operand cannot be a read at all now, so `check_param_expansion_taint` no
  longer walks one. The real taint hole Stage 1 closed — a tainted value
  reaching a clean sink through a default, with no `read` — is closed by the
  grammar instead of by a checker.
- **SB5035 is gone**, along with `flag_random_default`, `random_default_diag`,
  `expr_invokes_random`, and `cmdsub_head_is`. The `$(random)` ban existed
  because an eagerly-materialized default advanced the PRNG on the untaken
  path; a literal cannot invoke it.
- **The laziness amendment is moot.** `lift_default_value` is a branchless
  select again and `contains_capture` is deleted. This retires DG's own call
  from journal 432 ("**b** — lower lazily"), which was the right answer to the
  question as it stood then: given an interpolating operand, bash parity was
  the correct tiebreak. Removing the operand removes the question.

So Scope's premise — that a default "may contain arbitrary expansions" and must
therefore be constrained — no longer holds for any of the three forms. **This
DD retains no live rule.** It is kept for the account of the taint hole and for
the two errors recorded against it, both of which generalise:

1. **Uniformity written into a Scope section outlived the Stage that
   contradicted it.** Stage 1 said `:?` had "nothing to check"; Scope claimed
   it as a constrained default anyway. The contradiction sat unread until
   someone acted on the wrong half.
2. **Grouping two forms by purpose is not a statement about their grammar.**
   See the withdrawal above.

A third is worth adding now that the arc has closed: the eagerness these rules
were built on was never a safety decision. It was pre-existing
branchless-select codegen, and SB5035 and the read discipline were both
*consequences* of it that then had to be defended on their own terms. When a
rule exists to contain an implementation artifact, the cheaper fix is often to
remove the artifact's reach rather than to keep checking its blast radius.
