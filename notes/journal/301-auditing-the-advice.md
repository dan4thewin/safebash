title: 301 — auditing the advice, and grepping the wrong text

Filed `audit-diagnostic-suggested-spellings` at the end of journal 300 and
took it immediately, since it was the highest-priority ready task and I had
just written it. Two instances were known. The audit found a third.

## The new one

`type_check::conversion_hint` appended this to every SB4030 string→int
mismatch:

> — use `$(($var))` to convert string to int

`$(($s))` is **itself SB4030**: "a string→int conversion must carry a
disposition: `${s:-N}` … or `${s:?msg}`". So the compiler rejected an
assignment, told you how to fix it, and rejected the fix — with a message
that already contained the right answer, one code away.

Measuring the actual surface took four tries. `$(( ${s:-0} ))` works.
`$(( ${s:?msg} ))` works. `int n=${s:-0}` — disposition without arithmetic —
does **not**, which I would have guessed wrong; the type check fires before
the disposition can help. The advice has to name both halves, and now does.

The int→string hint next to it was correct. Both used `$var` as a
placeholder, which reads like the user's own variable name; the right-hand
side at that site is an arbitrary expression and the function is not given
its spelling, so a placeholder is honest — but it should look like one.
`VAR` now.

## I grepped the wrong text

My extraction pass produced two confident hits that were wrong, and the way
they were wrong is the interesting part.

`parser.rs:2741` and `validate.rs:5688` both contain, in source:

```rust
"… — use `@{name}` for the splat, or `$(join {name})` for a joined string"
```

`@{name}` is the invalid braced splat — bare `@{a}` is SB5033, only
`@{a:-W}` / `@{a:?W}` / `@{a:~}` exist. I had two instances and a clean
story.

It is a `format!` placeholder. It renders as `@a`. Both messages are correct
and always were.

The lesson generalises past this audit: **the message a user sees exists only
after interpolation, so an audit of message text has to read stderr.** I
re-ran every candidate by triggering the real diagnostic, which is also how I
found that several of my *other* "failures" were my fixtures' fault —
`while readline v` hit SB4021 because I put the redirect on the compound,
`a[i]` in arithmetic needs an `int array`, `map m = @{a:DISP}` needs its
result read. Four of my first thirteen failures were mine, not the
compiler's. Checking a failure before believing it is the whole discipline.

## Compiled clean

Twenty-odd suggestions, all valid: `@{a:-WORD}`, `${x:-WORD}`, `$(strlen $s)`,
`"$v"`, `write`, `while readline v`, `$1..$N`, `$(len n)` / `(len n)`,
`${n[i]}`, `$(join n SEP)`, `@a`, `signal`, `[k]=v`, the three braced-index
forms behind SB4041, absolute `exec` paths, `$(toupper $s)`, `[[ ]]` dispatch,
a separate `(( i++ ))`, capture-then-arithmetic, `map m = @{a:DISP}`, and
`if a; b { … }`.

One bug in twenty-five is a good rate for an audit. It is a bad rate for a
mechanism, which is the point below.

## The gate is not closed

What I did was diligent, not structural. A manual sweep finds today's
instances; the class re-opens the next time DD-something retires a form,
because a message is a string literal and nothing compiles it.

I filed `harness-diagnostic-advice-compiles` with three options and did not
pick one. The option that actually makes this structural — carrying
suggestions as data on `Diagnostic` rather than baking them into prose, so
the harness reads a field instead of parsing English — changes the
`Diagnostic` type, and that is a surface decision. DG's call, not mine.

The specs are the shape worth copying in the meantime: `hint.1`–`hint.6` and
`gs.36`/`gs.37` each assert the message text **and run the form it
recommends**. Asserting text alone is precisely how SB5030 survived being
found, recorded in DD#100, and assigned to a task that closed without it.

## Verification

Fail-first predicted `hint.1`/`hint.4`/`hint.6` and observed exactly those.
`hint.2`/`hint.3`/`hint.5` were predicted to pass *without* the change, since
they only compile forms that already worked, and did.

Sweep: **4493 passing, 9 skipped, 0 failing, 0 flips.** Predicted 4493
(4481 + 6 × 2 modes) before running.
