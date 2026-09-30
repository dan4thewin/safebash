---
title: "`\"@g\"` is silently literal text, while every other quoted-splat spelling is an error"
status: done
priority: P3
kind: question
depends: []
gates: []
---

# Retraction first

Filed 2026-09-30 as `bug-quoted-array-splat-not-a-read`, claiming SB5050's
read-tracking failed to count a quoted splat as a read. **That filing was
wrong in every particular**, and DG caught it in one line: "`@g` is already a
error saying to use join?"

I checked that `for v in "@g"` *compiled* and asserted it "runs correctly,
printing `[a]` `[b]`". I never ran it. It prints `[@g]` — **one** iteration
over the literal string. The claimed output was fabricated.

With that corrected, the rest of the filing collapses:

- "the splat itself works end to end" — false; it is not a splat.
- "two passes disagree about the same word" — false; both agree it is a
  literal. `--emit-ir` shows `"@g"` parses as
  `DoubleQuoted([Literal "@g"])`, never a `ParameterExpansion`.
- "SB5050 is a defect here" — false; `g` genuinely is never read, so the
  dead-variable diagnostic is **correct**.
- `detect_array_iter`'s `DoubleQuoted` arm is not evidence of intent for this
  spelling: it recurses into a `Literal` and returns `None`. (It is live for
  `"$(keys m)"`, which SB5025/SB5026 then reject.)

There is no bug. What remains is a narrow language question.

## The question

Every way of quoting an array splat is diagnosed **except** the `@` spellings:

| spelling | verdict |
|---|---|
| `"@g"` | **silent** — literal text `@g` |
| `"@{g:-x}"` | **silent** — literal text |
| `"${g[@]}"` | SB4038 — retired; "use `@g` for the splat, or `$(join g)`" |
| `"$(keys m)"` | SB5025 / SB5026 |
| `"$(argv)"` | SB5025 |
| `@g` in a scalar operand | SB5070 — "write `$(join g SEP)`" |

SB5026 exists for exactly this mistake — "quoting an array splat is not
allowed — quotes yield a single string; use the bare form (@arr or $(argv))".
Its own advice names `@arr`, yet `"@arr"` is the one spelling it cannot see,
because the parser never produces a splat node inside the quotes.

So an author reaching for "the whole array as one string" who writes `"@g"`
gets silent literal text, while the same intent spelled `"${g[@]}"` gets a
diagnostic that names `join`.

## Against changing it

`"@g"` matches **bash exactly** — bash prints `@g` too. A bash author is not
surprised, and `@` is not an expansion sigil inside quotes by design (the
brace-less disposition rule already says interpolation requires braces). Making
it an error is a deliberate divergence from bash for the sake of catching a
suspected mistake.

Note `"@{g:-x}"` is the more surprising half: the braces *are* present and it
is still literal.

## Options

1. Leave it. Bash-compatible; the other five spellings still catch the mistake.
2. Extend SB5026 to a double-quoted word whose literal text matches `@name` /
   `@{name…}` where `name` is a live collection, advising `$(join g SEP)`.
   Catches the natural spelling; diverges from bash; a literal `@` followed by
   a collection name becomes un-writable without escaping.
3. As 2, but restricted to `"@{…}"` (braces present ⇒ the author clearly meant
   an expansion), leaving bare `"@g"` alone. Narrower divergence.

DG decides. Nothing is broken today; this is about whether a likely mistake
should be caught.

## Resolved 2026-09-30 — DG's call

> "seems like the diagnostic should include the single quoted form to get a
> literal, i.e., '@g'; then we could at least throw an error where a splat is
> obviously wanted like a for loop"

Both halves implemented. Option 2 of the three above, scoped to the for-in
source — which is the narrowest form of it, and the one DG named.

**The escape hatch is real**: `'@g'` single-quoted yields the literal text,
verified by running it. So the rejection always has somewhere to send the
author.

**The rejection is purely syntactic.** `check_for_header_splat` has no symbol
table, so `g` is not required to resolve — and in this position that is the
right call anyway: it is the POSITION that makes the intent obvious, not the
name. A loop over the single constant string `@g` is not something anyone
writes on purpose.

**Scoped to the for-in source only.** As a command argument, `"@g"` remains
ordinary literal text and matches bash exactly. Widening further would be a
much larger divergence with no comparable signal.

**SB5026 reused**, not a new code — the rule is the same one its existing
message already states ("quoting an array splat is not allowed"). That message
now also names `$(join arr SEP)` and the `'@arr'` literal form.

### The advice had to be checked against the compiler

The first version advised writing `@{g:-x}` back for `"@{g:-x}"` — which is
**SB5034**, because a for-in source rejects the empty-disposition (DD#69
addendum). That would have handed the author a form that fails on the next
compile. The advice now strips a disposition (`@g`) but carries over `:!`,
which is a taint MODIFIER and legal here (DD#91). Every advised spelling was
then run: all five compile and produce the right output (spec `gs.61` asserts
the `@{g:-x}` case specifically, including that the bad advice is *absent*).

Specs `gs.59`-`gs.65` in `spec/glob_splat.sh`. Sweep 7107, zero failing.
Journal 452.
