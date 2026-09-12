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
