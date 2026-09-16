# 332 — the fifth time the comment was the bug

Bug #12, found the same way as #10 and #11: not by the coverage *number*, but
by the work of collecting behaviour for a cold branch.

`validate.rs:1849` and `:1853` were cold — both arms of
`attest_arith_near_miss`, the helper behind SB5054. To write a spec I needed
inputs that reach it, which meant enumerating attest shapes carrying a
`(( … ))` predicate. The scalar ones were already specced (93.42, 93.43). The
attest-all ones were not, so I tried one:

```sash
map m=([k]=v)
attest k v in m (( 1 ))
/bin/echo ${m[k]}
```

It compiled. Then:

```
sash: bug: bare command passed to sash_exec (expected absolute path)
```

A `sash: bug:` assertion in the output of a program the compiler had just
accepted — the same class as bug #9, reached by a completely different route.

## The hole

`classify_attest` dispatches on the last arg. An `Arithmetic` last arg goes to
`classify_attest_convert`, which requires `let [only] = lead` — a one-name
lead. A attest-all lead is two or three names, so it returns `None`, and
`classify_attest` returns `None`.

`None` is not an error anywhere. Every attest-specific pass is written as
`let Some(...) = classify_attest(...) else { return }` — type check, taint,
transform, scope collection. All of them politely declined. What was left was
a `Command` whose name is the literal `attest`, which resolution handled by
doing nothing (it is not on PATH, it is not a builtin, it is not an absolute
path) and codegen dutifully emitted an exec for.

The parser is where it gets in. `parse_attest` parses the lead first, then
checks for `[[`, `((`, or a bare unary operator. The `((` arm carries a
comment saying the form is "scalar-only" — but it runs regardless of what the
lead turned out to be.

## The tell, for the fifth time

```rust
// Scalar lead only — `attest x (( … ))`; anything else is a different error.
let [only] = &c.args[..c.args.len() - 1] else {
    return None;
};
```

"anything else is a different error." There was no different error. The
comment asserts the existence of a check that does not exist, and because it
reads like a hand-off it stops the reader from looking for one.

That is now five for five this month:

| DD | comment described | code handled |
|----|-------------------|--------------|
| #72 | numeric literals are rejected | only in arithmetic context |
| #64 | line continuations are handled | two of three scanner contexts |
| #111 | value intrinsics are exempt inside `$( )` | any body, not just the rewritten one |
| #112 | the `arg` index is checked | the sign, not the digits |
| #113 | a non-scalar lead is "a different error" | no error at all |

The pattern is specific enough to grep for. A comment that *delegates* —
"handled elsewhere", "a different error", "caught by X" — next to a `return
None` / `else` / early-exit, is a claim about code you cannot see from where
you are standing. Four of these five were exactly that shape.

I have started reading those comments as questions rather than statements.
Not one of the five was written dishonestly; each was true when written, and
each was left behind by a change somewhere else. Which is the argument for
verifying them mechanically rather than trusting them — a comment cannot fail
a build.

## Fix

`attest_arith_near_miss` now returns an enum instead of `Option<&str>`, so
every lead arity reaches a diagnostic. The attest-all case gets SB5054 with
its own wording — telling someone whose lead is `k v in m` to "make EXPR read
$k" would be advice they cannot take.

The assignment spelling (`attest e in a (( t = e ))`) *was* caught before, but
by SB4020 — "an assignment inside a non-statement arithmetic position" —
which is true, unhelpful, and coincidental. It now reports SB5054 as well.

Six specs, 93.49–93.54, including one that pins the *message* rather than the
code: if someone later merges the two SB5054 messages back together, 93.50
fails.

## Numbers

- Sweep **5071** passing / 10 skipped / 0 failing — exactly the predicted
  total (5059 + 2×6).
- Units 83 green.
- Branch coverage re-measured before this fix: **456 → 444 cold**, from the 23
  specs of the previous two batches.
