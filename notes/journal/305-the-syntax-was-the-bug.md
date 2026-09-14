title: 305 — the syntax was the bug

I filed `bug-array-regex-filter-silent-literal` during the test redux with two
proposed fixes, both of which were wrong, and presented them to DG as the
choice. DG picked neither.

The bug: `array a=(*.ff =~ 'RE')` silently yields three un-globbed literal
words. `try_array_regex_filter` required element 0 to be *exactly* `"*"`, so
any real glob pattern fell through to a plain indexed array with no
diagnostic. My two options were (a) generalize the guard to any glob
metacharacter, (b) diagnose with a new SB code.

DG's first reply reframed it:

> seems like this should just be sugar for `array a; for f in *.ff { [[ $f =~
> RE { a += ($f) } ]] }`

That reads as option (a) with the semantics spelled out, and I started
implementing it. Two probes in, it stopped being option (a).

## What measurement kept turning up

**Quoting was invisible.** I checked whether quoting could distinguish the
filter form from a literal array, because DG had said quotes should prevent
the desugaring and "we don't want to reinvent bash's alias stuff". All four
spellings produced identical output:

```text
array a=(*  =~  're')   → 2
array a=(*  '=~' 're')  → 2
array a=('*' =~  're')  → 2
array a=("*" =~  're')  → 2
```

The desugar ran on lowered `SsaValue`s, by which point `Literal("=~")` is all
that survives of any of them. So the escape hatch did not merely need wiring
— making it work would mean **quoting changes a list's arity and meaning**,
not just a value. That is exactly the bash trap DG named.

**The form could not say what it matched.** The runtime tested the regex
against the *bare filename*; the elements it produced were DD#73 *absolute
paths*. Both are defensible, the syntax names neither, and DG's own loop
sketch implies the path (`$f` is absolute) while the implementation used the
name. I could not resolve it from the syntax because the syntax has no place
to put the answer.

**Arity was silent.** `(* =~ a)` filtered; `(* =~ a b)` was a four-element
literal. Same shape, no diagnostic, different meaning.

So I went back to DG with the thing I should have led with: is the syntax
right? DG: *"do we need a better syntax?"* — then, given the options, **drop
it**.

The clinching fact was one I had already measured without noticing its
significance: the loop works today. `array a; for f in *.ff { if [[ $f =~
'RE' ]] { a += ($f) } }` compiles and filters. The shorthand was convenience,
not capability, which makes removing it nearly free.

## The correction I most needed

On my proposal to give `sash_glob_regex` a second pattern argument:

> I don't understand the bit about sash_glob_regex needing another argument -
> the function name itself seems a contradiction...

Right, and it exposed that I had stopped reading my own design. A function
that both globs and regexes is two pattern languages doing one job; journal
042 had settled "one pattern engine" years of commits ago. My fix would have
taken a badly-named function and added the argument that made the name
actively false. The function is now deleted, and `sash_glob(dir, pattern)` is
the single glob primitive.

This is the second time this stretch that my instinct was to *extend* the
thing that was wrong rather than ask whether it should exist. The first was
proposing to rehabilitate 546 Rust tests.

## What landed

`arr=(GLOB =~ 're')` is **SB5075**, DD#108. The rule is uniform: a bare `=~`
anywhere in an array literal is rejected. The narrower rule — reject only when
element 0 holds a glob metacharacter — would have re-introduced the
content-dependent reading being removed, and would mean `(x =~ y)` and
`(x* =~ y)` differ in kind for a reason invisible at the callsite.

A **quoted** `=~` stays an ordinary element: `arr=(x '=~' y)` is a
three-element array. That is DG's quoting principle honoured, and it is what
lets the rule be uniform — there is always a way to say "I meant the string".

Five specs, `glob.sh` 11 → 14. `g.11` is the filed bug's exact form, now
loud. `g.14` compiles the loop the diagnostic advises, because a diagnostic
whose advice does not compile is a bug I have already filed once this month.

## A false alarm, and a real nit

I reported `argv [N] a` as an ICE. DG: *"are you putting literal brackets in
the argv? I just did argv 2 a and it works"*. Yes, I was. `[N]` in SB5025's
message is placeholder notation and I read it as syntax — helped along by the
message rendering it inside backticks, as code. `argv 2 a` works fine.

Two small things survive that: the backtick rendering invites exactly my
misreading, and malformed `argv [2] a` compiles to a bare-command exec and
ICEs at runtime instead of being rejected. Both filed, neither urgent.

## The pattern, again

Third session running: **re-measure before implementing**. But this one had a
sharper edge than "the filing was stale". The filing was accurate — the bug
was real and correctly described. What was wrong was the *frame*: I had
assumed the guard was the defect and offered DG two ways to fix a guard. Two
probes showed the guard was faithfully implementing a syntax that could not be
made coherent. Presenting two repair options for something that should be
deleted is narrowing the choice before DG sees it, which is the failure mode
the parser-migration postmortem exists to prevent.

The tell I missed: when both of my options felt unsatisfying, that was
information about the question, not about the options.
