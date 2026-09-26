# 415 — A target must prove itself

## Where this started

Not with redirects at all. DG's directive was that sash should have **no
magical variables** — every `$X` punctuation form ought to be *sugar* for a
proper intrinsic, not the other way round. `$#` is already `$(len args)`;
`$(getcpid)` was being rewritten *to* `$!`, which is backwards.

I inventoried the magic by measurement rather than archaeology, probing the
live binary. Only **three** magic variables actually exist: `$?`, `$!`, `$$`.
Everything else already follows the principle — `$0`/`$1` are SB5017/SB5024
directing to `$(arg N)`, top-level `$#` is SB5024 directing to `$(argc)`, and
`$LINENO`/`$RANDOM`/`$PPID`/`$SECONDS`/`$@`/`$*` are all rejected outright.

The codebase already *states* DG's principle. SB5012 reads:

> `$(status)` is not available here — no preceding command (**the punctuation
> sugar `$?`** reads the same value)

The design was written down. Only the implementation ran backwards.

## The collision

Step A — desugar `$$` to `$(getpid)` — worked everywhere except one place.
`> $$` broke, because the target became a `CommandSubstitution` and
`redirect_target_is_scalar` rejects those at parse (SB4049), before the
existing spill machinery can run.

I offered DG three options and then a fourth. He took none of them:

> what if we require any variable that is a redirect target to use a
> disposition, save perhaps an observably non-empty const, and excluded direct
> use of commands or intrinsics

This is a better rule than anything I proposed, and it is better for a reason
worth recording. Every option I had offered was aimed at the *syntactic*
problem I had just created. DG's aims at the actual bug class the old rule was
a proxy for: `bug-redirect-nonsimple-target-empty` was an **empty filename**,
not a complicated one.

## DD#119

The rule, stated as a principle rather than a syntax list:

> **A redirect target must be provably non-empty, and string-typed.**

Dispositions are the *proof mechanism* — the only construct in sash that makes
emptiness accountable at the point of use. But the test is **"does the result
prove non-empty"**, not "did you write a disposition". That distinction is
load-bearing: a looser wording would admit `${p:~}` and `${p:-}`, both
dispositions, both proving exactly nothing. The original bug would have walked
back in wearing the remedy's clothes.

DG settled three details. **Array cells are in** — a cell behaves like a string
variable, so it carries the same obligation. **Concat is strict**: every
variable part owes its own proof regardless of literal neighbours, because
`> $p.log` with an empty `p` silently writes a hidden `.log` file. And ints are
**exempt but still need quotes** — non-empty by type, but quoting is how int
becomes string everywhere else in sash.

One wart, recorded in the DD rather than papered over: `lib.rs:387` prints
*every* `TypeError` as SB4030. The int exemption needs `env.var_type`, so the
rule has to live in `type_check.rs`, so all its rejections share that code. A
dedicated SB5xxx would mean either duplicating the type environment into
`validate.rs` or teaching `type_check` per-error codes. Both are larger than
the rule itself, and neither has been decided.

## Three bugs

The rule work turned up three, none of them related to each other.

**1. A native ICE, reachable from ordinary source.** `int p=5; /bin/echo hi >
$p.log` died with `ICE: expected string var, got int`. Three shapes reached it.
The general path was healthy — `echo "$p"`, `str s="$p"` all convert fine — so
this was redirect-specific: `redirect_expr_simple_expansion` called `str_ptr()`
unconditionally. Braced spellings survived only by accident, because the
transform spills `${p}` targets to a scalar temp, which converts on the way.

The comment at `types.rs:19` *claimed* this was unreachable — "not
user-reachable input" — and listed the callers it had audited. `redirects.rs`
was not among them. The invariant was not wrong when written; it was never
re-checked when a caller was added. I corrected the comment along with the
code.

**2. A missing C arm.** `"$n".log` is `Concat[DoubleQuoted, Literal]`, and
`ir_expr_as_str` had no `Concat` arm, so it hit the catch-all ICE.

**3. A silent wrong answer, which is worse.** Behind that arm,
`latest_var_as_str` returned `("\"\"", "0")` for int vars — an empty string —
with a comment deferring the case as "rare enough". Not an ICE. Just wrong
output, quietly.

This one only surfaced because **native 14.59 passed and c:14.59 failed**. That
is the differential oracle doing precisely the job it exists for: neither
backend is trusted, and a divergence localises the fault to one of them. A
single-backend suite would have shipped this.

## Proving it non-vacuous

Three mechanisms changed, so three neuters, one per mechanism — not one per
commit. A single neuter would have proved only part of the change.

| neutered | expected | observed |
|---|---|---|
| the DD#119 proof | the rejection specs fail | 6 failed / 136 passed |
| the native int render | native 14.59 ICEs | 1 failed / 141 passed |
| the C itoa | c:14.59 diverges | 1 failed / 141 passed |

Each neuter failed exactly its own cohort and nothing else.

## The gate did its job too

`redirect_target_proof` landed at cc 17 / cog 9 against a 3× bar of 11.5 / 5.6
— a genuine reject, not a dispatcher false positive (the delta was −8, well
inside the exemption threshold). Splitting the disposition arm into its own
function and extracting the shared cell-read message brought it to 10 / 4, and
had the side benefit of removing a duplicated diagnostic string that the bare
`$a[i]` and braced `${a[i]}` spellings could otherwise have drifted apart on.

## Result

Full sweep **6464** — native 3332, c 3119, skipped 13 — exactly the predicted
total. Zero failing, zero flips against `505b581`. Unit tests 59 / 27 / 20.

## What this unblocked

DD#119 dissolved the problem that produced it. `> $$` is now rejected uniformly
as an undispositioned read, so the magic-variable work needs no carve-out for
redirect targets and no `ExprKind::Intrinsic` node. Whether intrinsics deserve
their own node anyway is a real question with real precedent (`SlurpFile`), but
it no longer has a forcing function, and I have left it explicitly undecided in
the DD rather than settling it by default.

Next: re-apply the `$$` desugar, then `$!`, then `$?` — the last is the largest
step, because the DD#56 handling rules attach to `?`.
