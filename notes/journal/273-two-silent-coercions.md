title: 273 — two silent coercions, and the question that changed the fix

Two holes closed, both of the same family: the compiler quietly
inventing a string it was never given.

## What I set out to do, and why the list was wrong

status.md's "Next up" named three native items. I checked them against
the live binary instead of trusting the file, and two were already
resolved: `bug-getenv-status-unvalidated` was fixed 2026-09-12 (the
filing says so; `$(getenv)` is now SB4045 and bare `getenv` is SB5073),
and the regex-NUL item had been reclassified as a documented
limitation. The list had aged.

That left DG's decision #3 — string intrinsics must reject int
variables — which had **no task file at all**. It was the real next
item and it was invisible to the ledger.

First measurement, and my first mistake: I probed with `n=42`, which
declares a *string* variable, so `$(toupper $n)` yielding `42` was
correct behaviour and I nearly recorded a non-bug. Re-probing with
`int n=42` showed the real thing — eight intrinsics silently coercing.

## Hole 1 — a string intrinsic on an int

`toupper` `tolower` `strlen` `basename` `dirname` `match` `sub` `gsub`
all accepted an int operand and rendered it as text. `join` and `len`
already rejected a mistyped operand (SB4030), and so did `${n:1:2}` and
an int redirect target — the string intrinsics were the one hole left.

`$(strlen $n)` is the sharpest case. It returns 2 for `int n=42`: the
digit count of a decimal rendering the source never asked for, wearing
the costume of a string-length question.

DG chose the **broad** scope: key on the operand's *type*, not its
spelling. So `$(( … ))`, `$(getpid)` and an `int array` element are
caught too — `$(strlen $(( n * 2 )))` is the same mistake as
`$(strlen $n)`, and a rule that caught only named variables would be an
accident of syntax. `"$n"` stays legal, which is not a loophole: it is
the same explicit conversion the pre-existing substring diagnostic
already recommends. DD#105.

**Where it had to be registered** cost me a build. I added the check to
`check_command`, which is the obvious place, and nothing fired. The IR
dump (`--emit-ir`, earning its keep) confirmed the shape was right, so
the walk was wrong: a command inside `$( )` is reached via
`check_subscript_types` → `check_subscript_in_stmt`, **not**
`check_command`. That is precisely why `join`/`len`/`keys` are
registered in both places — a detail that reads as redundancy until you
skip it.

## Hole 2 — join's separator, and DG's question

Found while probing operand shapes for hole 1:

```sh
echo $(join a "$sep")    # printed  x"$sep"y
```

The separator's *source text*, quotes included. Not a crash; a wrong
answer, which is worse. `SsaValue::ParamExpansion.operand` is a
compile-time `String`, so a dynamic separator had nothing to lower to,
and `strip_word_quotes` fell back to rendering the expression.

I scoped it and brought DG the fork (reject vs. support vs. reject-now-
file-later), with the measurement that rejecting costs nothing: every
`join` in the corpus uses a literal separator and the converter never
emits a dynamic one. DG chose reject-now — and asked:

> does `strip_word_quotes()` have a legitimate use? I would have
> expected a general string interpolation call if there were double
> quotes.

That changed the fix. I had been planning to add a diagnostic and leave
the helper alone, which would have left the source-text fallback in
place for whatever called it next. Instead the helper is gone, replaced
by `ir::literal_word_text` returning **`Option<String>`** with no
fallback arm. A slot that requires a constant can now say *"this is not
one"* instead of inventing one. The bug class is deleted rather than
diagnosed.

Shared between validator and transform so the two cannot drift, with
the transform's `None` arm an `unreachable!` guarded by a stated
invariant — the same shape as the existing arity invariant. And
`check_join_arity` became `check_join_form`, because it now checks two
things and the old name lied. DD#106.

## The bug the spec found

Pinning `$(join a "")` as a spec immediately turned up a **backend
divergence**: native concatenated, `--emit-c` joined with spaces.

`emit_c::arrjoin_separator` keyed on *"is the operand empty"*. That
conflates two different facts — `$(join a)` has **no** separator and
means a space; `$(join a "")` has an **explicitly empty** one and means
concatenate. Native keyed on the operator and was right. A sentinel
that cannot tell "absent" from "empty" is the entire bug, and it is the
same shape as hole 2 one layer down: a representation with no way to
say "nothing here" forces a wrong guess.

This one was worth the detour on its own. It was invisible from the
native side, and it had been sitting in a backend DG has queued as the
*next* work area.

## Verification

Fail-first for all 23 new specs against a stashed pre-fix build:
`jsep.1`-`jsep.3` and `sint.1`-`sint.12` fail, the seven control specs
pass both sides — and `c:jsep.6` failed pre-fix while native passed,
which is how the divergence announced itself.

Sweep **4059 passing, 29 skipped, 0 failing**. Zero flips against
`219a8bc`, zero lost ids, 46 gained = 23 specs × 2 modes.

## Worth remembering

- A stale "next up" list is not a cheap error: two of three items were
  already done, and the one real item was not filed at all. Measure the
  binary, not the note.
- `n=42` is a string. Check what a probe actually declares before
  concluding anything about types.
- When a check silently does nothing, suspect the *walk* before the
  predicate. `--emit-ir` settles the shape question in one command.
- DG's question was about a helper's name and it moved the fix from
  "report the bad case" to "make the bad case unrepresentable". Asking
  what a function is *for* is a better review question than asking
  whether it is correct.
