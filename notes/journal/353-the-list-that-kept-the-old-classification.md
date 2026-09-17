# 353 — The list that kept the old classification

DG, reviewing the SB5079 work: *"strlen should only work on scalars, not
collections."*

I had just written a spec asserting the opposite. `75.88` claimed
`strlen @a` on `array a=(12 345)` gives 6, because that is what it did, and I
had checked it was coherent — it is the byte length of the space-joined
splat — and moved on. Coherent is not the same as intended.

## What it actually was

Every sibling operation rejects a splat there:

| form | before |
|---|---|
| `toupper @a` | SB5070 |
| `tolower @a` | SB5070 |
| `len @a` | SB5062 |
| `join @a` | SB5062 |
| `strlen @a` | **6** |

`is_scalar_operand_intrinsic` in `validate.rs` reads
`sub | gsub | toupper | tolower | basename | dirname | match`. No `strlen`.
Its doc comment explains the omissions: `len`/`join`/`keys` are excluded
because "they take a collection by name, so `@a` there is not a silent
reinterpretation."

That was true of `strlen` too — until DD#102, five days earlier, moved it out
of that family. DD#102 is titled "`strlen` takes a value, like every other
scalar operation" and settles it with DG's own words: *"strlen should behave
like toupper."* `type_check.rs::string_operand_slots` was updated and now
groups `strlen` with `toupper`. The SB5070 list was not.

So `strlen @a` was the one silent join left in the language: it quietly meant
`strlen "$(join a ' ')"`. Arity checked out, because a splat is one
*syntactic* argument — the same mechanism SB5070 was written to catch, in the
one operator SB5070 no longer covered.

**The general form:** a decision that moves an operator between families has
to move it in every list keyed on the old family. Two lists encoded
`strlen`'s family membership. One was updated with the decision, one was not,
and nothing connects them.

That is the same failure as journal 352's, one level up. There the inventory
named the next bug and I fixed one operator at a time; here the decision named
the new family and one list was left in the old one. Both are **an incomplete
application of something already decided** — which is exactly what AGENTS.md
warns about under Decision authority: *apply a stated principle to all its
consequences.* I have now hit it twice in two days from the other side, as the
one leaving consequences unapplied.

## The second defect, found while fixing the first

Adding `strlen` to the list made `strlen @s` — splat of a *scalar* — report
SB5070 instead of SB5079. That is worse, and checking `toupper @s` showed why
it was not my doing:

```
[SB5070] `toupper` takes a string, but `@s` is an array splat — one argument
per element (DD#69), not a joined string. To operate on the joined text write
`$(toupper "$(join s SEP)" …)`
```

`s` is a string. It is not an array splat, and `join s SEP` is itself
rejected. SB5070 fired on anything splat-*shaped* without asking what the
subject was, so for a scalar or an unintroduced name it asserted something
false and prescribed a remedy the compiler rejects. Pre-existing, all eight
operators.

Fixed by gating it: SB5070 now claims a splat only when the subject really is
a collection — a declaration, a bulk-fill target, a DD#100 slot, or `args` —
and otherwise stays quiet so SB5079 takes the scalar case and SB5031 the
unintroduced one. Each of the three now produces exactly one diagnostic, and
it is the right one.

The set is over-approximated program-wide rather than per-scope, which is
sound here because sash forbids shadowing: one name, one meaning, everywhere.

## Cost

One word in a `matches!`, a threaded `&HashSet`, and a four-line predicate.
Specs `75.88` rewritten and `75.90`–`75.93` added. Predicted +8 on a dual-mode
file; got +8. **5404, 0 failing.** Units 83.

The cheap part was the fix. The expensive part was that nothing was going to
find it: the behaviour was self-consistent, the spec suite agreed with it, and
I had personally inspected it the day before and called it coherent. It took
DG reading one sentence of a summary.
