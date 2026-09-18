---
title: The gate had been counting ghosts
date: 2026-09-18
---

# The gate had been counting ghosts

Sixteen specs this round and one line of code deleted. The code is the
least of it.

## Three cold spots in the parser, none of them dead

The branch queue reached `parse/expr.rs`. Four functions there carried
cold branches, and the question each time is the same one: what can
reach this? Answered by execution, never by reading the call site.

`absorb_text` first. Three guards — `Amp`, `AndAnd`, `OrOr` under
`AbsorbMode::Assign` — each with a true branch at 4 and a false branch
at 0. The reason turned out to be upstream: `parse_word_parts` breaks
out the moment the next token is not *adjacent*, so a `&&` with spaces
around it never arrives at `absorb_text` at all. Every pre-existing
spec writes `&&` with spaces. Only `a=foo&&bar` — no spaces — reaches
the guard, and DD#71 lines 74-78 already decided what it means: the
literal value `foo&&bar`, a deliberate divergence from bash. Decided,
implemented, and completely unspecced. Ten specs, `ow.14`-`ow.23`.

The first probe round for that was wasted, and worth recording why. I
wrote the tests as bare `[[ … ]]`, which sash rejects with SB4018
because a bare test result is unused. I had measured my own syntax
error and briefly believed I had found a `&&` rejection. **Copy the
spec syntax; do not invent it.** That is the second time this stretch.

Then `dynamic_subscript_var`, which shortcuts a subscript index spelled
like an identifier. Its "first character is neither a letter nor `_`"
branch had never been taken, and neither had the `None` arm of the
caller that depends on it. `${arr[$1]}` and `${arr[$#]}` take it —
both match bash exactly. The non-brace `$arr[foo]` forms take the
other decline path and reject SB5064, where bash would print `p[foo`;
that divergence is DG's decision B from journal 115, checked before I
called anything a bug.

## A guard that could not decide anything

`try_at_transform` recognises `${var@X}`. Its condition ended:

```rust
&& !rest.contains("[@]")
&& !rest.contains("[@")
```

Both false branches cold. I instrumented the function rather than
reason about it, and the instrumentation was the whole answer: `rest`
is the *post-name* remainder, normally `"@U"`. But an unterminated
subscript leaves the bracket in: `${arr[@U}` gives `rest = "[@U"`,
whose last `@` is followed by a lone letter. So the `[@` test is live
— it is the only thing stopping that from reading as an `@U`
transform. Reachable, untested, now `cov.326`.

The `[@]` test above it is another matter. `"[@]"` contains `"[@"`.
So `rest.contains("[@]")` implies `rest.contains("[@")`, and
`!A && !B` where `A ⟹ B` is just `!B`. The line could never decide an
outcome the next line did not already decide. Not cold because it was
hard to reach — cold because it was incapable of mattering. Deleted,
with the reasoning left in the doc comment so nobody restores it.

That is a distinct category from the ICE guards we keep. An ICE guard
is reachable and load-bearing; this was a tautology.

## The gate had been counting ghosts

Sixteen specs, all dual-mode, so +32. The sweep came back green at
5585. I had predicted 5583.

Two. The rule this stretch established is that a total you cannot
account for is a finding, so I chased it, and the first two answers
were both wrong. `64.18` looked duplicated in both modes — an artifact
of slicing `runs` by a fixed rowid window, since `64.18` is the last
spec in the last file alphabetically and my window kept straddling the
previous sweep. Then no file exceeded its own `plan` when run alone.

The answer was in a caveat I wrote into AGENTS.md myself, one commit
earlier, and then failed to apply: `test_status` is
`tests LEFT JOIN runs` over a **permanent** registry, so a deleted
spec reports its last status forever. What I had missed is that
`run.sh` *prints its summary from that view*. The number the gate
reports is subject to the very fault I had documented.

The ghosts were `c:75.99` and `native:75.99` — the duplicate spec I
deleted last commit, last genuinely run at `5cfeb24`, still counted as
passing in both modes. 5585 reported, 5583 executed, and my prediction
had been right all along.

So the corrected ground truth is the sum of every file's `plan=`,
which is trustworthy only because `plan` is itself checked against each
file's real `chk` count. Both recipes are now in AGENTS.md, along with
the retirement rule: after deleting a spec, the summary stays high by
one per mode, permanently, and you subtract it deliberately.

There is a residual question for DG here. The summary over-reports the
gate, and it will drift further every time a spec is retired. Options
are in the status block; I have not touched `run.sh`.

## A stale binary nearly cost an afternoon

Running a spec file standalone, three pre-existing specs failed and 46
failed in `coverage.sh`. Nothing I had changed could explain it. The
cause: `common.sh` invokes `${SASH:-sash}`, and only `run.sh` puts
`target/release` on `PATH`. A standalone run therefore tests
`~/.local/bin/sash` — five days old on this machine. That is the same
shape as the coverage-harness faults in the notes: a thing that
silently measures the wrong artifact and still prints a clean-looking
result. Documented.

**Sweep 5583 executed, 0 failing. Units 77, clippy clean.**
