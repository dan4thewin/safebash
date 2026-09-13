# 285 — The spec that was pinning the bug

`${#a}` and `${#a[@]}` are different questions in bash. The first is the byte
length of element 0; the second is the element count. `sash-convert` answered
both with `$(len a)`, because the code deliberately stripped the subscript:

```rust
// Strip any subscript (`[@]`/`[*]`/`[i]`) — `len` dispatches
// on the whole collection.
let base = rest.split('[').next().unwrap_or(rest);
```

The comment is true about `len` and false about bash. So `${#a}` on `(x y z)`
converted to a program printing **3** where bash prints **1**.

## The filing had rotted, in the usual direction

It said the scalar case `${#s}` was "correct today only by coincidence". By
today it was not correct at all. DD#75 made `len` collection-only, so the
converter's `$(len s)` on a scalar had become **SB4030** — output that does not
compile. The compiler split `len` from `strlen` and the converter was never
told.

That is the third or fourth filing this month to be wrong in the same
direction: a defect described accurately when filed, then quietly made worse by
a change elsewhere. Re-measuring before starting is not diligence, it is the
only way to know what you are fixing.

## A spec was holding the bug in place

`57.13c` asserted:

```sh
[[ $out == *'$(len arr)'* && $out == *'$(len s)'* ]]
```

Both halves of the conflation, written down as the expectation. The suite was
green the whole time. The bug was not surviving because nobody looked — it was
surviving because a test said it was correct.

I rewrote it rather than deleting it: same id, correct assertion, and a comment
saying what it used to require and why. The id is worth keeping precisely
because it is the one that was wrong.

There is no flip to show for it, either. It passed before and it passes now;
only the meaning changed. The regression query cannot see that, which is worth
remembering when "0 flips" is used as evidence.

## Four rows, one of which needs to know a type

```
${#s}           byte length of a scalar   →  $(strlen $s)
${#a}           byte length of element 0  →  $(strlen ${a[0]})
${#a[@]}/[*]    element count             →  $(len a)
${#a[i]}        byte length of element i  →  $(strlen ${a[i]})
```

The subscript decides every row but the bare one, where bash's meaning depends
on whether the name is a collection. So a pre-pass collects the names bash
declares as one.

The part that made this safe to guess: **a wrong guess cannot pass silently.**
`$a` on a collection is SB5064, `s[0]` on a scalar is SB5068, and both name
the fix in the message. A name declared outside the file is simply absent from
the set and lands on the scalar reading, which is the common case. I would not
have shipped a heuristic here if the failure mode had been a wrong answer.

## Demonstrating the fix found the next bug

The gate is "`${#a}` converts to the length of element 0", and the honest way
to show that is to compile the converted program and run it. It did not
compile — for an unrelated reason:

```
error [SB5055] `a` is given a collection value without a collection
declaration — declare it `array a=(…)`
```

**The converter never introduced the DD#97 keyword.** `declare -a b=(p q)` →
`array b=(p q)` worked, because there was an explicit declaration to rewrite.
But bash declares implicitly — `a=(x y z)`, `a[0]=x`, `mapfile -t a`,
`read -a a` — and that is how most real scripts make an array. Every one of
those converted to a program that fails at the next step of the pipeline.

Three symptoms, one cause, so one fix: walk in source order and introduce the
keyword at the first mention of each name. `a=(x y z)` takes it inline as a
**zero-width insert**, so patches on words inside the literal still apply;
`a[0]=x` and the array-filling commands get `array a` on their own line,
because the keyword has nowhere to go inline.

The round trip now matches bash exactly — `1 3 3 1 5` — where before it
printed `3 3 3 3 3` in principle and in practice printed nothing, because it
never built.

## Fail-first

Ten failures predicted on revert, ten observed, the same ten: `57.13c`,
`57.75`–`.78`, `.80`, `.81`, `.83`, `.84`, `.85`. The three that pass either
way do so by construction — the old code already emitted `$(len a)` for `[@]`,
already left `${##…}` alone, and never added a keyword it could get wrong.

Sweep: **4304 passing, 9 skipped, 0 failing, 0 flips.**
