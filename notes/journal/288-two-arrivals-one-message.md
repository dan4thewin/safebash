# 288 — Two arrivals, one message

`a-b=1` reported:

> [SB5011] a command name must be a literal path — a variable or expansion
> can't be resolved at compile time; use the literal path, or `/usr/bin/env`
> for a runtime tool

There is no variable in that line. There is no expansion in it. The
diagnostic named a cause the input does not have, and then prescribed a
remedy — "use the literal path" — for a line that was never trying to run a
command. Same for `9x=1`, `0=1`, `a]b[=1`.

The mechanism is a collision, not a mistake in the message. `check_cmd_in_stmt`
guards on "the command head is not a `Literal`", which was written for the
DD#23 case: `$x foo`, a command chosen at runtime. But a word containing an
unquoted `=` whose target is not a valid identifier is not recognised as an
assignment, so it stays a command word — and the `=` splits it into a
`Concat`. `Concat` is not `Literal`. Two completely different authoring
mistakes arrive at one arm, and the arm could only describe one of them.

So the fix is not better wording. It is telling the arrivals apart before
choosing wording. `malformed_assignment_target` walks the `Concat` and
accumulates literal parts up to the first `Literal("=")`; anything else in
that prefix returns `None`. That predicate is exactly the question worth
asking: *is there an expansion to the left of the `=`?* If there is —
`9x[$i]=1` — the word genuinely does contain an expansion and the original
message fits it. If there is not, the word is a failed assignment and can be
told so, by name:

> [SB5011] `a-b` is not a valid variable name, so this is neither an
> assignment nor a runnable command — a name is letters, digits and `_`, and
> cannot start with a digit

Code stays SB5011. The six existing `aw.*` cases assert the code and
deliberately not the wording, which is what made the wording free to move —
a small vindication of writing assertions at the granularity you actually
mean. The six new ones (`aw.21`–`aw.26`) pin the split itself: three that the
malformed targets are named and that the word "expansion" does *not* appear,
two that both genuine-expansion arrivals keep the original text, and one that
an `=` in an *argument* (`echo a=b`) is still just an argument.

Fail-first predicted 21, 22, 23 failing and 24, 25, 26 passing, and got
exactly that — the negative half of the prediction is the part that shows the
new specs are testing the change rather than the feature.

The general shape, which has come up before and will again: when one error
site serves two populations, the size of the population you didn't consider
is invisible from inside the message. It took an unrelated probe typing
`a-b=1` by accident to see it at all.

Sweep 4342 passing, 9 skipped, 0 failing, 0 flips.
