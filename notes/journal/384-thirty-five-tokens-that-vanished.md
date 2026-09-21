# 384 — Thirty-five tokens that vanished

DG asked a convention question: with several regression shields now in the
tree, is there a clear way to tell a worthwhile shield from dead code? That
became DD#116, and DD#116's first application found a bug it was not looking
for.

## The test

**Does this code BACKSTOP a decision, or RIVAL it?** A backstop sits
downstream of an authoritative decision and has no opinion of its own. A rival
is a second independent answer to an already-answered question. A rival is
dead code however defensive it looks — and duplicates drift silently.

Two candidates were queued. `arith_brief`'s dead arms were a clean rival: a
third copy of the `x = ...` brief. `tok_to_text`'s four paren arms looked like
another, since `step_arith_collect` renders `(`, `((` and `))` itself before
the default token path ever runs.

That reading was right, and beside the point.

## Proving it, and getting a different answer

DD#116 requires proving unreachability by execution. The four paren arms were
never the risk; the `_ => String::new()` beneath them was. Replacing it with a
`panic!` failed spec `31.51` — so `_` was reachable, and the plan DG had
approved ("delete the four arms, make `_` loud") was already impossible.

An `eprintln!` in the same position had printed **nothing** and left the sweep
fully green, because spec harnesses capture the compiler's stderr into `$err`.
The quiet instrument was the misleading one. `panic!` is the instrument here.

The token turned out to be `Tok::DblSemi`, reaching the collector from a
`for (( ;; ))` header — and being deleted. Which raised the real question:
what *else* falls into `_`?

`tok_to_text` rendered 24 of 59 token kinds. The other 35 returned the empty
string and vanished from the collected text before the arithmetic re-lexer
ever saw them.

```
(( x = 1 done ))     sash: compiles, x=1      bash: syntax error
```

21 of the 22 keywords behaved this way — `if then else elif fi while until do
done for in case esac fn function str int const array map attest` — in both
`(( ))` and `$(( ))`. Only `END` was rejected. So did `[[ ]] { } $(( <<< >&
<& <> &>` and `$'…'`. Trailing garbage was silently accepted because the token
that made it garbage had been deleted first.

DG checked the claim rather than taking it — *aren't `case`, `esac`, `;;` and
`<<<` rejected?* They were not. All four compiled, in both forms.

## Two bad measurements on the way

The first bash differential ran `bash -c "x=1; (( x = 1 $kw )); echo \$x"` and
read the exit status, which is the status of the **`echo`**. It reported bash
accepting all six. Re-run without the trailing command, bash rejects all six
with `syntax error in expression`. A differential whose control command
answers for the thing under test is not a differential.

The second: probes that declared `int n=2` without reading it tripped SB5050,
the dead-variable gate, and the rejection looked like the behaviour under
test. That one is a standing hazard in this codebase and it caught me again.

## The fix

Not a deletion — a totality. `tok_to_text` is a total map from token to text
and it was wrong across most of its domain; redundancy is only worth arguing
about above a correct baseline. Every one of the 59 variants now has a
deliberate rendering and there is no `_`, so a new `Tok` is a compile error
here rather than another silent drop (DD#116, property 2). Exactly two
renderings are empty, both deliberate: a comment is trivia, and `Eof` has no
text.

The four paren arms survive as ordinary members of a total match. Their row in
DD#116's precedent table now reads *keep* — subsumed rather than deleted.

31 of the 32 probed spellings are now rejected. The exception is `;;`, and it
is not an exception: **DD#94** makes `(( a; b ))` a sanctioned sequence, so
`;;` reading as two empty separators is consistent with the `;` that was
already accepted. The one case my change flipped, `(( x = 1 ;; 2 ))`, moved
from rejected to accepted for that reason.

A comment repaired itself. `parse_arith_str` has claimed for a long time that
the `for (( ;; ))` header "reaches here as `;;`". It did not — it arrived as
`""`, because `;;` was one of the 35. The fix made the comment true. That is
the fifth confident-but-wrong comment this week, and the only one so far that
was fixed by correcting the code instead of the prose.

## What the specs pin

`31.64`-`31.67` pin the rejection in both the statement and expansion forms,
for a keyword, a redirect operator and an ANSI-C quote. `31.68` is the shield
for the one token that legitimately reaches the renderer: `for (( ;; ))` must
stay a legitimately empty expression.

## The sweep found a second bug

DG asked for the other content collectors to be probed before committing.
`[[ ]]` and array subscripts are clean — SB4014 and SB4041. Two sites are not.
The C-style `for` header shares the arithmetic collector and is fixed with it.
The substring offset is a separate mechanism and is still broken:

```
${s:1 2}     sash: abcdef     bash: syntax error
${s:1x}      sash: abcdef     bash: syntax error
${s:1+1}     sash: abcdef     bash: cdef
${s:0:n}     sash: (empty)    bash: ab
```

`emit_substring` (`codegen/value_handlers.rs:403`) and `emit_substring_c`
(`emit_c.rs:3184`) both parse the offset with `parse::<i64>().unwrap_or(0)` at
codegen time. `${s:zzz}` is caught upstream by SB4047, but anything that
starts with a digit gets past it and then silently becomes 0. Both backends
agree, so the differential oracle cannot see it.

This one is worse than the first: the first accepted a program whose value
happened to be right, this one emits a **wrong value** for a program bash
rejects outright. Awaiting DG on whether the offset should be rejected or
should accept arithmetic as bash does.

## Method

Three things that read identically in a coverage report — a bug, a spec that
missed its target, and dead code — were all present in one 30-line function.
Only execution separates them. The rival test would have produced a tidy patch
and left the bug in place.

**Before asking whether a branch is a rival, check that the function it sits
in is right.**

Sweep **5883** (5870 passing + 13 skipped), 0 failing, **0 flips vs
`6dd9e90`**, no retirements. Predicted 5883. Units 51/27/20.
