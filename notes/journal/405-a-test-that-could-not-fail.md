# 405 — A test that could not fail, in a shape that looked like a comparison

DG asked a small question: does a single `=` have two meanings?

It does. **Assignment** in statement position, **string equality** in a `[[ ]]`
operator slot. But the positions are disjoint — the parser never has to guess —
so the honest answer was "two meanings, no ambiguity." That could have been the
whole entry.

The ambiguity is not the parser's. It is the reader's, and probing it turned up
a test that cannot fail.

## The symmetric-`=` rule, and what it was hiding

`=` binds as assignment only when it is **symmetric**: glued both sides (`x=y`)
or spaced both sides (`x = y`). Asymmetric `x =y` is not an assignment.

I nearly filed that as a bug. `echo = y` assigns to a variable named `echo` in
sash where bash prints `= y`, which looks exactly like a divergence worth
stopping for. Checking first — `git log -L` on the disambiguator — surfaced
commit `e48f3f0`, which established the rule deliberately so DD#86's
frictionless write (`msg = hi`) would work. Not a bug. The "check whether it's
deliberate before filing" rule earned its place again.

What `e48f3f0` recorded as its caveat was that ``[[ $x =b ]]`` still evaluates
as `= b`. What it did not record is the consequence that matters:

```sh
[[ $x = y ]]   # F
[[ $x=y ]]     # T
```

Same three characters, opposite answers, and the second one is not answering a
question at all. Glued, it is **one word** — `$x=y` — which falls to the
implicit-`-n` rule, and that word contains a literal `=y`, so it is non-empty
for every possible value of `x`. `[[ $x=y ]]` is unconditionally true.

Bash does the same thing. That is precisely why it survived: there was no
divergence to catch, and a differential oracle cannot see a test that agrees
with its reference about being useless.

## Three changes, and the one that does the work

I recommended retiring the implicit form. DG settled more than that:

> erring on the side of clarity, we want the vacuity rule, the `-n` test as
> explicit and not implied, and `==` the only spelling of equality

Separating **vacuity** from **implicitness** turned out to be the useful move,
because they are not the same defect and only one of them is fixed by requiring
`-n`. I checked by execution rather than by reasoning, which was the right
instinct: `[[ -n foo ]]`, `[[ abc == abc ]]` and `[[ -n $x=y ]]` are all still
constants after the implicit form is gone. Requiring `-n` makes the *shape*
honest; it does nothing about the *answer*.

So: **SB4067** (`=` is not a comparison operator), **SB4068** (a bare word is
not a test), **SB5086** (this test's result is fixed at compile time). DD#118.

## Where I was wrong

I proposed scoping the vacuity rule narrowly — `-n`/`-z` only, about three
sites — and leaving constant comparisons like `[[ 1 == 1 ]]` alone, on the
argument that they at least exercise `sash_strcmp` and rewriting ~30 specs
would cost coverage.

DG: *"isn't `[[ foo == foo ]]` just a mechanical rewrite to `s=foo;
[[ $s == foo ]]`?"*

It is. I checked the emitted C: the rewrite produces `sash_strcmp(s_ptr, s_len,
"foo", 3)` and `s` survives as a real variable — there is no const-propagation
of string assignments to collapse it. The primitive is exercised either way. My
objection was wrong on the facts, and the wide rule went in.

That exchange also **forced** a design property I should have derived myself:
if DG's rewrite must keep working, the rule cannot const-propagate. It has to
be **syntactic** — ask what the operand was *written* as, and stop at the first
variable. Not a tradeoff; a consequence.

## Two measurements I had to correct

**The blast radius.** My first count of `=`-in-test sites was 1763. Nearly all
of them were bash *harness* lines — `[[ $rc = 0 ]]` in the assertions — not
sash programs. The real figure was 55. A grep across a test suite cannot tell
the language under test from the language testing it.

**The extraction.** I built a checker that pulled every `run_sash` body out of
the spec files with a regex and ran it through `--emit-ir` (full validation,
~2ms, no link step — a good tool). It reported zero remaining breakage. The
sweep then failed four tests.

The regex stopped at the first `'`, so any body using the `'"'"'` idiom to
embed a quote was silently truncated — and the four casualties all had their
`attest e in f [[ $e = $e ]]` line *after* the truncation point. The fix was to
stop parsing bash and let bash do it: copy the specs to a scratch dir beside a
stub `common.sh` whose `run_sash` dumps `$1` to a file, and source them. 2967
bodies, exactly quoted, and re-running the check found the four and nothing
else.

The lesson is narrower than "don't parse shell with regex". It is that **a
checker that reports zero is making a claim about its own coverage**, and I had
no evidence for that claim until the sweep contradicted it. 2967 extracted
against 2937 literal call sites is the kind of number that can be reconciled;
"zero remaining" is not.

## A diagnostic that got better by being asked to be worse

The first cut of SB4068 broke `[[ a&b == x ]]`, which had been reporting
SB4014 `expected ']]'`. Both readings are defensible — there *is* a bare word
there — but `&` is what actually ended the word, and blaming the word for the
token that interrupted it is a worse diagnosis.

Guarding SB4068 on "the scanner is at a test terminator" fixed it, and it also
restored `[[ -i 42 ]]` (an unknown unary operator) to SB4014. Two spec changes
I had queued turned out not to be needed. Narrowing a new diagnostic's reach
made it *more* accurate rather than less, which is worth remembering the next
time a new error code starts absorbing its neighbours' cases.

## Cost

Forty-four spec sites, all mechanical. One of them is worth naming:
`operand_words.sh`'s `ow.14`-`ow.16` pin that an adjacent `&&` *terminates* a
`=~` pattern, and their own comments explained the result in terms of the
implicit form — `[[ $x =~ a&&b ]]` is `[[ $x =~ a ]] && [[ b ]]`. The specs
were documenting the surface being retired. They now read `a&&-n $y`, which
pins the same structural claim without depending on it.

`sash-convert` needed the matching rewrites, since it was emitting both retired
spellings; bash input is exactly where they come from.

## Left open

`[[ $x==a ]]` answers F in sash and T in bash — `extract_compact_operator`
splits the glued operator. That predates this work and is **not** the same as
the deliberate no-space extension `[[a==a]]`, which bash cannot parse at all
and which `dispatch.sh` specs 28 times. Raised with DG; his call.
