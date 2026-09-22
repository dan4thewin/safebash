# 390 — a narrowing-by-default becomes a rule

Small follow-up to 389. Fixing the eight `argv` runtime ICEs forced a call on
a language-surface question, and I shipped the safe side of it without asking
first: `argv $n a` had never been *rejected*, it had been mislowered, so
SB5085 turned a crash into an error — but "you must write a literal" was a
narrowing by **default**, not an answer. I filed it rather than pretend it was
settled.

**DG ruled: no.** A compile-time-constant fill start is the rule. The task
closes `wontfix`.

He added one thing I had over-thought. I had started splitting SB5085 in two —
a quoted index (`argv "1" a`) is a spelling slip that wants "drop the quotes",
a sigilled one (`argv $n a`) is someone reaching for a feature that wants "here
is how to say it instead" — on the reasoning that 389's whole lesson was advice
being *wrong for the input*. I had already moved `expr_is_dynamic` out of
`transform/expressions.rs` into `ir.rs` to get the predicate where `validate`
could see it.

DG: **the advice can be generic.** And he is right, because the two audiences
are not actually in conflict here. The message states the rule *and* names the
alternatives; the over-quoting user reads the first clause and stops, the `$n`
user reads on. Splitting would have bought a marginally tighter message for a
new recursive predicate and a cross-module move. I reverted both edits — the
tree went back to byte-clean before the reword, which is the check that the
revert was complete.

The lesson is narrower than 389's and worth keeping separate from it: *advice
being wrong for an input* is a bug; *advice being general across inputs* is
not the same failure and does not need the same remedy.

## What the message says now

```
`argv N a`'s index must be a bare non-negative integer literal that fits a
64-bit integer — the fill start is fixed at compile time. For a start that
varies at run time, fill the whole array (`argv a`) and index it
(`${a[$n]}`), or loop over `for x in $(argv)` and skip the ones you do not
want
```

Both alternatives were **measured against the live binary before being
written into the message**, which is not ceremony — my first attempt at them
was rejected outright (SB5020: everything `argv` fills is tainted, so the
advice only works with an `attest` the first draft never mentioned). Advice
that does not compile is the same defect as advice that is wrong.

They are also coherent with each other: `arg 2` and `${a[2]}` name the same
argument, because `argv a` includes argv[0]. And `$(arg $n)` has accepted a
dynamic index all along (`av.t8i`, "a COMPUTED index is a runtime concern, not
a typo"), so only the *slice* half was ever constant-only.

## Also

`struct Case` (`ir.rs`) now carries a doc comment saying it is the DD#66
dispatch form and not bash `case`/`esac`. Resolving that in 389 cost a detour
through the parser to be sure; the comment is so the next reader does not
repeat it.

## Gates

`mk test` 51/27/20. Sweep **6011**, 0 failing — the predicted total, since no
specs were added (`av.f9`'s comment changed, not its assertion). Three id
checks clean.
