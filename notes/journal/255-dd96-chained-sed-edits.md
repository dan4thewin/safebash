# 255 — comma-chained sed edits (DD#96)

DG: *"I like the sed-like edits, e.g. `$foo =~ s/a/b/`; I want to be able to
chain them... what seems consistent and logical?"* Then, after the survey:
**"yes to comma, yes to both forms, only `s///` (on practical grounds)."**

The interesting part was not the feature. It was what the feature was sitting
on top of.

## The separator answers itself

I laid out five candidates. Comma won on one argument: sash *already* uses `,`
for sequencing — array literals, argument lists, and as of DD#94 the inside of
`(( ))`. Every other candidate would have introduced a separator that means
one thing here and something else three lines up. Juxtaposition was the
tempting one (it's what sed-adjacent DSLs often do) and the worst one: with no
visible separator, a typo'd `/` turns two edits into one long broken edit with
no diagnostic.

Once comma was chosen, the multi-line behavior wasn't a second decision — DD#94
had already settled it two days earlier. *A separator continues while an
operand is owed; a complete item ends at the newline.* Applying that rule to
`,` in a sed chain is not an extension of it, it's just the rule. So:

```sh
v =~ s/one/1/,
     s/two/2/
```

works, and works for the same reason `(( a=1, ⏎ b=2 ))` does. This is what a
consistent language buys you: the second feature costs less than the first.

## The bug underneath

Writing the specs found a silent-drop that predates the chain:

```sh
w = ${v:s/a/1/,s/b/2/g}
```

`split_operator` took everything after the closing `/` and called it *flags*.
Not validated — just stored. So the second edit vanished, and because the
dropped text happened to contain a `g`, the **first** edit quietly became a
gsub. Wrong answer, no diagnostic, and the wrongness was worse than the
omission.

This is the third instance this week of the same shape: journal 252's ICEs,
DD#95's mixed-chain divergence, and now this. Every one was a path that
*accepted* something it should have rejected, and every one was invisible
because no spec pinned the rejection. The lesson keeps re-arriving:

> An unpinned rejection path isn't a gap in coverage. It's an untested branch
> that is free to do anything, including succeed.

`split_sed_edit` now returns the unconsumed rest explicitly rather than
sweeping it into flags. The chain folder consumes it if it's a `,`-led edit and
raises SB4032 if it isn't — which means the chaining feature and the bug fix
are the same code change. That's usually the sign the design was right: the
fix is not a special case bolted on, it's what falls out of representing the
thing properly.

## Scoping down

DG limited chaining to `s///` "on practical grounds," which I read as: don't
design the general case before anyone needs it. A general "chain any modifier"
rule would have to answer what `${x:1:3,:-y}` means — whether the default
applies to the slice or the original — and that's a real question with no
obviously right answer. Substitution is the one edit people actually repeat.
Ship that; leave the door open.

## The debugging detour

The multi-line case failed for an hour on a wrong assumption. My continuation
check was gated on `!scanner.next_is_adjacent()` — reasoning that a chain that
crosses whitespace or a newline is by definition non-adjacent. It isn't: the
newline token starts at exactly the offset the comma ends, so `adj=true`, the
guard never fired, and the newline fell through to `_ => break`.

I spent too long theorizing (including a wrong hypothesis that the lexer might
not emit `Tok::Newline` at all, from a grep that found no *literal occurrence*
in `lexer.rs` — the variant is produced via the logos mapping). Three lines of
`eprintln!` printing the raw text, the adjacency flag, and the token answered it
on the first run.

Note to self, again: when a hypothesis about *what the code sees* is cheap to
observe directly, observe it. Reading more code to infer the value is slower
than printing the value.
